use crate::models::models::{AppState, Body, Message};
//use crate::util::HttpClient;
use actix_web::{web, HttpRequest, HttpResponse, Responder};
use serde_json::json;

// #[path ="./util.rs"]
// mod util;
pub async fn webhook(
    req: HttpRequest,
    body: web::Bytes,
    state: web::Data<AppState>,
    //pool: web::Data<sqlx::MySqlPool>,
) -> impl Responder {
    let _header = req.headers();
    let body = String::from_utf8_lossy(&body).into_owned();
    //println!("消息请求头: {:?}, 请求体: {}", header, body_string);
    //println!("收到的消息: {}", body);

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

pub async fn handle_message(msg: Message, state: web::Data<AppState>) -> serde_json::Value {
    let mut data_type: &str = "Unknown";
    if let Some(header) = msg.header {
        let msg_id = header.get_message_id();
        data_type = match msg.body {
            Some(ref body) => {
                match body {
                    Body::WarnBody(data) => {
                        data.push_warn(&state,msg_id,body.get_name()).await;
                    }
                    Body::OnOffLine(data) => {
                        data.push_on_off_line(&state).await;
                    }
                    Body::Call(data) => {
                        data.push_call(&state,msg_id,body.get_name()).await;
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
        crate::db::insert_message(&state.db_pool, &header,msg.body.as_ref(),data_type).await;
        json!({ "messageId": msg_id ,"dataType":data_type})
    } else {
        json!({ "messageId": 0 ,"dataType":data_type})
    }
}
