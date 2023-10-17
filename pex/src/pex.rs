use futures::channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};
use futures::lock::Mutex as AsyncMutex;
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use log::{debug, error, info};
use std::sync::Arc;
use std::time::Duration;
use tokio::select;
use tokio::task::{JoinError, JoinHandle};

use crate::networking::Networking;
use crate::peer_interactor::PeerEvent;
use crate::peer_keeper::PeerKeeper;
use crate::{NetworkError, NetworkEvent, PeerError, PeerExchangeConfig, PeerInteractor};

const CHECK_PEER_RESULTS_TIMEOUT_MILLIS: u64 = 10;

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
            _ = self.process_new_connections(network_event_rx, peer_event_tx) => {},
            _ = self.peers_keeper.execute() => {},
            r = self.networking.accept_connections(self.config.port) => { if let Err(error) = r { error!("Accepting connections unexpectedly interrupted: {}", error) }},
            _ = self.process_peer_events(peer_event_rx) => {}
            r = self.check_peer_results() => { if let Err(error) = r { error!("Checking peer results unexpectedly interrupted: {}", error); } }
            r = self.check_connect_results() => { if let Err(error) = r { error!("Checking connecting results unexpectedly interrupted: {}", error); } }
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
                    self.on_peer_public_address(conn_id, address).await;
                }
                PeerEvent::RandomMessage { conn_id, data } => {
                    self.on_random_message(conn_id, data).await
                }
                PeerEvent::ListOfPeers { conn_id, peers } => {
                    self.on_list_of_peers(conn_id, peers).await
                }
                PeerEvent::Disconnected { conn_id } => self.on_peer_disconnected(conn_id).await,
            }
        }
    }

    async fn check_peer_results(&self) -> Result<(), JoinError> {
        loop {
            let handle = { self.peer_handles.lock().await.next().await };
            let Some(handle) = handle else {
                tokio::time::sleep(Duration::from_millis(CHECK_PEER_RESULTS_TIMEOUT_MILLIS)).await;
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
            let handle = { self.connect_handles.lock().await.next().await };
            let Some(handle) = handle else {
                tokio::time::sleep(Duration::from_millis(CHECK_PEER_RESULTS_TIMEOUT_MILLIS)).await;
                continue;
            };
            match handle? {
                Ok(()) => {}
                Err(connect_error) => self.on_connect_error(connect_error).await,
            }
        }
    }

    async fn on_netwrok_event(
        &self,
        network_event: NetworkEvent,
        peer_event_tx: UnboundedSender<PeerEvent>,
    ) {
        match network_event {
            NetworkEvent::Connected(peer) => {
                let peer: Arc<dyn PeerInteractor + Send> = peer.into();
                let conn_id = peer.get_id();
                let address = peer.get_address().to_string();
                let listen_handle = peer.start_listen_messages(peer_event_tx.clone());
                self.peer_handles.lock().await.push(listen_handle);
                let send_handle =
                    peer.send_public_port(self.config.address.clone(), self.config.port);
                self.peer_handles.lock().await.push(send_handle);
                self.peers_keeper.on_new_peer(peer).await;
                self.peers_keeper.on_peer_public_address(conn_id, address).await;
            }
            NetworkEvent::Accepted(peer) => {
                let peer: Arc<dyn PeerInteractor + Send> = peer.into();
                let listen_handle = peer.start_listen_messages(peer_event_tx.clone());
                self.peer_handles.lock().await.push(listen_handle);
                self.peers_keeper.on_new_peer(peer).await;
            }
        }
    }

    async fn on_peer_public_address(&self, conn_id: u64, address: String) {
        self.peers_keeper.on_peer_public_address(conn_id, address).await
    }

    async fn on_peer_disconnected(&self, conn_id: u64) {
        self.peers_keeper.on_peer_disconnected(conn_id).await
    }

    async fn on_random_message(&self, conn_id: u64, message_data: String) {
        info!("Got message_data, conn_id: {}, data: {}", conn_id, message_data);
    }

    async fn on_list_of_peers(&self, conn_id: u64, peers: Vec<String>) {
        debug!("Got list of peers, conn_id: {}, peers: {:?}", conn_id, peers);
        for peer_address in peers {
            if self.peers_keeper.is_address_known(&peer_address).await {
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
        self.peers_keeper.get_peers().await
    }
}
