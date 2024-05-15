pub struct ChanMsg{
    db_pool: sqlx::mysql::MySqlPool,
    http_client: reqwest::Client,
    api_send_count: std::sync::Mutex<u8>, //接口调用次数,不允许调用时，设置当前值为最大值。
    api_max_count: u8,                    //接口可调用的最大次数
    api_token: String,                    //apiToken
    api_topic: String,                    //消息主题通道
    message_size: u8,                     //一次消息最大包含几张图片信息
    use_time: chrono::prelude::NaiveDate, //同时调用次数时间
}
impl ChanMsg {
    pub fn new(db_pool: sqlx::mysql::MySqlPool,
        )-> Self{
            ChanMsg {
                db_pool,
                http_client:reqwest::Client::new(),
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
            }
    }
    pub fn get_db_pool(&self) -> &sqlx::Pool<sqlx::MySql> {
        &self.db_pool
    }
    fn can_send(& mut self)->bool{
        let current_date: chrono::prelude::NaiveDate = chrono::prelude::Local::now().naive_local().date();
        if current_date > self.use_time {
            // 第二天重置
            self.use_time = current_date;
            let mut count = self.api_send_count.lock().unwrap();
            *count = 0;
        }
        let count = self.api_send_count.lock().unwrap();
        *count <= self.api_max_count
    }
    pub(crate) async fn send(& mut self, title: String, message: String){
        let message = serde_json::json!({
            "token": &self.api_token,
            "title": title,
            "content": message,
            "topic": &self.api_topic
        });
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
    pub fn get_message_size(&self)->u8{
        self.message_size
    }
}