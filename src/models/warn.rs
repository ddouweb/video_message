use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize,Clone)]
pub struct WarnBody {
    #[serde(rename = "crypt")]
    crypt: i32,
    #[serde(rename = "alarmTime")]
    alarm_time: String,
    #[serde(rename = "channel")]
    channel: i32,
    #[serde(rename = "channelType")]
    channel_type: i32,
    #[serde(rename = "relationId")]
    relation_id: String,
    #[serde(rename = "customInfo")]
    custom_info: String,
    #[serde(rename = "requestTime")]
    request_time: u64,
    #[serde(rename = "devSerial")]
    dev_serial: String,
    #[serde(rename = "alarmType")]
    alarm_type: String,
    #[serde(rename = "customType")]
    custom_type: String,
    #[serde(rename = "alarmId")]
    alarm_id: String,
    #[serde(rename = "checksum")]
    checksum: String,
    #[serde(rename = "channelName")]
    channel_name: String,
    #[serde(rename = "location")]
    location: String,
    #[serde(rename = "describe")]
    describe: String,
    #[serde(default, rename = "pictureList")]
    picture_list: Vec<Picture>,
    #[serde(rename = "status")]
    status: i32,
}
impl WarnBody {
    pub fn get_picture_list(&self)->&Vec<Picture>{
        &self.picture_list
    }
}
#[derive(Debug, Deserialize, Serialize,Clone)]
pub struct Picture {
    #[serde(rename = "id")]
    id: String,
    #[serde(rename = "url")]
    url: String,
}
impl Picture {
    pub fn get_url_string(&self)->String{
        self.url.clone()
    }
}
