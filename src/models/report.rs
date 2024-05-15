use serde::{Deserialize, Serialize};
#[derive(Debug, Serialize, Deserialize,Clone)]
pub struct ReportBody {
    #[serde(rename = "reported")]
    reported: Vec<ReportedItem>,
    #[serde(rename = "type")]
    r#type: String,
}
#[derive(Debug, Serialize, Deserialize,Clone)]
struct ReportedItem {
    #[serde(default, rename = "actor")]
    actor: String,
    #[serde(default, rename = "channel")]
    channel: i32,
    #[serde(default, rename = "type")]
    r#type: String,
    #[serde(default, rename = "status")]
    status: i32,
}