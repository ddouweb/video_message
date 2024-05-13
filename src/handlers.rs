use std::sync::Arc;
use tokio::sync::Mutex;
use crate::models::models::{AppState, Body, Message};
use actix_web::{web, web::Data, HttpRequest, HttpResponse, Responder};
use serde_json::json;

// #[path ="./util.rs"]
// mod util;
pub async fn webhook(
    req: HttpRequest,
    body: web::Bytes,
    state: web::Data<Arc<Mutex<AppState>>>,
    //sender: web::Data<std::sync::mpsc::Sender<actix_web::web::Bytes>>,
) -> impl Responder {
    let _header = req.headers();
    let body = String::from_utf8_lossy(&body).into_owned();
    let result: serde_json::Value = match serde_json::from_str::<Message>(&body) {
        Ok(msg) => handle_message(msg, state).await,
        Err(e) => {
            eprintln!("Error parsing webhook message: {}", e);
            //json!({ "error": "Failed to parse webhook message" })
            json!({ "messageId": 1 })
        }
    };
    HttpResponse::Ok().json(result)
}

pub async fn handle_message(
    msg: Message,
    state: Data<Arc<Mutex<AppState>>>,
    //sender: web::Data<std::sync::mpsc::Sender<actix_web::web::Bytes>>,
) -> serde_json::Value {
    //println!("debug:收到网络请求");
    let mut data_type: &str = "Unknown";
    if let Some(header) = msg.header {
        let msg_id = header.get_message_id();
        data_type = match msg.body {
            Some(ref body) => {
                //let state = state.lock().unwrap();
                match body {
                    Body::WarnBody(data) => {
                        data.push_warn(&state, msg_id, body.get_name()).await;
                    }
                    Body::OnOffLine(data) => {
                        data.push_on_off_line(&state).await;
                    }
                    Body::Call(data) => {
                        data.push_call(&state, msg_id, body.get_name()).await;
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
        crate::db::insert_message(
            state.lock().await.get_db_pool(),
            &header,
            msg.body.as_ref(),
            data_type,
        )
        .await;
        json!({ "messageId": msg_id ,"dataType":data_type})
    } else {
        json!({ "messageId": 0 ,"dataType":data_type})
    }
}

pub async fn push_messages_to_third_party(
    app_state: Arc<Mutex<crate::models::models::AppState>>,
    receiver: std::sync::mpsc::Receiver<actix_web::web::Bytes>,
) {
    let timeout = std::time::Duration::from_secs(300);
    let mut urls: Vec<String> = Vec::new();
    let mut message_count = 0;
    let mut last_consumed_time = std::time::Instant::now();
    loop {
        match receiver.recv_timeout(timeout) {
            Ok(data) => {
                let url = String::from_utf8_lossy(&data).to_string();
                //println!("debug:收到url:{url}");
                urls.push(url);
                message_count += 1;

                // 检查消息数量是否达到10条
                if message_count >= app_state.lock().await.get_message_size() {
                    let combined_messages = urls
                        .iter()
                        .map(|url| format!("<img src='{}' />", url))
                        .collect::<Vec<String>>()
                        .join(" ");
                    app_state.lock().await
                        .send("图片抓拍信息".to_owned(), combined_messages)
                        .await;
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                // 在超时时执行其他逻辑
                println!("No data received. Continuing with other tasks.");

                // 检查是否超过5分钟且存在已接收的消息
                let elapsed_time = last_consumed_time.elapsed();
                if elapsed_time >= timeout && !urls.is_empty() {
                    let combined_messages = urls
                        .iter()
                        .map(|url| format!("<img src='{}' />", url))
                        .collect::<Vec<String>>()
                        .join("");
                    app_state.lock().await
                        .send("图片抓拍信息".to_owned(), combined_messages)
                        .await;
                    urls.clear();
                    message_count = 0;
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                // 通道的发送端已经被丢弃
                break;
            }
        }

        // 更新最后消费时间
        last_consumed_time = std::time::Instant::now();

        // 重置消息数量和 URL 列表
        if message_count >= 10 {
            urls.clear();
            message_count = 0;
        }
        tokio::time::sleep(tokio::time::Duration::from_micros(300)).await;
    }
}
