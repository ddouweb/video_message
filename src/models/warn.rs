use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use actix_web::web::Data;
use crate::models::models::AppState;
#[derive(Debug, Serialize, Deserialize)]
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
    pub(crate) async fn push_warn(
        &self,
        state: &Data<Arc<Mutex<AppState>>>,
        msg_id: &str,
        data_type: &str,
        //sender:actix_web::web::Data<mpsc::Sender<actix_web::web::Bytes>>
    ) {
        for picture in &self.picture_list {
            let _= state.lock().unwrap().get_sender().send(actix_web::web::Bytes::from(picture.url.clone()));
            crate::db::insert_image_url(
                state.lock().unwrap().get_db_pool(),
                msg_id,
                &self.channel_name,
                &picture.url,
                data_type,
            )
            .await;
        }
    }
}
#[derive(Debug, Deserialize, Serialize)]
pub struct Picture {
    #[serde(rename = "id")]
    id: String,
    #[serde(rename = "url")]
    url: String,
}
