use crate::{
    db,
    models::{
        chan_msg::ChanMsg,
        models::{AppState, Body, Message},
    },
};
use actix_web::{web, HttpRequest, HttpResponse, Responder};
use chrono::{prelude::NaiveTime, Timelike};
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
        .parse::<u64>()
        .expect("设置APP_API_TIMEOUT错误");

    let start_time = env::var("APP_START_TIME").unwrap_or_else(|_| "7:00:00".to_string());
    let start_time =
        NaiveTime::parse_from_str(&start_time, "%H:%M:%S").expect("Invalid start time format");

    let end_time = env::var("APP_END_TIME").unwrap_or_else(|_| "23:00:00".to_string());
    let end_time =
        NaiveTime::parse_from_str(&end_time, "%H:%M:%S").expect("Invalid end time format");

    let min_pic_count = env::var("APP_MIN_PIC_COUNT")
        .unwrap_or_else(|_| "5".to_owned())
        .parse::<u8>()
        .expect("设置APP_MIN_PIC_COUNT错误");
    println!("最小接收图片数量: {min_pic_count}");

    let max_timeout_count = env::var("APP_MAX_TIMEOUT_COUNT")
        .unwrap_or_else(|_| "3".to_owned())
        .parse::<u8>()
        .expect("设置APP_MIN_PIC_COUNT错误");
    println!("最大超时次数: {max_timeout_count}");

    let img_server = std::env::var("APP_IMG_SERVER").expect("未设置APP_IMG_SERVER");
    println!("图片服务器地址: {img_server}");

    let now = chrono::Local::now().time();
    if now >= start_time && now <= end_time {
        println!("当前时间{}:{}在{start_time}到{end_time}之间",chrono::Local::now().hour(),chrono::Local::now().minute());
    } else {
        println!("当前时间{}:{}不在{start_time}到{end_time}之间",chrono::Local::now().hour(),chrono::Local::now().minute());
    }

    //设置允许过程参数
    let mut urls: Vec<String> = Vec::new();
    let mut pic_count = 0;
    let mut timeout_count = 0;

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
                                        let id = crate::db::insert_image_url(
                                            app.get_db_pool(),
                                            msg_id,
                                            data.get_channel_name(),
                                            picture.get_url(),
                                            body.get_name(),
                                        )
                                        .await;
                                        app.save_image(id,picture.get_url_string()).await;
                                        //app.save_image(id,picture.get_url_string()).await;
                                        urls.push(format!("{img_server}/{id}.jpg"));
                                        pic_count += 1;
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
                                Body::Unknown(_) => {
                                    //println!("返回的信息: {}", data);
                                }
                                _ => {}
                            };
                            body.get_name()
                        }
                        None => "Unknown",
                    };
                    crate::db::insert_message(
                        app.get_db_pool(),
                        &header,
                        msg.body.as_ref(),
                        data_type,
                    )
                    .await;
                    // 检查消息数量是否达到指定条数
                    if pic_count >= app.get_pic_size() {
                        let combined_messages = urls
                            .iter()
                            .map(|url| format!("<img src='{}' />", url))
                            .collect::<Vec<String>>()
                            .join(" ");
                        app.send("{message_count}张图片抓拍".to_owned(), combined_messages)
                            .await;
                        pic_count = 0;
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

                let mut current_compare_pic_count = min_pic_count;
                let mut current_compare_timeout_count = max_timeout_count;
                let now = chrono::Local::now().time(); // 获取当前本地时间
                if now < start_time || now > end_time {
                    current_compare_pic_count *= 2;
                    current_compare_timeout_count *= 4;
                }
                if pic_count > 0
                    && (pic_count >= current_compare_pic_count
                        || timeout_count >= current_compare_timeout_count)
                {
                    let combined_messages = urls
                        .iter()
                        .map(|url| format!("<img src='{}' />", url))
                        .collect::<Vec<String>>()
                        .join(" ");
                    //app.send("图片抓拍信息".to_owned(), combined_messages).await;
                    app.send("{message_count}张图片抓拍".to_owned(), combined_messages)
                        .await;
                    pic_count = 0;
                    timeout_count = 0;
                    urls.clear();
                } else {
                    timeout_count += 1;
                }
            }
        }
    }
}
