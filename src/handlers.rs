use crate::{
    db,
    models::{chan_msg::ChanMsg, models::{AppState, Body, Message}},
};
use actix_web::{web, HttpRequest, HttpResponse, Responder};
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
    let mut app = ChanMsg::new(db::get_db_pool().await);

    let timeout = env::var("APP_API_TIMEOUT")
        .unwrap_or_else(|_| "600".to_owned())
        .parse::<u64>().expect("设置APP_API_TIMEOUT错误");

    //设置允许过程参数
    let mut urls: Vec<String> = Vec::new();
    let mut message_count = 0;

    loop {
        match time::timeout(time::Duration::from_secs(timeout), receiver.recv()).await {
            Ok(Some(msg)) => {
                let mut data_type: &str = "Unknown";
                //println!("收到消息,消息header:{:?}",msg.header);
                //println!("收到消息,消息body:{:?}",msg.body);
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
                                            app.get_db_pool(),
                                            msg_id,
                                            data.get_channel_name(),
                                            picture.get_url(),
                                            body.get_name(),
                                        )
                                        .await;
                                    }
                                }
                                Body::OnOffLine(data) => {
                                    //send(data.get_title(), data.get_message());
                                    app.send(data.get_title(), data.get_message()).await;
                                }
                                Body::Call(data) => {
                                    app.send(data.get_title(), data.get_message()).await;
                                    crate::db::insert_image_url(
                                        app.get_db_pool(),
                                        msg_id,
                                        data_type,
                                        data.get_image(),
                                        body.get_name(),
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
                    crate::db::insert_message(app.get_db_pool(), &header, msg.body.as_ref(), data_type)
                        .await;
                    // 检查消息数量是否达到10条
                    if message_count >= app.get_message_size() {
                        let combined_messages = urls
                            .iter()
                            .map(|url| format!("<img src='{}' />", url))
                            .collect::<Vec<String>>()
                            .join(" ");
                        app.send("图片抓拍信息".to_owned(), combined_messages).await;
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
                println!("获取管道数据超时");
                if message_count > 0 {
                    let combined_messages = urls
                        .iter()
                        .map(|url| format!("<img src='{}' />", url))
                        .collect::<Vec<String>>()
                        .join(" ");
                    app.send("图片抓拍信息".to_owned(), combined_messages).await;
                    message_count = 0;
                    urls.clear();
                }

            }
        }
    }
}
