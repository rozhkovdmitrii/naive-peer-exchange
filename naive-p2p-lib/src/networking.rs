use async_trait::async_trait;
use derive_more::Display;
use futures::{
    channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender},
    lock::Mutex as AsyncMutex,
    SinkExt,
};
use log::{debug, info};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use tokio::{
    net::{TcpListener, TcpStream},
    spawn,
    task::JoinHandle,
};

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
    BindServer {
        interface: String,
        port: u16,
        error: String,
    },
    #[display(fmt = "Failed to accept incoming connection: {}", _0)]
    Accept(String),
    #[display(fmt = "Failed to connect to: {}, error: {}", address, error)]
    Connecting { address: String, error: String },
    #[display(fmt = "Failed to pass a new peer connection over the channel: {}", _0)]
    EventChannel(String),
}

pub enum NetworkEvent {
    Accepted(Box<dyn PeerInteractor + Send>),
    Connected(Box<dyn PeerInteractor + Send>),
}

#[async_trait]
pub trait Networking: Send + Sync {
    async fn get_network_event_rx(&self) -> Option<UnboundedReceiver<NetworkEvent>>;
    async fn connect_to(&self, peer_address: String) -> JoinHandle<Result<(), NetworkError>>;
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

    async fn connect_to(&self, peer_address: String) -> JoinHandle<Result<(), NetworkError>> {
        self.connect_to(peer_address).await
    }

    async fn accept_connections(&self, port: u16) -> Result<(), NetworkError> {
        let event_tx = { self.event_tx.lock().await.clone() };
        Self::accept_connections(self.id_counter.clone(), port, event_tx).await
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
            TcpListener::bind((ip, port)).await.map_err(|err| NetworkError::BindServer {
                interface: ip.to_string(),
                port,
                error: err.to_string(),
            })?;
        info!("My address is: {}:{}", ip, port);
        loop {
            debug!("Wait for the new connection on port:  {}", port);
            let (stream, address) =
                server.accept().await.map_err(|error| NetworkError::Accept(error.to_string()))?;
            debug!("Successfully accepted connection: {}", address);
            let id = id_counter.fetch_add(1, Ordering::Release);
            tx.send(NetworkEvent::Accepted(Box::new(PeerInteractorImpl::new(
                id,
                address.to_string(),
                stream,
            ))))
            .await
            .map_err(|error| NetworkError::EventChannel(error.to_string()))?;
            debug!("Accepted connection sent through the channel: {}", address);
        }
    }

    async fn connect_to(&self, address: String) -> JoinHandle<Result<(), NetworkError>> {
        let new_id = self.id_counter.fetch_add(1, Ordering::Release);
        let mut event_tx = { self.event_tx.lock().await.clone() };
        spawn(async move {
            let stream =
                TcpStream::connect(&address).await.map_err(|error| NetworkError::Connecting {
                    address: address.clone(),
                    error: error.to_string(),
                })?;
            let peer = Box::new(PeerInteractorImpl::new(new_id, address, stream));
            event_tx
                .send(NetworkEvent::Connected(peer))
                .await
                .map_err(|error| NetworkError::EventChannel(error.to_string()))
        })
    }
}
