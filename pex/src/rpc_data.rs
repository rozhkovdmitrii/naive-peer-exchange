use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case", tag = "type", content = "data")]
pub(super) enum NaiveExchangeMessage {
    RandomMessage { data: String },
    PublicAddress { port: u16 },
}
