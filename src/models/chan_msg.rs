use std::time::{SystemTime, UNIX_EPOCH};

pub struct ChanMsg {
    db_pool: sqlx::mysql::MySqlPool,
    http_client: reqwest::Client,
    api_send_count: std::sync::Mutex<u8>, //接口调用次数,不允许调用时，设置当前值为最大值。
    api_max_count: u8,                    //接口可调用的最大次数
    api_token: String,                    //apiToken
    api_topic: String,                    //消息主题通道
    pic_size: u8,                         //一次消息最大包含几张图片信息
    use_time: chrono::prelude::NaiveDate, //API调用时间
    his_path: String,                     //历史图片的保存路径
    last_path: String,                    //最新图片的保存路径
                                          //img_server: String,                   //图片服务器地址
}
impl ChanMsg {
    pub fn new(db_pool: sqlx::mysql::MySqlPool) -> Self {
        let his_path =
            std::env::var("APP_IMG_PATH_HIS").unwrap_or_else(|_| "/data/his_path".to_owned());
        let last_path =
            std::env::var("APP_IMG_PATH_LAST").unwrap_or_else(|_| "/data/last_path".to_owned());
        std::fs::create_dir_all(&his_path).unwrap();
        std::fs::create_dir_all(&last_path).unwrap();
        ChanMsg {
            db_pool,
            http_client: reqwest::Client::new(),
            // http_client: reqwest::Client::builder()
            //     .pool_max_idle_per_host(10) // 设置每个主机最大的空闲连接数
            //     .pool_idle_timeout(std::time::Duration::from_secs(30)) // 设置连接的空闲超时时间为30秒
            //     .build()
            //     .unwrap(),
            api_send_count: std::sync::Mutex::new(0),
            api_topic: std::env::var("APP_API_TOPIC").unwrap_or_else(|_| "video".to_owned()),
            pic_size: std::env::var("APP_MESSAGE_SIZE")
                .unwrap_or_else(|_| "10".to_owned())
                .parse::<u8>()
                .expect("设置APP_MESSAGE_SIZE错误!"),
            api_max_count: std::env::var("APP_API_COUNT")
                .unwrap_or_else(|_| "200".to_owned())
                .parse::<u8>()
                .expect("设置API调用次数错误"),
            api_token: std::env::var("APP_API_TOKEN").expect("未设置APP_API_TOKEN"),
            use_time: chrono::Local::now().naive_local().date(),
            his_path: his_path,
            last_path: last_path,
            //img_server: std::env::var("APP_IMG_SERVER").expect("未设置APP_IMG_SERVER"),
        }
    }
    pub fn get_db_pool(&self) -> &sqlx::Pool<sqlx::MySql> {
        &self.db_pool
    }
    fn can_send(&mut self) -> bool {
        let current_date = chrono::prelude::Local::now().naive_local().date();
        if current_date > self.use_time {
            // 第二天重置
            self.use_time = current_date;
            let mut count = self.api_send_count.lock().unwrap();
            *count = 0;
            //清理昨日图片
            let _ = self.clean_current_folder();
        }
        let count = self.api_send_count.lock().unwrap();
        *count <= self.api_max_count
    }
    pub(crate) async fn send(&mut self, title: String, message: String) {
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
                        if code == 999 || code == 900 || code == 903 {
                            self.disable_api_send();
                        }
                        // else {
                        //     // 请求成功，继续处理其他逻辑
                        //     let data = data.get("data").unwrap();
                        //     println!("api接口返回成功:{data}");
                        // }
                    }
                }
                Err(e) => {
                    println!("接口请求出现错误：{}", e);
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
            println!(
                "{} 已经发送请求：{}次",
                chrono::prelude::Local::now().naive_local().date(),
                count
            );
        } else {
            // 处理获取锁失败的情况
            eprintln!("Failed to acquire lock for api_send_count");
        }
    }
    //今日不再允许推送
    fn disable_api_send(&mut self) {
        let mut count = self.api_send_count.lock().unwrap();
        println!(
            "{} 今日不再允许推送,已经使用{}次",
            chrono::prelude::Local::now().naive_local().date(),
            count
        );
        *count = self.api_max_count;
    }
    pub fn get_pic_size(&self) -> u8 {
        self.pic_size
    }

    //下载图片。
    pub(crate) async fn save_image(&self, id: u64, img_url: String) {
        crate::util::save_image(&self.http_client,id, img_url, &self.last_path, &self.his_path).await;
    }

    /**
     * 清理昨日的图片
     */
    fn clean_current_folder(&self) -> std::io::Result<()> {
        let now = SystemTime::now();
        let today = now.duration_since(UNIX_EPOCH).unwrap().as_secs() / 86400;

        for entry in std::fs::read_dir(&self.last_path)? {
            let entry = entry?;
            let metadata = entry.metadata()?;
            let modified = metadata.modified()?;
            let modified_day = modified.duration_since(UNIX_EPOCH).unwrap().as_secs() / 86400;

            if modified_day + 600 < today {
                std::fs::remove_file(entry.path())?;
            }
        }
        Ok(())
    }
}
