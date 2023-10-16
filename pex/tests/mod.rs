use async_trait::async_trait;
use log::error;
use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;
use std::time::Duration;
use tokio::spawn;

use futures::channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};
use futures::lock::Mutex as AsyncMutex;
use futures::StreamExt;

use pex::{
    NetworkError, NetworkEvent, Networking, NetworkingImpl, PeerConfig, PeerError, PeerEvent,
    PeerExchange, PeerInteractor,
};

type TestConnReceiver = UnboundedReceiver<TestConnection>;

struct TestNetworking {
    address: String,
    registry: Arc<AsyncMutex<NetworkRegistry>>,
}

struct TestConnection(UnboundedSender<Vec<u8>>, UnboundedReceiver<Vec<u8>>);

#[derive(Default)]
struct NetworkRegistry {
    peers: HashMap<String, NetworkRegistryRow>,
}

struct NetworkRegistryRow {
    peer_sender: UnboundedSender<TestConnection>,
    connected_to: BTreeSet<String>,
}

impl TestNetworking {
    fn new(address: String, registry: Arc<AsyncMutex<NetworkRegistry>>) -> TestNetworking {
        TestNetworking { address, registry }
    }
}

struct TestPeerInteractor {
    connection: TestConnection,
}

#[tokio::test]
async fn test_peer_are_listening() {
    let peer1 = PeerExchange::new(
        PeerConfig {
            messaging_timeout_sec: 4,
            init_peer: None,
            port: 8080,
        },
        Box::new(NetworkingImpl::new()),
    );

    let peer2 = PeerExchange::new(
        PeerConfig {
            messaging_timeout_sec: 4,
            init_peer: Some("127.0.0.1:8080".to_string()),
            port: 8082,
        },
        Box::new(NetworkingImpl::new()),
    );

    spawn(peer2.execute());
    spawn(peer1.execute());

    tokio::time::sleep(Duration::from_millis(100000)).await;
}
