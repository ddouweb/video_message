use serde::{Deserialize, Serialize};
#[derive(Debug, Serialize, Deserialize)]
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
    pub(crate) async fn push_on_off_line(&self, state: &actix_web::web::Data<crate::models::models::AppState>) {
        println!("{:?}", state);
        println!("摄像头[{}] 于{}  状态变更为{}", &self.device_name, &self.occur_time, &self.msg_type);
        // match self.msg_type {
        //     DeviceStatus::Online => {
        //         println!("设备在线 {} {} {:?}", &self.device_name, self.occur_time, &self.msg_type);
        //     }
        //     DeviceStatus::Offline => {
        //         println!("摄像头[{}] 于{}  离线{:?}", &self.device_name, &self.occur_time, self.msg_type);
        //     }
        // }
    }
}