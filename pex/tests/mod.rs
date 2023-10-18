use log::LevelFilter;
use pex::{NetworkingImpl, PeerExchange, PeerExchangeConfig};
use std::iter::once;
use std::sync::Arc;
use std::time::Duration;
use tokio::spawn;

fn init_logging() {
    let mut builder = env_logger::builder();
    let level = std::env::var("RUST_LOG")
        .map(|s| s.parse().expect("Failed to parse RUST_LOG"))
        .unwrap_or(LevelFilter::Off);
    builder.filter_level(level).format_level(true).format_target(true);
    builder.init();
}

#[tokio::test]
async fn test_couple_peers_network() {
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
    tokio::time::sleep(Duration::from_millis(10)).await;
    assert_eq!(peer2.get_peers().await, vec!["127.0.0.1:8080"]);
    assert_eq!(peer1.get_peers().await, vec!["127.0.0.1:8081"]);
}

#[tokio::test]
async fn test_a_few_peers_network() {
    let peer1 = Arc::new(PeerExchange::new(
        PeerExchangeConfig::new("127.0.0.1", 8082, 2, None),
        <Box<NetworkingImpl>>::default(),
    ));
    let peer1_copy = peer1.clone();

    let peer2 = Arc::new(PeerExchange::new(
        PeerExchangeConfig::new("127.0.0.1", 8083, 1, Some("127.0.0.1:8082")),
        <Box<NetworkingImpl>>::default(),
    ));
    let peer2_copy = peer2.clone();

    let peer3 = Arc::new(PeerExchange::new(
        PeerExchangeConfig::new("127.0.0.1", 8084, 1, Some("127.0.0.1:8083")),
        <Box<NetworkingImpl>>::default(),
    ));
    let peer3_copy = peer3.clone();

    let peer4 = Arc::new(PeerExchange::new(
        PeerExchangeConfig::new("127.0.0.1", 8085, 1, Some("127.0.0.1:8083")),
        <Box<NetworkingImpl>>::default(),
    ));
    let peer4_copy = peer4.clone();

    let peer5 = Arc::new(PeerExchange::new(
        PeerExchangeConfig::new("127.0.0.1", 8086, 1, Some("127.0.0.1:8085")),
        <Box<NetworkingImpl>>::default(),
    ));
    let peer5_copy = peer5.clone();

    spawn(async move { peer1_copy.execute().await });
    spawn(async move { peer2_copy.execute().await });
    spawn(async move { peer3_copy.execute().await });
    spawn(async move { peer4_copy.execute().await });
    spawn(async move { peer5_copy.execute().await });
    tokio::time::sleep(Duration::from_millis(10)).await;

    assert_eq!(
        peer1.get_peers().await,
        vec![
            "127.0.0.1:8083",
            "127.0.0.1:8084",
            "127.0.0.1:8085",
            "127.0.0.1:8086"
        ]
    );
    assert_eq!(
        peer2.get_peers().await,
        vec![
            "127.0.0.1:8082",
            "127.0.0.1:8084",
            "127.0.0.1:8085",
            "127.0.0.1:8086"
        ]
    );
    assert_eq!(
        peer3.get_peers().await,
        vec![
            "127.0.0.1:8082",
            "127.0.0.1:8083",
            "127.0.0.1:8085",
            "127.0.0.1:8086"
        ]
    );
}

#[tokio::test]
async fn test_many_peers_network() {
    init_logging();
    tokio::time::sleep(Duration::from_secs(1)).await;
    let ports_range = 8090..8120;
    let ports: Vec<u16> = ports_range.into_iter().collect();
    let base = Arc::new(PeerExchange::new(
        PeerExchangeConfig::new("127.0.0.1", 8080, 1, None),
        <Box<NetworkingImpl>>::default(),
    ));
    let base_copy = base.clone();

    spawn(async move { base_copy.execute().await });
    let mut peers = vec![];
    for port in ports.iter().by_ref() {
        let peer = Arc::new(PeerExchange::new(
            PeerExchangeConfig::new("127.0.0.1", *port, 1, Some("127.0.0.1:8080")),
            <Box<NetworkingImpl>>::default(),
        ));
        peers.push(peer);
    }

    for peer in peers.iter().cloned() {
        tokio::time::sleep(Duration::from_millis(1)).await;
        spawn(async move { peer.execute().await });
    }
    tokio::time::sleep(Duration::from_millis(500)).await;

    for (i, peer) in peers.iter().enumerate() {
        let expected: Vec<String> = once(&8080_u16)
            .chain(ports[..i].iter())
            .chain(ports[i + 1..].iter())
            .map(|port| format!("127.0.0.1:{}", port))
            .collect();

        let _info = peer.get_peers_and_conn_ids().await;
        assert_eq!(peer.get_peers().await, expected);
    }
}
