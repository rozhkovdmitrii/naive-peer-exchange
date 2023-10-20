use log::LevelFilter;
use rand::{thread_rng, Rng};
use std::iter::once;
use std::sync::Arc;
use std::time::Duration;
use tokio::spawn;

use naive_p2p_lib::{NaivePeer, NaivePeerConfig, NetworkingImpl};

fn init_logging() {
    let mut builder = env_logger::builder();
    let level = std::env::var("RUST_LOG")
        .map(|s| s.parse().expect("Failed to parse RUST_LOG"))
        .unwrap_or(LevelFilter::Off);
    builder.filter_level(level).format_level(true).format_target(true);
    let _ = builder.try_init();
}

#[tokio::test]
async fn test_couple_peers_network() {
    init_logging();
    let peer1 = Arc::new(NaivePeer::new(
        NaivePeerConfig::new("127.0.0.1", 8080, 2, None),
        <Box<NetworkingImpl>>::default(),
    ));

    let peer2 = Arc::new(NaivePeer::new(
        NaivePeerConfig::new("127.0.0.1", 8081, 1, Some("127.0.0.1:8080")),
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
    init_logging();
    let peer1 = Arc::new(NaivePeer::new(
        NaivePeerConfig::new("127.0.0.1", 8082, 2, None),
        <Box<NetworkingImpl>>::default(),
    ));
    let peer1_copy = peer1.clone();

    let peer2 = Arc::new(NaivePeer::new(
        NaivePeerConfig::new("127.0.0.1", 8083, 1, Some("127.0.0.1:8082")),
        <Box<NetworkingImpl>>::default(),
    ));
    let peer2_copy = peer2.clone();

    let peer3 = Arc::new(NaivePeer::new(
        NaivePeerConfig::new("127.0.0.1", 8084, 1, Some("127.0.0.1:8083")),
        <Box<NetworkingImpl>>::default(),
    ));
    let peer3_copy = peer3.clone();

    let peer4 = Arc::new(NaivePeer::new(
        NaivePeerConfig::new("127.0.0.1", 8085, 1, Some("127.0.0.1:8083")),
        <Box<NetworkingImpl>>::default(),
    ));
    let peer4_copy = peer4.clone();

    let peer5 = Arc::new(NaivePeer::new(
        NaivePeerConfig::new("127.0.0.1", 8086, 1, Some("127.0.0.1:8085")),
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
    const COUNT: u16 = 20;
    const BASE_PORT: u16 = 8089;
    let ports_range = BASE_PORT + 1..BASE_PORT + 1 + COUNT;
    let ports: Vec<u16> = ports_range.into_iter().collect();
    let base = Arc::new(NaivePeer::new(
        NaivePeerConfig::new("127.0.0.1", BASE_PORT, 1, None),
        <Box<NetworkingImpl>>::default(),
    ));
    let base_copy = base.clone();

    spawn(async move { base_copy.execute().await });
    let mut peers = vec![];
    let mut rng = thread_rng();
    for (i, port) in ports.iter().by_ref().enumerate() {
        let connect_port = if i == 0 {
            BASE_PORT
        } else {
            ports[rng.gen::<usize>() % i]
        };
        let connect_to = format!("127.0.0.1:{}", connect_port);
        let peer = Arc::new(NaivePeer::new(
            NaivePeerConfig::new("127.0.0.1", *port, 1, Some(&connect_to)),
            <Box<NetworkingImpl>>::default(),
        ));
        peers.push(peer);
    }

    #[allow(clippy::unnecessary_to_owned)]
    for peer in peers.iter().cloned() {
        tokio::time::sleep(Duration::from_millis(50)).await;
        spawn(async move { peer.execute().await });
    }
    tokio::time::sleep(Duration::from_millis(500)).await;

    for (i, peer) in peers.iter().enumerate() {
        let expected: Vec<String> = once(&BASE_PORT)
            .chain(ports[..i].iter())
            .chain(ports[i + 1..].iter())
            .map(|port| format!("127.0.0.1:{}", port))
            .collect();

        assert_eq!(peer.get_peers().await, expected);
    }
}
