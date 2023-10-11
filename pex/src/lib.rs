mod networking;
mod peer_client;
mod peer_interactor;
mod peer_server;
mod pex;
mod rpc_data;

pub use pex::Pex;

#[derive(Debug)]
pub struct Config {
    pub messaging_timeout_sec: u64,
    pub port: u16,
    pub peers: Vec<String>,
}
