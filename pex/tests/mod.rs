use std::sync::Arc;
use std::time::Duration;
use tokio::spawn;

use pex::{NetworkingImpl, PeerExchange, PeerExchangeConfig};

#[tokio::test]
async fn test_single_connection() {
    let peer1 = Arc::new(PeerExchange::new(
        PeerExchangeConfig::new("127.0.0.1", 8080, 2, None),
        <Box<NetworkingImpl>>::default(),
    ));

    let peer2 = Arc::new(PeerExchange::new(
        PeerExchangeConfig::new("127.0.0.1", 8081, 1, Some("127.0.0.1:8080")),
        <Box<NetworkingImpl>>::default(),
    ));

    let peer1_copy = peer1.clone();
    spawn(async move { peer1_copy.execute().await });
    let peer2_copy = peer2.clone();
    spawn(async move { peer2_copy.execute().await });
    tokio::time::sleep(Duration::from_millis(1)).await;
    assert_eq!(peer1.get_peers().await, vec!["127.0.0.1:8081"]);
    assert_eq!(peer2.get_peers().await, vec!["127.0.0.1:8080"]);
}

#[tokio::test]
async fn test_triple_connection() {
    let peer1 = Arc::new(PeerExchange::new(
        PeerExchangeConfig::new("127.0.0.1", 8082, 2, None),
        <Box<NetworkingImpl>>::default(),
    ));

    let peer2 = Arc::new(PeerExchange::new(
        PeerExchangeConfig::new("127.0.0.1", 8083, 1, Some("127.0.0.1:8082")),
        <Box<NetworkingImpl>>::default(),
    ));

    let peer3 = Arc::new(PeerExchange::new(
        PeerExchangeConfig::new("127.0.0.1", 8084, 1, Some("127.0.0.1:8082")),
        <Box<NetworkingImpl>>::default(),
    ));

    let peer1_copy = peer1.clone();
    spawn(async move { peer1_copy.execute().await });
    let peer2_copy = peer2.clone();
    spawn(async move { peer2_copy.execute().await });
    let peer3_copy = peer3.clone();
    spawn(async move { peer3_copy.execute().await });

    tokio::time::sleep(Duration::from_millis(1)).await;
    assert_eq!(peer1.get_peers().await, vec!["127.0.0.1:8083", "127.0.0.1:8084"]);
    assert_eq!(peer2.get_peers().await, vec!["127.0.0.1:8082", "127.0.0.1:8084"]);
    assert_eq!(peer3.get_peers().await, vec!["127.0.0.1:8082", "127.0.0.1:8083"]);
}
