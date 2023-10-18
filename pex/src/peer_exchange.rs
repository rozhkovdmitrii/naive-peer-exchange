use futures::channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};
use futures::lock::Mutex as AsyncMutex;
use futures::stream::{FuturesUnordered, StreamExt};
use log::{debug, error, info, warn};
use std::sync::Arc;
use std::time::Duration;
use tokio::select;
use tokio::task::{JoinError, JoinHandle};

use crate::networking::Networking;
use crate::peer_interactor::PeerEvent;
use crate::peer_keeper::PeerKeeper;
use crate::{NetworkError, NetworkEvent, PeerError, PeerExchangeConfig, PeerInteractor};

const CHECK_TASK_RESULTS_TIMEOUT_MICROS: u64 = 10;

pub struct PeerExchange {
    config: PeerExchangeConfig,
    networking: Box<dyn Networking>,
    peers_keeper: PeerKeeper,
    peer_handles: AsyncMutex<FuturesUnordered<JoinHandle<Result<(), PeerError>>>>,
    connect_handles: AsyncMutex<FuturesUnordered<JoinHandle<Result<(), NetworkError>>>>,
}

impl PeerExchange {
    pub fn new(config: PeerExchangeConfig, networking: Box<dyn Networking>) -> PeerExchange {
        PeerExchange {
            config,
            networking,
            peers_keeper: PeerKeeper::new(),
            peer_handles: AsyncMutex::new(FuturesUnordered::default()),
            connect_handles: AsyncMutex::new(FuturesUnordered::default()),
        }
    }

    pub async fn execute(&self) {
        let network_event_rx = self.networking.get_network_event_rx().await;
        let network_event_rx = network_event_rx.expect("Expected network event rx available");
        self.init_peer().await;
        let (peer_event_tx, peer_event_rx) = unbounded::<PeerEvent>();
        select!(
            _ = self.process_new_connections(network_event_rx, peer_event_tx) => { error!("New connection processing unexpectedly interrupted") },
            _ = self.peers_keeper.execute() => { error!("Peer keeping processing unexpectedly interrupted") },
            r = self.networking.accept_connections(self.config.port) => { if let Err(error) = r { error!("Accepting connections unexpectedly interrupted: {}", error) }},
            _ = self.process_peer_events(peer_event_rx) => { error!("Peer events processing unexpectedly interrupted")}
            r = self.check_peer_results() => { if let Err(error) = r { error!("Checking peer results unexpectedly interrupted: {}", error); } }
            r = self.check_connect_results() => { if let Err(error) = r { error!("Checking connecting results unexpectedly interrupted: {}", error); } }
            _ = self.messaging_loop() => { error!("Messaging loop unexpectedly interrupted")}
        );
    }

    async fn process_new_connections(
        &self,
        mut network_event_rx: UnboundedReceiver<NetworkEvent>,
        peer_event_tx: UnboundedSender<PeerEvent>,
    ) {
        while let Some(network_event) = network_event_rx.next().await {
            self.on_netwrok_event(network_event, peer_event_tx.clone()).await;
        }
    }

    async fn process_peer_events(&self, mut peer_event_rx: UnboundedReceiver<PeerEvent>) {
        while let Some(peer_event) = peer_event_rx.next().await {
            match peer_event {
                PeerEvent::PublicAddress { conn_id, address } => {
                    self.on_public_address(conn_id, address).await;
                }
                PeerEvent::RandomMessage { conn_id, data } => {
                    self.on_random_message(conn_id, data).await
                }
                PeerEvent::KnownPeers { conn_id, peers } => {
                    self.on_konwn_peers(conn_id, peers).await
                }
                PeerEvent::Disconnected { conn_id } => self.on_peer_disconnected(conn_id).await,
            }
        }
    }

    async fn check_peer_results(&self) -> Result<(), JoinError> {
        loop {
            tokio::time::sleep(Duration::from_micros(CHECK_TASK_RESULTS_TIMEOUT_MICROS)).await;
            let mut peer_handles = self.peer_handles.lock().await;
            let handle = select! (
                handle = peer_handles.next() => handle,
                _ = tokio::time::sleep(Duration::from_micros(1)) => {continue}
            );
            // No more futures are currently running
            let Some(handle) = handle else {
                continue;
            };
            match handle? {
                Ok(()) => {}
                Err(peer_error) => self.on_peer_error(peer_error).await,
            }
        }
    }

    async fn check_connect_results(&self) -> Result<(), JoinError> {
        loop {
            tokio::time::sleep(Duration::from_micros(CHECK_TASK_RESULTS_TIMEOUT_MICROS)).await;
            let mut connect_handles = self.connect_handles.lock().await;
            let handle = select! (
                handle = connect_handles.next() => handle,
                _ = tokio::time::sleep(Duration::from_micros(1)) => {continue}
            );
            // No more futures are currently running
            let Some(handle) = handle else {
                continue;
            };
            match handle? {
                Ok(()) => {}
                Err(connect_error) => self.on_connect_error(connect_error).await,
            }
        }
    }

    async fn messaging_loop(&self) {
        let timeout_sec = self.config.messaging_timeout_sec;
        loop {
            tokio::time::sleep(Duration::from_secs(timeout_sec)).await;

            let conns = self.peers_keeper.get_valid_conns().await;
            for conn in conns {
                debug!(
                    "Send random message, address: {}, conn_id: {}",
                    conn.get_address(),
                    conn.get_id()
                );
                let send_handle = conn.send_random_message();
                self.peer_handles.lock().await.push(send_handle);
            }
        }
    }

    async fn on_netwrok_event(
        &self,
        network_event: NetworkEvent,
        peer_event_tx: UnboundedSender<PeerEvent>,
    ) {
        debug!("Got on the network channel, peer connected");
        match network_event {
            NetworkEvent::Connected(peer) => self.on_peer_connected(peer, peer_event_tx).await,
            NetworkEvent::Accepted(peer) => self.on_peer_accepted(peer, peer_event_tx).await,
        }
    }

    async fn on_peer_connected(
        &self,
        peer: Box<dyn PeerInteractor + Send>,
        peer_event_tx: UnboundedSender<PeerEvent>,
    ) {
        let peer: Arc<dyn PeerInteractor + Send> = peer.into();
        let conn_id = peer.get_id();
        let address = peer.get_address().to_string();
        debug!("peer connected, conn_id: {}, address: {}", conn_id, address);
        if let Err(error) = self.peers_keeper.on_new_connection(peer.clone()).await {
            error!("Failed to register connection: {}", error);
            return;
        }
        if let Err(error) = self.peers_keeper.set_peer_as_valid(conn_id, address).await {
            error!("Failed to set_peer_as_valid: {}", error);
            return;
        };
        let listen_handle = peer.start_listen_messages(peer_event_tx.clone());
        self.peer_handles.lock().await.push(listen_handle);

        let send_handle = peer.send_public_address(self.config.address.clone(), self.config.port);
        self.peer_handles.lock().await.push(send_handle);
    }

    async fn on_peer_accepted(
        &self,
        peer: Box<dyn PeerInteractor + Send>,
        peer_event_tx: UnboundedSender<PeerEvent>,
    ) {
        let peer: Arc<dyn PeerInteractor + Send> = peer.into();
        let conn_id = peer.get_id();
        let address = peer.get_address().to_string();
        debug!("peer accepted, conn_id: {}, address: {}", conn_id, address);
        if let Err(error) = self.peers_keeper.on_new_connection(peer.clone()).await {
            error!("{}", error);
            return;
        }
        let listen_handle = peer.start_listen_messages(peer_event_tx.clone());
        self.peer_handles.lock().await.push(listen_handle);
    }

    async fn on_public_address(&self, conn_id: u64, address: String) {
        // This address could be wrong and send by the malicious person that is why connection
        // should be proved by the negotiation procedure that is due to be implemented in the future
        if let Err(error) = self.peers_keeper.set_peer_as_valid(conn_id, address.clone()).await {
            error!("Failed process public address: {}", error);
            return;
        };
        let mut known_peers = self.peers_keeper.get_peers().await;
        if known_peers.is_empty() {
            return;
        }

        let conn = self.peers_keeper.get_connection(conn_id).await;
        let conn = conn.expect("Expected connection be able to be found");
        known_peers.drain_filter(|peer_address| peer_address.as_str() == address.as_str());
        let send_handle = conn.send_known_peers(known_peers);
        self.peer_handles.lock().await.push(send_handle)
    }

    async fn on_peer_disconnected(&self, conn_id: u64) {
        info!("Got disconnected event, conn_id: {}", conn_id);
        self.peers_keeper.on_peer_disconnected(conn_id).await
    }

    async fn on_random_message(&self, conn_id: u64, message_data: String) {
        info!("Got message_data, conn_id: {}, data: {}", conn_id, message_data);
    }

    async fn on_konwn_peers(&self, conn_id: u64, peers: Vec<String>) {
        debug!("Got list of peers, conn_id: {}, peers: {:?}", conn_id, peers);
        for peer_address in peers {
            if peer_address == self.self_address() {
                warn!(
                    "Address that is the same as current peer is listening on rejected: {}",
                    peer_address
                );
                continue;
            }
            if self.peers_keeper.is_address_known(&peer_address).await {
                debug!("Address known, continue: {}", peer_address);
                continue;
            }
            self.connect_to(peer_address).await;
        }
    }

    async fn on_peer_error(&self, peer_error: PeerError) {
        error!("{}", peer_error);
        self.peers_keeper.on_peer_error(peer_error.conn_id).await
    }

    async fn on_connect_error(&self, connect_error: NetworkError) {
        error!("{}", connect_error);
    }

    async fn init_peer(&self) {
        let Some(address) = self.config.init_peer.clone() else {
            return;
        };
        self.connect_to(address).await;
    }

    async fn connect_to(&self, address: String) {
        info!("Connecting to the peer: {}", address);
        let handle = self.networking.connect_to(address).await;
        self.connect_handles.lock().await.push(handle);
    }

    pub async fn get_peers(&self) -> Vec<String> {
        let mut peers = self.peers_keeper.get_peers().await;
        peers.sort();
        peers
    }

    pub async fn get_peers_and_conn_ids(&self) -> Vec<(u64, String)> {
        let mut peers = self.peers_keeper.get_peers_and_conn_ids().await;
        peers.sort();
        peers
    }

    fn self_address(&self) -> String {
        format!("{}:{}", self.config.address, self.config.port)
    }
}
