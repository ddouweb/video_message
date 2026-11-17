use crate::{
    db,
    image_dedup::{compute_phash_from_memory, ImageDedup},
    person_detector::PersonDetector,
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
    let start_time = NaiveTime::parse_from_str(&start_time, "%H:%M:%S").expect("Invalid start time format");

    let end_time = env::var("APP_END_TIME").unwrap_or_else(|_| "23:00:00".to_string());
    let end_time = NaiveTime::parse_from_str(&end_time, "%H:%M:%S").expect("Invalid end time format");

    let min_pic_count = env::var("APP_MIN_PIC_COUNT")
        .unwrap_or_else(|_| "10".to_owned())
        .parse::<u8>()
        .expect("设置APP_MIN_PIC_COUNT错误");
    println!("最小接收图片数量: {min_pic_count}");


    let max_pic_count = std::env::var("APP_MESSAGE_SIZE")
                .unwrap_or_else(|_| "50".to_owned())
                .parse::<u8>()
                .expect("设置APP_MESSAGE_SIZE错误!");
    println!("最大接收图片数量: {max_pic_count}");

    // 有人出现即时提醒的冷却期: 期内不重复即时推送, 告警图静默累计走批量摘要
    let alert_cooldown = std::time::Duration::from_secs(
        env::var("APP_ALERT_COOLDOWN_SECS")
            .unwrap_or_else(|_| "600".to_owned())
            .parse::<u64>()
            .unwrap_or(600),
    );
    println!("有人出现即时提醒冷却期: {}秒", alert_cooldown.as_secs());

    /*let max_timeout_count = env::var("APP_MAX_TIMEOUT_COUNT")
        .unwrap_or_else(|_| "3".to_owned())
        .parse::<u8>()
        .expect("设置APP_MIN_PIC_COUNT错误");
    println!("最大超时次数: {max_timeout_count}");*/

    let img_server = std::env::var("APP_IMG_SERVER").expect("未设置APP_IMG_SERVER");
    println!("图片服务器地址: {img_server}");

    // 初始化图片去重
    let p_hash_threshold = env::var("APP_P_HASH_THRESHOLD")
        .unwrap_or_else(|_| "5".to_owned())
        .parse::<u8>()
        .unwrap_or(5);

    let ai_similarity_threshold = env::var("APP_AI_SIMILARITY_THRESHOLD")
        .unwrap_or_else(|_| "0.95".to_owned())
        .parse::<f32>()
        .unwrap_or(0.95);

    let mut image_dedup = ImageDedup::new(p_hash_threshold, ai_similarity_threshold, 1000);
    let http_client = reqwest::Client::new();
    println!("图片去重功能已启用，pHash阈值: {}, AI相似度阈值: {}", p_hash_threshold, ai_similarity_threshold);

    // 初始化人员检测
    let person_alert_enabled = env::var("APP_PERSON_ALERT_ENABLED")
        .unwrap_or_else(|_| "true".to_owned())
        .parse::<bool>()
        .unwrap_or(true);

    let person_confidence = env::var("APP_PERSON_CONFIDENCE_THRESHOLD")
        .unwrap_or_else(|_| "0.5".to_owned())
        .parse::<f32>()
        .unwrap_or(0.5);

    let model_path = env::var("APP_MODEL_PATH")
        .unwrap_or_else(|_| "models/yolov8n-pose.onnx".to_owned());

    let mut person_detector: Option<PersonDetector> = if person_alert_enabled {
        let model_path = std::path::Path::new(&model_path);
        match PersonDetector::new(model_path, person_confidence) {
            Ok(detector) => {
                println!("人员检测功能已启用，置信度阈值: {}", person_confidence);
                Some(detector)
            }
            Err(e) => {
                eprintln!("人员检测初始化失败: {}", e);
                None
            }
        }
    } else {
        None
    };

    let now = chrono::Local::now().time();
    if now >= start_time && now <= end_time {
        println!(
            "当前时间{}:{}在{start_time}到{end_time}之间",
            chrono::Local::now().hour(),
            chrono::Local::now().minute()
        );
    } else {
        println!(
            "当前时间{}:{}不在{start_time}到{end_time}之间",
            chrono::Local::now().hour(),
            chrono::Local::now().minute()
        );
    }

    //设置允许过程参数
    let mut urls: Vec<String> = Vec::new();
    let mut pic_count = 0;
    let mut last_person_alert: Option<std::time::Instant> = None;
    //let mut timeout_count = 0;
    let mut date_dir = chrono::Local::now().format("%Y-%m-%d").to_string();
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
                                    // 本事件是否一张图都没能展示(全部加密/下载失败)
                                    let mut encrypted_only = true;
                                    for picture in data.get_picture_list() {
                                        // 只下载一次, 哈希/落盘复用同一份数据
                                        let bytes = match http_client.get(picture.get_url()).send().await {
                                            Ok(resp) => match resp.bytes().await {
                                                Ok(b) => b,
                                                Err(e) => {
                                                    eprintln!("下载告警图片失败: {e}");
                                                    continue;
                                                }
                                            },
                                            Err(e) => {
                                                eprintln!("下载告警图片失败: {e}");
                                                continue;
                                            }
                                        };

                                        // 计算 pHash; 解码失败(hikencod加密图/损坏图)不入库不推送,
                                        // 事件降级为纯文字即时提醒
                                        let p_hash = match compute_phash_from_memory(&bytes) {
                                            Ok(h) => {
                                                encrypted_only = false;
                                                Some(h)
                                            }
                                            Err(_) => {
                                                eprintln!(
                                                    "告警图片无法解码(疑似hikencod加密), 跳过: {}",
                                                    picture.get_url()
                                                );
                                                continue;
                                            }
                                        };

                                        // 提取 AI 特征（用于去重, 当前模型无特征头返回空）
                                        let ai_feature = if let Some(ref mut detector) = person_detector {
                                            detector.extract_feature(&bytes).ok()
                                        } else {
                                            None
                                        };

                                        // 检查是否重复（pHash + AI 组合）
                                        let is_dup = if let (Some(ref ph), Some(ref feat)) = (&p_hash, &ai_feature) {
                                            image_dedup.is_duplicate(ph, feat)
                                        } else if let Some(ref ph) = p_hash {
                                            image_dedup.is_duplicate_phash(ph)
                                        } else {
                                            false
                                        };

                                        if is_dup {
                                            println!("过滤重复图片: {}", picture.get_url());
                                            continue; // 跳过，不计入 pic_count
                                        }

                                        let id = crate::db::insert_image_url(
                                            app.get_db_pool(),
                                            msg_id,
                                            data.get_channel_name(),
                                            picture.get_url(),
                                            body.get_name(),
                                            &date_dir,
                                        )
                                        .await;
                                        app.save_image_bytes(id, &bytes, &date_dir).await;
                                        let img_url = format!("{img_server}/{id}.jpg");
                                        urls.push(img_url.clone());

                                        // 人体告警即时提醒: 冷却期外首张告警图立即推送,
                                        // 期内不重复打扰, 后续告警图继续累计走批量摘要
                                        if last_person_alert
                                            .map_or(true, |t| t.elapsed() >= alert_cooldown)
                                        {
                                            let message = format!("<img src='{}' />", img_url);
                                            app.send(
                                                format!("⚠️ 有人出现: {}", data.get_channel_name()),
                                                message,
                                            )
                                            .await;
                                            last_person_alert = Some(std::time::Instant::now());
                                        }

                                        // 添加到去重缓存
                                        if let Some(ref ph) = p_hash {
                                            image_dedup.add(
                                                ph.clone(),
                                                ai_feature.unwrap_or_default(),
                                            );
                                        }

                                        pic_count += 1;
                                    }

                                    // 整个事件一张可用图都没有时, 仍发纯文字即时提醒(冷却期规则同上)
                                    if encrypted_only
                                        && last_person_alert
                                            .map_or(true, |t| t.elapsed() >= alert_cooldown)
                                    {
                                        app.send(
                                            format!("⚠️ 有人出现: {}", data.get_channel_name()),
                                            "告警图片已加密无法展示, 稍后由轮询抓拍补图".to_owned(),
                                        )
                                        .await;
                                        last_person_alert = Some(std::time::Instant::now());
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
                                        &date_dir,
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
                    if pic_count >= max_pic_count {
                        let combined_messages = urls
                            .iter()
                            .map(|url| format!("<img src='{}' />", url))
                            .collect::<Vec<String>>()
                            .join(" ");
                        app.send(format!("{}张图片抓拍", pic_count), combined_messages)
                            .await;
                        pic_count = 0;
                        urls.clear();
                        image_dedup.clear(); // 清空去重缓存
                    }
                }
            }
            Ok(None) => {
                println!("Channel closed.");
                break;
            }
            Err(_) => {
                println!("获取管道数据超时");
                date_dir = chrono::Local::now().format("%Y-%m-%d").to_string();
                let mut current_compare_pic_count = min_pic_count;
                //let mut current_compare_timeout_count = max_timeout_count;
                let now = chrono::Local::now().time(); // 获取当前本地时间
                if now < start_time || now > end_time {
                    current_compare_pic_count = max_pic_count;
                    //current_compare_timeout_count *= 4;
                }
                if pic_count >= current_compare_pic_count
                // || timeout_count >= current_compare_timeout_count)
                {
                    let combined_messages = urls
                        .iter()
                        .map(|url| format!("<img src='{}' />", url))
                        .collect::<Vec<String>>()
                        .join(" ");
                    //app.send("图片抓拍信息".to_owned(), combined_messages).await;
                    app.send(format!("{}张图片抓拍", pic_count), combined_messages)
                        .await;
                    pic_count = 0;
                    //timeout_count = 0;
                    urls.clear();
                    image_dedup.clear(); // 清空去重缓存
                } else {
                    //timeout_count += 1;
                }
            }
        }
    }
}
