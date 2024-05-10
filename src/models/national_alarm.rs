use serde::{Deserialize, Serialize};
#[derive(Debug, Serialize, Deserialize)]
pub struct NationalAlarmBody {
    #[serde(rename = "identifier")]
    identifier: String,
    #[serde(rename = "payload")]
    payload: String,
    #[serde(rename = "domain")]
    domain: String,
    #[serde(rename = "localIndex")]
    local_index: String,
    #[serde(rename = "deviceId")]
    device_id: String,
    #[serde(rename = "userId")]
    user_id: String,
    #[serde(rename = "resourceType")]
    resource_type: String,
    #[serde(rename = "username")]
    username: String,
}
