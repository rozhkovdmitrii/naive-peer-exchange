#![feature(map_try_insert)]

mod networking;
mod peer_interactor;
mod peer_keeper;
mod pex;
mod rpc_data;

pub use crate::pex::Peer;
pub use networking::{NetworkError, NetworkEvent, Networking, NetworkingImpl};
pub use peer_interactor::{PeerError, PeerEvent, PeerInteractor};

#[derive(Debug)]
pub struct PeerConfig {
    pub messaging_timeout_sec: u64,
    pub init_peer: Option<String>,
    pub port: u16,
}
