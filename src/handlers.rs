use crate::{
    db,
    models::models::{AppState, Body, Message},
};
use actix_web::{web, HttpRequest, HttpResponse, Responder};
use reqwest::Client;
use serde_json::json;
use std::env;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tokio::time;

pub async fn webhook(
    req: HttpRequest,
    body: web::Bytes,
    state: web::Data<Arc<Mutex<AppState>>>,
) -> impl Responder {
    let _header = req.headers();
    let body = String::from_utf8_lossy(&body).into_owned();
    let result: serde_json::Value = match serde_json::from_str::<Message>(&body) {
        Ok(msg) => handle_request(msg, state).await,
        Err(e) => {
            eprintln!("Error parsing webhook message: {}", e);
            //json!({ "error": "Failed to parse webhook message" })
            json!({ "messageId": 1 })
        }
    };
    HttpResponse::Ok().json(result)
}

pub async fn handle_request(
    msg: Message,
    state: web::Data<Arc<Mutex<AppState>>>,
) -> serde_json::Value {
    if let Some(header) = msg.header.clone() {
        let msg_id = header.get_message_id();
        let sender: mpsc::Sender<Message> = state.lock().unwrap().sender.clone();
        if let Err(e) = sender.send(msg).await {
            eprintln!("Failed to send message: {}", e);
        }
        json!({ "messageId": msg_id})
    } else {
        json!({ "messageId": 0})
    }
}

pub async fn handle_message(mut receiver: mpsc::Receiver<Message>) {
    //初始化系统信息
    dotenv::dotenv().ok();
    let db_pool = db::get_db_pool().await;
    let api_send_count = Mutex::new(0);
    let api_topic = env::var("APP_API_TOPIC").unwrap_or_else(|_| "video".to_owned());
    let message_size = env::var("APP_MESSAGE_SIZE")
        .unwrap_or_else(|_| "10".to_owned())
        .parse::<u8>()
        .expect("设置APP_MESSAGE_SIZE错误!");
    let api_max_count = env::var("APP_API_COUNT")
        .unwrap_or_else(|_| "200".to_owned())
        .parse::<u8>()
        .expect("设置API调用次数错误");
    let api_token = env::var("APP_API_TOKEN").expect("未设置APP_API_TOKEN");
    let mut use_time = chrono::Local::now().naive_local().date();
    let http_client = Client::new();

    let timeout = env::var("APP_API_TIMEOUT")
        .unwrap_or_else(|_| "200".to_owned())
        .parse::<u64>()
        .expect("设置API调用次数错误");

    //设置允许过程参数
    let mut urls: Vec<String> = Vec::new();
    let mut message_count = 0;
    let rt = tokio::runtime::Runtime::new().unwrap();

    // 子函数（闭包）：增加请求次数
    let increment_api_count = || {
        if let Ok(mut count) = api_send_count.lock() {
            *count += 1;
        } else {
            // 处理获取锁失败的情况
            eprintln!("Failed to acquire lock for api_send_count");
        }
    };

    // 子函数（闭包）：检查是否可以发送
    let mut can_send = || -> bool {
        let current_date = chrono::prelude::Local::now().naive_local().date();
        if current_date > use_time {
            // 第二天重置
            use_time = current_date;
            let mut count = api_send_count.lock().unwrap();
            *count = 0;
        }
        let count = api_send_count.lock().unwrap();
        *count <= api_max_count
    };

    //今日不在允许登录
    let disable_api_send = ||{
        let mut count = api_send_count.lock().unwrap();
        *count = api_max_count;
    };

    // 子函数（闭包）：发送消息
    let mut send = |title: String, message: String| {
        let message = serde_json::json!({
            "token": api_token,
            "title": title,
            "content": message,
            "topic": api_topic
        });
        if can_send() {
            increment_api_count();
            let res =rt.block_on(crate::util::send_post_request(&http_client, message.to_string()));
            match res {
                Ok(data) => {
                    if let Some(code) = data.get("code") {
                        if code == 999 {
                            disable_api_send();
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
            println!("API send limit reached");
        }
    };

    loop {
        match time::timeout(time::Duration::from_secs(timeout), receiver.recv()).await {
            Ok(Some(msg)) => {
                let mut data_type: &str = "Unknown";
                if let Some(header) = msg.header {
                    let msg_id = header.get_message_id();
                    data_type = match msg.body {
                        Some(ref body) => {
                            match body {
                                Body::WarnBody(data) => {
                                    for picture in data.get_picture_list() {
                                        urls.push(picture.get_url_string());
                                        message_count += 1;
                                        crate::db::insert_image_url(
                                            &db_pool,
                                            msg_id,
                                            data.get_channel_name(),
                                            picture.get_url(),
                                            data_type,
                                        )
                                        .await;
                                    }
                                }
                                Body::OnOffLine(data) => {
                                    send(data.get_title(), data.get_message());
                                }
                                Body::Call(data) => {
                                    send(data.get_title(), data.get_message());
                                    crate::db::insert_image_url(
                                        &db_pool,
                                        msg_id,
                                        data_type,
                                        data.get_image(),
                                        data_type,
                                    )
                                    .await;
                                }
                                Body::Unknown(data) => {
                                    println!("返回的信息: {}", data);
                                }
                                _ => {}
                            };
                            body.get_name()
                        }
                        None => "Unknown",
                    };
                    crate::db::insert_message(&db_pool, &header, msg.body.as_ref(), data_type)
                        .await;
                    // 检查消息数量是否达到10条
                    if message_count >= message_size {
                        let combined_messages = urls
                            .iter()
                            .map(|url| format!("<img src='{}' />", url))
                            .collect::<Vec<String>>()
                            .join(" ");
                        send("图片抓拍信息".to_owned(), combined_messages);
                        message_count = 0;
                        urls.clear();
                    }
                }
            }
            Ok(None) => {
                println!("Channel closed.");
                break;
            }
            Err(_) => {
                if message_count > 0 {
                    let combined_messages = urls
                        .iter()
                        .map(|url| format!("<img src='{}' />", url))
                        .collect::<Vec<String>>()
                        .join(" ");
                    send("图片抓拍信息".to_owned(), combined_messages);
                    message_count = 0;
                    urls.clear();
                }

            }
        }
    }
}
