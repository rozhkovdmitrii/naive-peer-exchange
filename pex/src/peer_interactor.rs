use derive_more::Display;
use futures::{channel::mpsc::UnboundedSender, lock::Mutex as AsyncMutex, SinkExt, StreamExt};
use log::{debug, error};
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
const RELEASE_PEER_READ_TIMEOUT_MILLIS: u64 = 10;

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
    KnownPeers { conn_id: u64, peers: Vec<String> },
    Disconnected { conn_id: u64 },
}

pub trait PeerInteractor: Send + Sync {
    fn start_listen_messages(
        &self,
        event_tx: UnboundedSender<PeerEvent>,
    ) -> JoinHandle<Result<(), PeerError>>;
    fn send_known_peers(&self, peers: Vec<String>) -> JoinHandle<Result<(), PeerError>>;
    fn send_random_message(&self) -> JoinHandle<Result<(), PeerError>>;
    fn send_public_address(&self, address: String, port: u16) -> JoinHandle<Result<(), PeerError>>;
    fn get_id(&self) -> u64;
    fn get_address(&self) -> &str;
}

pub(super) struct PeerInteractorImpl {
    id: u64,
    address: String,
    read: Arc<AsyncMutex<ReadHalf<TcpStream>>>,
    write: Arc<AsyncMutex<WriteHalf<TcpStream>>>,
}

impl PeerInteractor for PeerInteractorImpl {
    fn start_listen_messages(
        &self,
        event_tx: UnboundedSender<PeerEvent>,
    ) -> JoinHandle<Result<(), PeerError>> {
        let address = self.address.clone();
        let weak_read = Arc::downgrade(&self.read);
        let conn_id = self.id;
        spawn(Self::recv_messages_loop(conn_id, address, event_tx, weak_read))
    }

    fn send_known_peers(&self, peers: Vec<String>) -> JoinHandle<Result<(), PeerError>> {
        let address = self.address.clone();
        let weak_write = Arc::downgrade(&self.write);
        let conn_id = self.id;
        spawn(Self::send_known_peers_impl(conn_id, address, peers, weak_write))
    }

    fn send_random_message(&self) -> JoinHandle<Result<(), PeerError>> {
        let address = self.address.clone();
        let weak_write = Arc::downgrade(&self.write);
        let conn_id = self.id;
        spawn(Self::send_random_message_impl(conn_id, address, weak_write))
    }

    fn send_public_address(
        &self,
        public_address: String,
        public_port: u16,
    ) -> JoinHandle<Result<(), PeerError>> {
        let weak_write = Arc::downgrade(&self.write);
        let conn_id = self.id;
        spawn(PeerInteractorImpl::send_public_address_impl(
            conn_id,
            public_address,
            public_port,
            weak_write,
        ))
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
        peer_event_tx: &UnboundedSender<PeerEvent>,
    ) -> Result<bool, PeerError> {
        let Some(message) = message else {
            error!("!!!!!!!!!!!!!!!!!: {:?}", peer_event_tx);
            peer_event_tx.unbounded_send(PeerEvent::Disconnected { conn_id }).map_err(|error| PeerError::new(conn_id, PeerErrorImpl::ChannelError(error.to_string())))?;
            return Ok(false)
        };
        let peer_message = message.map_err(|error| {
            PeerError::new(conn_id, PeerErrorImpl::RecvError(error.to_string()))
        })?;
        let peer_event = PeerEvent::from_message(conn_id, peer_message);
        peer_event_tx.unbounded_send(peer_event).map_err(|error| {
            error!("ddddddddddddddddddddddddddddd");
            PeerError::new(conn_id, PeerErrorImpl::ChannelError(error.to_string()))
        })?;
        Ok(true)
    }

    async fn recv_messages_loop(
        conn_id: u64,
        address: String,
        peer_event_tx: UnboundedSender<PeerEvent>,
        weak_read: Weak<AsyncMutex<ReadHalf<TcpStream>>>,
    ) -> Result<(), PeerError> {
        debug!("Start listen for new messages on address: {}", address);
        loop {
            let Some(read) = weak_read.upgrade() else {
                break
            };
            let mut read = read.lock().await;
            let framed_read = FramedRead::with_capacity(
                read.deref_mut(),
                LengthDelimitedCodec::new(),
                PEER_BUFFER_SIZE,
            );
            let mut decoder =
                SymmetricallyFramed::new(framed_read, SymmetricalJson::<PeerMessage>::default());

            select!(
                message = decoder.next() => {
                    if !PeerInteractorImpl::process_message(conn_id, message, & peer_event_tx)? {
                        debug!("{}: Break listening for new messages", address);
                        break
                    }
                },
                _ = tokio::time::sleep(Duration::from_millis(RELEASE_PEER_READ_TIMEOUT_MILLIS)) => { continue }
            )
        }
        debug!("Recv data has finished processing");
        Ok(())
    }

    async fn send_public_address_impl(
        conn_id: u64,
        public_address: String,
        public_port: u16,
        weak_write: Weak<AsyncMutex<WriteHalf<TcpStream>>>,
    ) -> Result<(), PeerError> {
        debug!("Send public address impl, conn_id: {}, public_port: {}", conn_id, public_port);
        let message = PeerMessage::PublicAddress {
            address: format!("{}:{}", public_address, public_port),
        };
        Self::send_message(conn_id, message, weak_write).await
    }

    async fn send_known_peers_impl(
        conn_id: u64,
        address: String,
        known_peers: Vec<String>,
        weak_write: Weak<AsyncMutex<WriteHalf<TcpStream>>>,
    ) -> Result<(), PeerError> {
        debug!(
            "Send known peers, conn_id: {}, address: {}, known_peers: {:?}",
            conn_id, address, known_peers
        );
        let message = PeerMessage::KnownPeers { peers: known_peers };
        Self::send_message(conn_id, message, weak_write).await
    }

    async fn send_random_message_impl(
        conn_id: u64,
        address: String,
        weak_write: Weak<AsyncMutex<WriteHalf<TcpStream>>>,
    ) -> Result<(), PeerError> {
        debug!("Send random message, conn_id: {}, address: {}", conn_id, address);
        let message = PeerMessage::RandomMessage {
            data: "abcde".to_string(),
        };
        Self::send_message(conn_id, message, weak_write).await
    }

    async fn send_message(
        conn_id: u64,
        message: PeerMessage,
        weak_write: Weak<AsyncMutex<WriteHalf<TcpStream>>>,
    ) -> Result<(), PeerError> {
        let write = weak_write
            .upgrade()
            .ok_or_else(|| PeerError::new(conn_id, PeerErrorImpl::PeerNotAvailable))?;
        let mut write = write.lock().await;
        let framed_write = FramedWrite::new(write.deref_mut(), LengthDelimitedCodec::new());
        let mut sender = SymmetricallyFramed::new(framed_write, SymmetricalJson::default());
        sender
            .send(message)
            .await
            .map_err(|error| PeerError::new(conn_id, PeerErrorImpl::SendError(error.to_string())))
    }
}

impl PeerEvent {
    fn from_message(conn_id: u64, message: PeerMessage) -> PeerEvent {
        debug!("Got message {:?}", message);
        match message {
            PeerMessage::PublicAddress { address } => PeerEvent::PublicAddress { conn_id, address },
            PeerMessage::RandomMessage { data } => PeerEvent::RandomMessage { conn_id, data },
            PeerMessage::KnownPeers { peers } => PeerEvent::KnownPeers { conn_id, peers },
        }
    }
}
