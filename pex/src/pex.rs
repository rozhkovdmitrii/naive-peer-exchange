use futures::channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};
use futures::StreamExt;
use log::{error, info};
use std::sync::Arc;
use tokio::select;

use crate::networking::Networking;
use crate::peer_keeper::PeerKeeper;
use crate::{NetworkError, NetworkEvent, PeerConfig, PeerEvent, PeerInteractor};

pub struct Peer {
    config: PeerConfig,
    networking: Box<dyn Networking>,
    peers_keeper: PeerKeeper,
}

impl Peer {
    pub fn new(config: PeerConfig, networking: Box<dyn Networking>) -> Peer {
        Peer {
            config,
            networking,
            peers_keeper: PeerKeeper::new(),
        }
    }

    pub async fn execute(self) {
        let network_event_rx = self.networking.get_network_event_rx().await;
        let network_event_rx = network_event_rx.expect("Expected network event rx available");
        if let Err(error) = self.init_peer().await {
            error!("Failed to connect initial peer, break execution: {}", error);
            return;
        }

        let (peer_event_tx, peer_event_rx) = unbounded::<PeerEvent>();
        select!(
            _ = self.process_new_connections(network_event_rx, peer_event_tx) => {},
            _ = self.peers_keeper.execute() => {},
            _ = self.networking.start_listening(self.config.port) => {},
            _ = self.process_peer_events(peer_event_rx) => {}
        );
    }

    async fn process_new_connections(
        &self,
        mut network_event_rx: UnboundedReceiver<NetworkEvent>,
        peer_event_tx: UnboundedSender<PeerEvent>,
    ) {
        while let Some(NetworkEvent::NewPeer(peer_interactor)) = network_event_rx.next().await {
            let peer: Arc<dyn PeerInteractor + Send> = peer_interactor.into();
            peer.set_event_tx(peer_event_tx.clone()).await;
            self.peers_keeper.on_new_peer(peer.clone()).await;
            peer.start_listen_messages().await.unwrap();

            peer.send_public_port(self.config.port).await;
        }
    }

    async fn process_peer_events(&self, mut peer_event_rx: UnboundedReceiver<PeerEvent>) {
        while let Some(peer_event) = peer_event_rx.next().await {
            match peer_event {
                PeerEvent::PublicAddress { conn_id, address } => {
                    self.on_peer_public_address(conn_id, address).await;
                }
                PeerEvent::Message(_) => {
                    todo!()
                }
                PeerEvent::Disconnected => {
                    todo!()
                }
                PeerEvent::ListOfPeers(_) => {
                    todo!()
                }
            }
        }
    }

    async fn on_peer_public_address(&self, conn_id: u64, address: String) {
        self.peers_keeper.on_peer_public_address(conn_id, address).await
    }

    async fn init_peer(&self) -> Result<(), NetworkError> {
        let Some(address) = self.config.init_peer.as_ref() else {
            return Ok(());
        };
        info!("Connect to the initial peer: {}", address);
        self.connect_to(address).await
    }

    async fn connect_to(&self, address: &str) -> Result<(), NetworkError> {
        self.networking.connect_to(address).await?;

        info!("Connection established for address: {}", address);
        Ok(())
    }
}
