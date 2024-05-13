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
    pub fn get_channel_no(&self) -> &i32 {
        &self.channel_no
    }

    pub fn get_type(&self) -> &String {
        &self.r#type
    }
    pub fn get_message_time(&self) -> &i64 {
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
    db_pool: sqlx::mysql::MySqlPool,
    http_client: reqwest::Client,
    api_send_count: std::sync::Mutex<u8>, //接口调用次数,不允许调用时，设置当前值为最大值。
    api_max_count: u8,                    //接口可调用的最大次数
    api_token: String,                    //apiToken
    api_topic: String,                    //消息主题通道
    message_size: u8,                     //一次消息最大包含几张图片信息
    use_time: chrono::prelude::NaiveDate, //同时调用次数时间
    sender: std::sync::mpsc::Sender<actix_web::web::Bytes>,
}
impl AppState {
    pub fn new(
        db_pool: sqlx::mysql::MySqlPool,
        http_client: reqwest::Client,
        sender: std::sync::mpsc::Sender<actix_web::web::Bytes>,
    ) -> Self {
        AppState {
            db_pool,
            http_client,
            api_send_count: std::sync::Mutex::new(0),
            api_topic: std::env::var("APP_API_TOPIC").unwrap_or_else(|_| "video".to_owned()),
            message_size: std::env::var("APP_MESSAGE_SIZE")
                .unwrap_or_else(|_| "10".to_owned())
                .parse::<u8>()
                .expect("设置APP_MESSAGE_SIZE错误!"),
            api_max_count: std::env::var("APP_API_COUNT")
                .unwrap_or_else(|_| "200".to_owned())
                .parse::<u8>()
                .expect("设置API调用次数错误"),
            api_token: std::env::var("APP_API_TOKEN").expect("未设置APP_API_TOKEN"),
            use_time: chrono::Local::now().naive_local().date(),
            sender,
        }
    }
    pub fn get_db_pool(&self) -> &sqlx::Pool<sqlx::MySql> {
        &self.db_pool
    }
    pub fn get_sender(&self) -> &std::sync::mpsc::Sender<actix_web::web::Bytes> {
        &self.sender
    }
    pub fn get_message_size(&self)->u8{
        self.message_size
    }
    //判断是否可以发送
    fn can_send(&mut self) -> bool {
        let current_date = chrono::prelude::Local::now().naive_local().date();
        if current_date > self.use_time {
            //第二天重置
            self.use_time = current_date;
            let mut count = self.api_send_count.lock().unwrap();
            *count = 0;
        }
        let count = *self.api_send_count.lock().unwrap();
        count <= self.api_max_count
    }
    //发送请求
    pub async fn send(&mut self, title: String, message: String) {
        let message = serde_json::json!({
            "token": &self.api_token,
            "title": title,
            "content": message,
            "topic": &self.api_topic
        });
        println!("debug:准备发起网络请求:{message}");
        if self.can_send() {
            self.increment_api_count();
            let res = crate::util::send_post_request(&self.http_client, message.to_string()).await;
            match res {
                Ok(data) => {
                    if let Some(code) = data.get("code") {
                        if code == 999 {
                            self.disable_api_send();
                        } else {
                            // 请求成功，继续处理其他逻辑
                            let data = data.get("data").unwrap();
                            println!("api接口返回成功:{data}");
                        }
                    } 
                }
                Err(e) => {
                    println!("接口请求出现错误：{}",e);
                    //self.disable_api_send();
                }
            }
        } else {
            println!("当前接口次数已经用完，不可使用！");
        }
    }
    //增加请求次数
    fn increment_api_count(&mut self) {
        if let Ok(mut count) = self.api_send_count.lock() {
            *count += 1;
        } else {
            // 处理获取锁失败的情况
            eprintln!("Failed to acquire lock for api_send_count");
        }
    }
    //今日不在允许登录
    fn disable_api_send(&mut self){
        let mut count = self.api_send_count.lock().unwrap();
        *count = self.api_max_count;
    }
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
