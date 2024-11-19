use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::Sender;


#[derive(Serialize, Deserialize,Clone)]
pub struct Message {
    pub header: Option<Header>,
    ///#[serde(default,rename = "header")]
    pub body: Option<Body>, //#[serde(default,rename = "body")]
}

#[derive(Serialize, Deserialize, Debug,Clone)]
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
}

#[derive(Serialize, Deserialize, Debug,Clone)]
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
    pub sender: Sender<Message>,
}
