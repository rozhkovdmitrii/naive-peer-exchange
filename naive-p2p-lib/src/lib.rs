#![feature(drain_filter)]
#![feature(map_try_insert)]

mod naive_peer;
mod networking;
mod peer_interactor;
mod peer_mng;
mod rpc_data;

pub use crate::naive_peer::NaivePeer;
pub use networking::{NetworkError, NetworkEvent, Networking, NetworkingImpl};
pub use peer_interactor::{PeerError, PeerInteractor};

#[derive(Debug)]
/// Determines common settings to be applied to the peer exchange instance
pub struct NaivePeerConfig {
    /// Public address could differ from the local interface that is accepting new connections
    pub address: String,
    /// Public port to accept new connections on
    pub port: u16,
    /// Timeout that is used to determine in which moments random messages should be sent
    pub messaging_timeout_sec: u64,
    /// Initial peer to be connected by the current one at start
    pub init_peer: Option<String>,
}

impl NaivePeerConfig {
    pub fn new<A: ToString>(
        address: A,
        port: u16,
        messaging_timeout_sec: u64,
        init_peer: Option<A>,
    ) -> NaivePeerConfig {
        NaivePeerConfig {
            address: address.to_string(),
            port,
            messaging_timeout_sec,
            init_peer: init_peer.map(|address| address.to_string()),
        }
    }
}
