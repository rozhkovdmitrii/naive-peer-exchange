use pex::{NetworkingImpl, Peer, PeerConfig};

pub(super) struct PexApp {
    pex: Peer,
}

impl PexApp {
    pub(super) fn new(config: PeerConfig) -> PexApp {
        PexApp {
            pex: Peer::new(config, Box::new(NetworkingImpl::new())),
        }
    }

    pub(super) async fn execute(self) {
        self.pex.execute().await;
    }
}
