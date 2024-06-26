use serde::{Deserialize, Serialize};
#[derive(Debug, Serialize, Deserialize,Clone)]
pub struct DataIndex {
    #[serde(rename = "data")]
    data: String,
    #[serde(rename = "index")]
    index: u32,
}