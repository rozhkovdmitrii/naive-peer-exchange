use pex::{NetworkingImpl, PeerConfig, PeerExchange};

pub(super) struct PexApp {
    pex: PeerExchange,
}

impl PexApp {
    pub(super) fn new(config: PeerConfig) -> PexApp {
        PexApp {
            pex: PeerExchange::new(config, <Box<NetworkingImpl>>::default()),
        }
    }

    pub(super) async fn execute(self) {
        self.pex.execute().await;
    }
}
