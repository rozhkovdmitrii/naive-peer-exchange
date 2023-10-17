use async_trait::async_trait;
use derive_more::Display;
use futures::{
    channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender},
    lock::Mutex as AsyncMutex,
    SinkExt,
};
use log::info;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use tokio::net::{TcpListener, TcpStream};

use super::peer_interactor::{PeerInteractor, PeerInteractorImpl};

const INTERFACE_TO_LISTEN: &str = "0.0.0.0";

#[derive(Display)]
pub enum NetworkError {
    #[display(
        fmt = "Failed to bind server: {}:{}, error: {}",
        interface,
        port,
        error
    )]
    BindServerError {
        interface: String,
        port: u16,
        error: String,
    },
    #[display(fmt = "Failed to accept incoming connection: {}", _0)]
    AcceptError(String),
    #[display(fmt = "Failed to connect to: {}, error: {}", address, error)]
    ConnectingError { address: String, error: String },
    #[display(fmt = "Failed to pass a new peer connection over the channel: {}", _0)]
    ChannelError(String),
}

pub enum NetworkEvent {
    NewPeer(Box<dyn PeerInteractor + Send>),
}

#[async_trait]
pub trait Networking: Send + Sync {
    async fn get_network_event_rx(&self) -> Option<UnboundedReceiver<NetworkEvent>>;
    async fn connect_to(&self, peer_address: &str) -> Result<(), NetworkError>;
    async fn accept_connections(&self, port: u16) -> Result<(), NetworkError>;
}

pub struct NetworkingImpl {
    id_counter: Arc<AtomicU64>,
    event_tx: AsyncMutex<UnboundedSender<NetworkEvent>>,
    event_rx: AsyncMutex<Option<UnboundedReceiver<NetworkEvent>>>,
}

#[async_trait]
impl Networking for NetworkingImpl {
    async fn get_network_event_rx(&self) -> Option<UnboundedReceiver<NetworkEvent>> {
        self.event_rx.lock().await.take()
    }

    async fn connect_to(&self, address: &str) -> Result<(), NetworkError> {
        let stream =
            TcpStream::connect(address).await.map_err(|error| NetworkError::ConnectingError {
                address: address.to_string(),
                error: error.to_string(),
            })?;
        let new_id = self.id_counter.fetch_add(1, Ordering::Release);
        let interactor = Box::new(PeerInteractorImpl::new(new_id, address.to_string(), stream));
        let mut event_tx = self.event_tx.lock().await;
        event_tx
            .send(NetworkEvent::NewPeer(interactor))
            .await
            .map_err(|error| NetworkError::ChannelError(error.to_string()))
    }

    async fn accept_connections(&self, port: u16) -> Result<(), NetworkError> {
        Self::accept_connections(self.id_counter.clone(), port, self.event_tx.lock().await.clone())
            .await
    }
}

impl Default for NetworkingImpl {
    fn default() -> Self {
        let (event_tx, event_rx) = unbounded::<NetworkEvent>();
        NetworkingImpl {
            id_counter: Arc::new(AtomicU64::new(0)),
            event_tx: AsyncMutex::new(event_tx),
            event_rx: AsyncMutex::new(Some(event_rx)),
        }
    }
}

impl NetworkingImpl {
    async fn accept_connections(
        id_counter: Arc<AtomicU64>,
        port: u16,
        mut tx: UnboundedSender<NetworkEvent>,
    ) -> Result<(), NetworkError> {
        let (ip, port) = (INTERFACE_TO_LISTEN, port);
        let server =
            TcpListener::bind((ip, port)).await.map_err(|err| NetworkError::BindServerError {
                interface: ip.to_string(),
                port,
                error: err.to_string(),
            })?;
        info!("Bound tcp server: {}, {}", ip, port);
        loop {
            let (stream, address) = server
                .accept()
                .await
                .map_err(|error| NetworkError::AcceptError(error.to_string()))?;
            info!("Successfully accepted connection: {}", address);
            let id = id_counter.fetch_add(1, Ordering::Release);
            tx.send(NetworkEvent::NewPeer(Box::new(PeerInteractorImpl::new(
                id,
                address.to_string(),
                stream,
            ))))
            .await
            .map_err(|error| NetworkError::ChannelError(error.to_string()))?;
        }
    }
}
