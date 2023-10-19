use naive_p2p_lib::{NaivePeer, NaivePeerConfig, NetworkingImpl};

pub(super) struct App {
    naive_peer: NaivePeer,
}

impl App {
    pub(super) fn new(config: NaivePeerConfig) -> App {
        App {
            naive_peer: NaivePeer::new(config, <Box<NetworkingImpl>>::default()),
        }
    }

    pub(super) async fn execute(self) {
        self.naive_peer.execute().await;
    }
}
