use pex::{NetworkingImpl, PeerExchange, PeerExchangeConfig};

pub(super) struct PexApp {
    pex: PeerExchange,
}

impl PexApp {
    pub(super) fn new(config: PeerExchangeConfig) -> PexApp {
        PexApp {
            pex: PeerExchange::new(config, <Box<NetworkingImpl>>::default()),
        }
    }

    pub(super) async fn execute(self) {
        self.pex.execute().await;
    }
}
