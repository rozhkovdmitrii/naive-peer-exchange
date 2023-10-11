use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
struct RandomMessage(String);
