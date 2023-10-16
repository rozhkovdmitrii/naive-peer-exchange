#![feature(map_try_insert)]

mod networking;
mod peer_interactor;
mod peer_keeper;
mod pex;
mod rpc_data;

pub use crate::pex::PeerExchange;
pub use networking::{NetworkError, NetworkEvent, Networking, NetworkingImpl};
pub use peer_interactor::{PeerError, PeerInteractor};

#[derive(Debug)]
pub struct PeerConfig {
    pub messaging_timeout_sec: u64,
    pub init_peer: Option<String>,
    pub port: u16,
}
