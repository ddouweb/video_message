use serde::{Deserialize, Serialize};
#[derive(Serialize, Deserialize)]
pub struct Message {
    pub header: Option<Header>,
    ///#[serde(default,rename = "header")]
    pub body: Option<Body>, //#[serde(default,rename = "body")]
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Header {
    #[serde(default, rename = "channelNo")]
    channel_no: i32,

    #[serde(default, rename = "deviceId")]
    device_id: String,

    #[serde(rename = "messageId")]
    message_id: String,

    #[serde(default, rename = "messageTime")]
    message_time: i64,

    #[serde(default, rename = "type")]
    r#type: String,
}
impl Header {
    pub fn get_message_id(&self) -> &String {
        &self.message_id
    }

    pub fn get_device_id(&self) -> &String {
        &self.device_id
    }
    pub fn get_channel_no(&self)->&i32{
        &self.channel_no
    }

    pub fn get_type(&self)->&String{
        &self.r#type
    }
    pub fn get_message_time(&self)->&i64{
        &self.message_time
    }
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
pub enum Body {
    DataIndex(crate::models::data_index::DataIndex),
    ReportBody(crate::models::report::ReportBody),
    WarnBody(crate::models::warn::WarnBody),
    NationalAlarmBody(crate::models::national_alarm::NationalAlarmBody), //国际告警消息
    OnOffLine(crate::models::on_off_line::OnOffLine),                    //上下线消息
    Call(crate::models::call::Call),                                     // 呼叫消息
    Unknown(serde_json::Value), // 将未知结构解析为serde_json::Value
}

#[derive(Debug)]
pub struct AppState {
    pub db_pool: sqlx::mysql::MySqlPool,
    pub http_client: reqwest::Client,
}

impl Body {
    pub fn get_name(&self) -> &str {
        match self {
            Body::DataIndex(_) => "DataIndex",
            Body::ReportBody(_) => "ReportBody",
            Body::WarnBody(_) => "WarnBody",
            Body::NationalAlarmBody(_) => "NationalAlarmBody",
            Body::OnOffLine(_) => "OnOffLine",
            Body::Call(_) => "Call",
            Body::Unknown(_) => "Body::Unknown",
        }
    }
}

// impl Display for Body {
//     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
//         match self {
//             Color::Red => write!(f, "Red"),
//             Color::Green => write!(f, "Green"),
//             Color::Blue => write!(f, "Blue"),
//         }
//     }
// }
