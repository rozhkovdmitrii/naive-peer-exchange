use log::info;
use std::time::Duration;

use super::Config;
use crate::networking::{Networking, NetworkingImpl};
use crate::peer_interactor::PeerInteractor;

pub struct Pex {
    _messaging_timeout_sec: u64,
    _peers: Vec<Box<dyn PeerInteractor>>,
    _networking: Box<dyn Networking>,
}

impl Pex {
    pub fn new(config: Config) -> Pex {
        Pex {
            _messaging_timeout_sec: config.messaging_timeout_sec,
            _peers: vec![],
            _networking: Box::new(NetworkingImpl::new(config.port, config.peers)),
        }
    }

    pub async fn execute(&self) {
        let mut n = 1;
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            info!("Executing pex: {}", n);
            n = n + 1;
        }
    }
}
