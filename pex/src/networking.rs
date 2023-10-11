use async_trait::async_trait;
use tokio::sync::mpsc::UnboundedReceiver;

use crate::peer_interactor::PeerInteractor;

pub enum NetworkError {}

pub(super) enum NetworkEvent {
    _NewPeer(Box<dyn PeerInteractor>),
}

#[async_trait]
pub(super) trait Networking {
    async fn connect_to(&self, _peer_address: &str) -> Result<(), NetworkError>;
    async fn start_listening(&self) -> Result<UnboundedReceiver<NetworkEvent>, NetworkError>;
}

pub struct NetworkingImpl {
    _port: u16,
    _peers_to_connect: Vec<String>,
}

impl NetworkingImpl {
    pub fn new(port: u16, peers_to_connect: Vec<String>) -> NetworkingImpl {
        NetworkingImpl {
            _port: port,
            _peers_to_connect: peers_to_connect,
        }
    }
}

#[async_trait]
impl Networking for NetworkingImpl {
    async fn connect_to(&self, _peer_address: &str) -> Result<(), NetworkError> {
        Ok(())
    }

    async fn start_listening(&self) -> Result<UnboundedReceiver<NetworkEvent>, NetworkError> {
        todo!()
    }
}
