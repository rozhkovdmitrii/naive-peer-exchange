use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case", tag = "type", content = "data")]
pub(super) enum PeerMessage {
    RandomMessage { data: String },
    PublicAddress { address: String },
    KnownPeers { peers: Vec<String> },
}
