use clap::Parser;

use pex::PeerExchangeConfig;

const MESSAGING_TIMEOUT_SEC: u64 = 2;
const DEFAULT_LISTEN_PORT: u16 = 8080;

#[derive(Debug, Parser)]
#[clap(author, version, about, long_about = None)]
pub(super) struct Cli {
    #[arg(long, short = 'P', help = "Messaging period", default_value_t = MESSAGING_TIMEOUT_SEC)]
    period: u64,
    #[arg(
        long,
        help = "Public address to be sent as a peer address",
        default_value = "127.0.0.1"
    )]
    address: String,
    #[arg(long, short, help = "Port of the current peer to listen to", default_value_t = DEFAULT_LISTEN_PORT)]
    port: u16,
    #[arg(long, short, help = "Address of the peer to connect to initially")]
    connect: Option<String>,
}

impl From<Cli> for PeerExchangeConfig {
    fn from(value: Cli) -> Self {
        PeerExchangeConfig {
            messaging_timeout_sec: value.period,
            port: value.port,
            address: value.address,
            init_peer: value.connect,
        }
    }
}
