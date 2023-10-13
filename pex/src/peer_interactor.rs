use async_trait::async_trait;
use futures::channel::mpsc::UnboundedSender;
use futures::lock::Mutex as AsyncMutex;
use log::{debug, error, warn};
use serde::forward_to_deserialize_any;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt, ReadHalf, WriteHalf};
use tokio::net::TcpStream;
use tokio::{select, spawn};

use serde_json::{to_string, Deserializer, StreamDeserializer};

use super::rpc_data::NaiveExchangeMessage;

const PEER_BUFFER_SIZE: usize = 2048;
const PEER_READ_TIMEOUT_MILLIS: u64 = 2000;

#[derive(Debug)]
pub enum PeerError {
    NotInitialized,
}

pub enum PeerEvent {
    Message(String),
    PublicAddress { conn_id: u64, address: String },
    ListOfPeers(Vec<String>),
    Disconnected,
}

#[async_trait]
pub trait PeerInteractor: Send + Sync {
    async fn start_listen_messages(&self) -> Result<(), PeerError>;
    async fn set_event_tx(&self, event_tx: UnboundedSender<PeerEvent>);
    fn get_id(&self) -> u64;
    fn get_address(&self) -> &str;
    async fn send_random_message(&self) -> Result<(), PeerError>;
    async fn send_public_port(&self, public_port: u16);
}

pub(super) struct PeerInteractorImpl {
    id: u64,
    address: String,
    read: Arc<AsyncMutex<ReadHalf<TcpStream>>>,
    write: Arc<AsyncMutex<WriteHalf<TcpStream>>>,
    event_tx: AsyncMutex<Option<UnboundedSender<PeerEvent>>>,
}

impl PeerInteractorImpl {
    pub fn new(id: u64, address: String, stream: TcpStream) -> PeerInteractorImpl {
        let (read, write) = tokio::io::split(stream);
        PeerInteractorImpl {
            id,
            address,
            read: Arc::new(AsyncMutex::new(read)),
            write: Arc::new(AsyncMutex::new(write)),
            event_tx: AsyncMutex::new(None),
        }
    }

    async fn process_message(&self, message: NaiveExchangeMessage) {}
}

#[async_trait]
impl PeerInteractor for PeerInteractorImpl {
    async fn start_listen_messages(&self) -> Result<(), PeerError> {
        let address = self.address.clone();
        debug!("Start listen for new messages");
        let weak_read = Arc::downgrade(&self.read);
        let tx = self.event_tx.lock().await.as_ref().unwrap().clone();
        let conn_id = self.id;
        spawn(async move {
            while let Some(arc_read) = weak_read.upgrade() {
                let mut read = arc_read.lock().await;
                let mut data = [0; PEER_BUFFER_SIZE];

                let mut len = select! {
                    readed = read.read(&mut data) => { readed.unwrap()} ,
                    _ = tokio::time::sleep(Duration::from_millis(PEER_READ_TIMEOUT_MILLIS)) => {
                        continue
                    }
                };
                //debug!("Read bytes: {}", len);
                let de = Deserializer::from_slice(&data[..len]).into_iter::<NaiveExchangeMessage>();
                for message in de {
                    let message = message.unwrap();
                    debug!("Got message {:?}", message);
                    let peer_vent = match message {
                        NaiveExchangeMessage::PublicAddress { port } => PeerEvent::PublicAddress {
                            address: port.to_string(),
                            conn_id,
                        },
                        _ => unimplemented!(),
                    };
                    tx.unbounded_send(peer_vent).unwrap()
                }
            }
            debug!("Connection no more available: {}, stop listen", address);
        });
        Ok(())
    }

    async fn set_event_tx(&self, event_tx: UnboundedSender<PeerEvent>) {
        if let Some(_) = self.event_tx.lock().await.replace(event_tx) {
            warn!("Existing peer event tx replaced with the new one");
        }
    }

    fn get_id(&self) -> u64 {
        self.id
    }

    fn get_address(&self) -> &str {
        self.address.as_str()
    }

    async fn send_random_message(&self) -> Result<(), PeerError> {
        Ok(())
    }

    async fn send_public_port(&self, public_port: u16) {
        debug!("Send public_port: {}", public_port);
        let weak_write = Arc::downgrade(&self.write);
        spawn(async move {
            let Some(write) = weak_write.upgrade() else {
                error!("Failed to send public_port, connection no more available: {}", public_port);
                return;
            };
            let mut write = write.lock().await;
            let message = NaiveExchangeMessage::PublicAddress { port: public_port };
            let message = serde_json::to_string(&message).unwrap();
            write.write_all(message.as_bytes()).await.unwrap();
            debug!("Message sent: {}", message);
        });
    }
}
