use async_trait::async_trait;
use derive_more::Display;
use futures::{channel::mpsc::UnboundedSender, lock::Mutex as AsyncMutex, SinkExt, StreamExt};
use log::debug;
use std::{
    ops::DerefMut,
    sync::{Arc, Weak},
    time::Duration,
};
use tokio::{
    io::{ReadHalf, WriteHalf},
    net::TcpStream,
    select, spawn,
    task::JoinHandle,
};
use tokio_serde::{formats::*, SymmetricallyFramed};
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};

use super::rpc_data::PeerMessage;

const PEER_BUFFER_SIZE: usize = 10;
const RELEASE_PEER_READ_TIMEOUT_MILLIS: u64 = 300;

#[derive(Debug, Display)]
#[display(fmt = "{}. conn_id: {}", error, conn_id)]
pub struct PeerError {
    pub conn_id: u64,
    error: PeerErrorImpl,
}

#[derive(Debug, Display)]
pub enum PeerErrorImpl {
    #[display(fmt = "Failed to send peer_event: {}", _0)]
    ChannelError(String),
    #[display(fmt = "Failed to receive message: {}", _0)]
    RecvError(String),
    #[display(fmt = "Failed to send message: {}", _0)]
    SendError(String),
    #[display(fmt = "Related peer is not existing anymore")]
    PeerNotAvailable,
}

impl PeerError {
    fn new(conn_id: u64, error: PeerErrorImpl) -> PeerError {
        PeerError { conn_id, error }
    }
}

pub enum PeerEvent {
    RandomMessage { conn_id: u64, data: String },
    PublicAddress { conn_id: u64, address: String },
    ListOfPeers { conn_id: u64, peers: Vec<String> },
    Disconnected { conn_id: u64 },
}

#[async_trait]
pub trait PeerInteractor: Send + Sync {
    fn start_listen_messages(
        &self,
        event_tx: UnboundedSender<PeerEvent>,
    ) -> JoinHandle<Result<(), PeerError>>;
    async fn send_random_message(&self) -> Result<(), PeerError>;
    fn send_public_port(&self, public_port: u16) -> JoinHandle<Result<(), PeerError>>;
    fn get_id(&self) -> u64;
    fn get_address(&self) -> &str;
}

pub(super) struct PeerInteractorImpl {
    id: u64,
    address: String,
    read: Arc<AsyncMutex<ReadHalf<TcpStream>>>,
    write: Arc<AsyncMutex<WriteHalf<TcpStream>>>,
}

#[async_trait]
impl PeerInteractor for PeerInteractorImpl {
    fn start_listen_messages(
        &self,
        event_tx: UnboundedSender<PeerEvent>,
    ) -> JoinHandle<Result<(), PeerError>> {
        let address = self.address.clone();
        debug!("Start listen for new messages on address: {}", address);
        let weak_read = Arc::downgrade(&self.read);
        let conn_id = self.id;
        spawn(PeerInteractorImpl::recv_messages_loop(conn_id, event_tx, weak_read))
    }

    async fn send_random_message(&self) -> Result<(), PeerError> {
        Ok(())
    }

    fn send_public_port(&self, public_port: u16) -> JoinHandle<Result<(), PeerError>> {
        debug!("Send public_port: {}", public_port);
        let weak_write = Arc::downgrade(&self.write);
        let conn_id = self.id;
        spawn(async move {
            let write = weak_write
                .upgrade()
                .ok_or_else(|| PeerError::new(conn_id, PeerErrorImpl::PeerNotAvailable))?;
            let mut write = write.lock().await;
            let framed_write = FramedWrite::new(write.deref_mut(), LengthDelimitedCodec::new());
            let mut sender = SymmetricallyFramed::new(framed_write, SymmetricalJson::default());
            let message = PeerMessage::PublicAddress { port: public_port };
            sender.send(message).await.map_err(|error| {
                PeerError::new(conn_id, PeerErrorImpl::SendError(error.to_string()))
            })
        })
    }

    fn get_id(&self) -> u64 {
        self.id
    }

    fn get_address(&self) -> &str {
        self.address.as_str()
    }
}

impl PeerInteractorImpl {
    pub fn new(id: u64, address: String, stream: TcpStream) -> PeerInteractorImpl {
        let (read, write) = tokio::io::split(stream);
        PeerInteractorImpl {
            id,
            address,
            read: Arc::new(AsyncMutex::new(read)),
            write: Arc::new(AsyncMutex::new(write)),
        }
    }

    fn process_message(
        conn_id: u64,
        message: Option<Result<PeerMessage, std::io::Error>>,
        event_tx: &UnboundedSender<PeerEvent>,
    ) -> Result<bool, PeerError> {
        let Some(message) = message else {
            event_tx.unbounded_send(PeerEvent::Disconnected { conn_id }).map_err(|error| PeerError::new(conn_id, PeerErrorImpl::ChannelError(error.to_string())))?;
            return Ok(false)
        };
        let peer_message = message.map_err(|error| {
            PeerError::new(conn_id, PeerErrorImpl::RecvError(error.to_string()))
        })?;
        let peer_event = PeerEvent::from_message(conn_id, peer_message);
        event_tx.unbounded_send(peer_event).map_err(|error| {
            PeerError::new(conn_id, PeerErrorImpl::ChannelError(error.to_string()))
        })?;
        Ok(true)
    }

    async fn recv_messages_loop(
        conn_id: u64,
        peer_event_tx: UnboundedSender<PeerEvent>,
        weak_read: Weak<AsyncMutex<ReadHalf<TcpStream>>>,
    ) -> Result<(), PeerError> {
        while let Some(arc_read) = weak_read.upgrade() {
            let mut read = arc_read.lock().await;
            let framed_read = FramedRead::with_capacity(
                read.deref_mut(),
                LengthDelimitedCodec::new(),
                PEER_BUFFER_SIZE,
            );
            let mut decoder =
                SymmetricallyFramed::new(framed_read, SymmetricalJson::<PeerMessage>::default());
            loop {
                select!(
                    message = decoder.next() => {
                        match PeerInteractorImpl::process_message(conn_id, message, & peer_event_tx)? {
                            true => {},
                            false => return Ok(()),
                        }
                    },
                    _ = tokio::time::sleep(Duration::from_millis(RELEASE_PEER_READ_TIMEOUT_MILLIS)) => { break }
                )
            }
        }
        Ok(())
    }
}

impl PeerEvent {
    fn from_message(conn_id: u64, message: PeerMessage) -> PeerEvent {
        debug!("Got message {:?}", message);
        match message {
            PeerMessage::PublicAddress { port } => PeerEvent::PublicAddress {
                conn_id,
                address: port.to_string(),
            },
            PeerMessage::RandomMessage { data } => PeerEvent::RandomMessage { conn_id, data },
        }
    }
}
