use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize,Clone)]
pub struct OnOffLine {
    #[serde(rename = "devType")]
    dev_type: String,
    #[serde(rename = "IsCleanSession")]
    is_clean_session: i32,
    #[serde(rename = "regTime")]
    reg_time: String,
    #[serde(rename = "natIp")]
    nat_ip: String,
    #[serde(rename = "msgType")]
    msg_type: String,
    #[serde(rename = "subSerial")]
    sub_serial: String,
    #[serde(rename = "occurTime")]
    occur_time: String,
    #[serde(rename = "deviceName")]
    device_name: String,
    #[serde(rename = "dasId")]
    das_id: String,
}
#[derive(Debug, Deserialize, Serialize)]
pub enum DeviceStatus {
    #[serde(rename = "ONLINE")]
    Online,
    #[serde(rename = "OFFLINE")]
    Offline
}

impl OnOffLine {
    pub fn get_title(&self)->String{
        format!("摄像头[{}] 状态消息",&self.device_name)
    }

    pub fn get_message(&self)->String{
        format!("摄像头[{}] 于{}  状态变更为{}", &self.device_name, &self.occur_time, &self.msg_type)
    }
}