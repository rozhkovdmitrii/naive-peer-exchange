use pex::{Config, Pex};

pub(super) struct PexApp {
    _pex: Pex,
}

impl PexApp {
    pub(super) fn new(config: Config) -> PexApp {
        PexApp {
            _pex: Pex::new(config),
        }
    }

    pub(super) async fn execute(self) {
        self._pex.execute().await;
    }
}
