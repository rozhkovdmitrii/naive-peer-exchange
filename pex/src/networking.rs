use async_trait::async_trait;
use derive_more::Display;
use log::{error, info};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};

use futures::channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};
use futures::lock::Mutex as AsyncMutex;
use futures::SinkExt;

use crate::peer_interactor::{PeerInteractor, PeerInteractorImpl};

#[derive(Display)]
pub enum NetworkError {
    ListeningError(String),
    ConnectingError(String),
}

pub enum NetworkEvent {
    NewPeer(Box<dyn PeerInteractor + Send>),
}

#[async_trait]
pub trait Networking: Send + Sync {
    async fn get_network_event_rx(&self) -> Option<UnboundedReceiver<NetworkEvent>>;
    async fn connect_to(&self, peer_address: &str) -> Result<(), NetworkError>;
    async fn start_listening(&self, port: u16) -> Result<(), NetworkError>;
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
        let stream = TcpStream::connect(address)
            .await
            .map_err(|error| NetworkError::ConnectingError(error.to_string()))?;
        let new_id = self.id_counter.fetch_add(1, Ordering::Release);
        let interactor = Box::new(PeerInteractorImpl::new(new_id, address.to_string(), stream));
        let mut event_tx = self.event_tx.lock().await;
        event_tx
            .send(NetworkEvent::NewPeer(interactor))
            .await
            .map_err(|error| NetworkError::ConnectingError(error.to_string()))
    }

    async fn start_listening(&self, port: u16) -> Result<(), NetworkError> {
        Self::listen(self.id_counter.clone(), port, self.event_tx.lock().await.clone()).await
    }
}

impl NetworkingImpl {
    pub fn new() -> NetworkingImpl {
        let (event_tx, event_rx) = unbounded::<NetworkEvent>();
        NetworkingImpl {
            id_counter: Arc::new(AtomicU64::new(0)),
            event_tx: AsyncMutex::new(event_tx),
            event_rx: AsyncMutex::new(Some(event_rx)),
        }
    }

    async fn listen(
        id_counter: Arc<AtomicU64>,
        port: u16,
        mut tx: UnboundedSender<NetworkEvent>,
    ) -> Result<(), NetworkError> {
        let (ip, port) = ("0.0.0.0", port);
        let server = TcpListener::bind((ip, port))
            .await
            .map_err(|err| NetworkError::ListeningError(err.to_string()))?;
        info!("Bound tcp server: {}, {}", ip, port);

        loop {
            match server.accept().await {
                Ok((stream, address)) => {
                    info!("Successfully accepted connection: {}", address);
                    let id = id_counter.fetch_add(1, Ordering::Release);
                    if let Err(error) = tx
                        .send(NetworkEvent::NewPeer(Box::new(PeerInteractorImpl::new(
                            id,
                            address.to_string(),
                            stream,
                        ))))
                        .await
                    {
                        error!("Failed to push out peer interactor: {}, error: {}", address, error);
                        continue;
                    };
                }
                Err(error) => error!("Failed to accept tcp stream, error: {}", error),
            }
        }
    }
}
