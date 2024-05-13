use std::sync::Arc;
use tokio::sync::Mutex;
use actix_web::web::Data;
use serde::{Deserialize, Serialize};

use super::models::AppState;
#[derive(Debug, Deserialize, Serialize)]
pub struct CoverUrl {
    #[serde(rename = "shortUrl")]
    short_url: String,
    id: String,
    url: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Call {
    #[serde(rename = "coverPicture")]
    cover_picture: CoverPicture,
    crypt: i32,
    #[serde(rename = "decryptedPicture")]
    decrypted_picture: String,
    #[serde(rename = "callingTime")]
    calling_time: u64,
    channel: i32,
    #[serde(rename = "relationId")]
    relation_id: String,
    #[serde(rename = "customInfo")]
    custom_info: String,
    #[serde(rename = "analysisStatusCode")]
    analysis_status_code: i32,
    #[serde(rename = "callingId")]
    calling_id: String,
    #[serde(rename = "coverUrl")]
    cover_url: CoverUrl,
    #[serde(rename = "devSerial")]
    dev_serial: String,
    #[serde(rename = "hasValueAddedService")]
    has_value_added_service: bool,
    #[serde(rename = "analysisTypeCode")]
    analysis_type_code: i32,
    checksum: String,
    action: i32,
    status: i32,
    timestamp: String,
}
impl Call {
    pub(crate) async fn push_call(
        &self,
        state: &Data<Arc<Mutex<AppState>>>,
        msg_id: &str,
        data_type: &str,
    ) {
        //println!("debug:收到视频呼叫消息");
        let message = format!("<img src='{}' />", &self.decrypted_picture);
        state.lock().await.send("视频呼叫消息".to_owned(), message).await;
        crate::db::insert_image_url(state.lock().await.get_db_pool(), msg_id,data_type, &self.decrypted_picture, data_type).await;
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CoverPicture {
    bucket: String,
    lifecycle: i32,
    crypt: i32,
    cloudtype: i32,
    checksum: String,
    length: i32,
    #[serde(rename = "type")]
    picture_type: String,
    tinyvideo: i32,
    fileid: String,
}
