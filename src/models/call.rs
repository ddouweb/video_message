use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize,Clone)]
pub struct CoverUrl {
    #[serde(rename = "shortUrl")]
    short_url: String,
    id: String,
    url: String,
}

#[derive(Debug, Deserialize, Serialize,Clone)]
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
    pub fn get_title(&self)->String{
        format!("视频呼叫消息")
    }

    pub fn get_message(&self)->String{
        format!("<img src='{}' />", &self.decrypted_picture)
    }
}

#[derive(Debug, Deserialize, Serialize,Clone)]
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
