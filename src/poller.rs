use crate::db;
use crate::image_dedup::{compute_phash_from_memory, phash_similarity};
use crate::models::chan_msg::ChanMsg;
use crate::person_detector::PersonDetector;
use crate::util;
use chrono::prelude::NaiveTime;
use std::collections::HashMap;
use std::time::Duration;

const TOKEN_URL: &str = "https://open.ys7.com/api/lapp/token/get";
const DEVICE_LIST_URL: &str = "https://open.ys7.com/api/lapp/device/list";
const CAPTURE_URL: &str = "https://open.ys7.com/api/lapp/device/capture";

/// accessToken 缓存
struct EzToken {
    access_token: String,
    refresh_at: chrono::DateTime<chrono::Local>,
}

enum EzError {
    /// token过期, 需要重新获取
    TokenExpired,
    Api(String),
}

impl std::fmt::Display for EzError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EzError::TokenExpired => write!(f, "accessToken已过期"),
            EzError::Api(msg) => write!(f, "{}", msg),
        }
    }
}

/// 兼容字符串/数字两种 code 返回格式
fn resp_code(v: &serde_json::Value) -> String {
    match &v["code"] {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        _ => String::new(),
    }
}

async fn fetch_token(
    client: &reqwest::Client,
    app_key: &str,
    app_secret: &str,
) -> Result<String, String> {
    let resp: serde_json::Value = client
        .post(TOKEN_URL)
        .form(&[("appKey", app_key), ("appSecret", app_secret)])
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    if resp_code(&resp) == "200" {
        resp["data"]["accessToken"]
            .as_str()
            .map(|s| s.to_owned())
            .ok_or_else(|| "返回数据缺少accessToken".to_owned())
    } else {
        Err(format!(
            "code={} msg={}",
            resp_code(&resp),
            resp["msg"].as_str().unwrap_or("")
        ))
    }
}

/// 返回在线设备的 (设备序列号, 设备名称) 列表
async fn fetch_device_list(
    client: &reqwest::Client,
    access_token: &str,
) -> Result<Vec<(String, String)>, EzError> {
    let resp: serde_json::Value = client
        .post(DEVICE_LIST_URL)
        .form(&[
            ("accessToken", access_token),
            ("pageStart", "0"),
            ("pageSize", "50"),
        ])
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| EzError::Api(e.to_string()))?
        .json()
        .await
        .map_err(|e| EzError::Api(e.to_string()))?;

    let code = resp_code(&resp);
    if code == "200" {
        let mut devices = Vec::new();
        if let Some(list) = resp["data"].as_array() {
            for dev in list {
                // status: 1-在线, 0-离线
                if dev["status"].as_i64() == Some(1) {
                    let serial = dev["deviceSerial"].as_str().unwrap_or("").to_owned();
                    let name = dev["deviceName"].as_str().unwrap_or("").to_owned();
                    if !serial.is_empty() {
                        devices.push((serial, name));
                    }
                }
            }
        }
        Ok(devices)
    } else if code == "10002" {
        Err(EzError::TokenExpired)
    } else {
        Err(EzError::Api(format!(
            "code={} msg={}",
            code,
            resp["msg"].as_str().unwrap_or("")
        )))
    }
}

/// 手动抓拍, 返回图片URL
async fn capture(
    client: &reqwest::Client,
    access_token: &str,
    device_serial: &str,
) -> Result<String, EzError> {
    let resp: serde_json::Value = client
        .post(CAPTURE_URL)
        .form(&[
            ("accessToken", access_token),
            ("deviceSerial", device_serial),
            ("channelNo", "1"),
        ])
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| EzError::Api(e.to_string()))?
        .json()
        .await
        .map_err(|e| EzError::Api(e.to_string()))?;

    let code = resp_code(&resp);
    if code == "200" {
        resp["data"]["picUrl"]
            .as_str()
            .map(|s| s.to_owned())
            .ok_or_else(|| EzError::Api("返回数据缺少picUrl".to_owned()))
    } else if code == "10002" {
        Err(EzError::TokenExpired)
    } else {
        Err(EzError::Api(format!(
            "code={} msg={}",
            code,
            resp["msg"].as_str().unwrap_or("")
        )))
    }
}

/// 读秒数型环境变量, 解析失败时回退默认值
fn read_secs_env(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .unwrap_or_else(|_| default.to_string())
        .parse::<u64>()
        .unwrap_or(default)
}

/// 计算设备下次抓拍间隔: 最近保留过有人图 → 高频; 夜间无人 → 低频; 否则基准
fn next_capture_interval(
    person_last: &HashMap<String, tokio::time::Instant>,
    serial: &str,
    night: bool,
    visit_end_secs: u64,
    base: u64,
    active: u64,
    night_iv: u64,
) -> u64 {
    let person_recent = person_last
        .get(serial)
        .map_or(false, |t| t.elapsed() < Duration::from_secs(visit_end_secs));
    if person_recent {
        active
    } else if night {
        night_iv
    } else {
        base
    }
}

/// 按原有规则推送累积的图片并清空缓存
async fn flush(app: &mut ChanMsg, urls: &mut Vec<String>, pic_count: &mut u8) {
    let combined_messages = urls
        .iter()
        .map(|url| format!("<img src='{}' />", url))
        .collect::<Vec<String>>()
        .join(" ");
    app.send(format!("{}张图片抓拍", pic_count), combined_messages)
        .await;
    urls.clear();
    *pic_count = 0;
}

/// 萤石开放平台轮询抓拍模式:
/// 周期刷新accessToken -> 白天定时对在线设备抓拍 -> 与上一张图对比相似度去重 -> 按原有规则推送
pub async fn run_poller() {
    dotenv::dotenv().ok();

    let poller_on = matches!(
        std::env::var("APP_POLLER").unwrap_or_default().to_lowercase().as_str(),
        "on" | "true" | "1"
    );
    if !poller_on {
        log_info!("轮询抓拍模式未开启(APP_POLLER)");
        return;
    }

    let app_key = std::env::var("EZVIZ_APP_KEY").expect("未设置EZVIZ_APP_KEY");
    let app_secret = std::env::var("EZVIZ_APP_SECRET").expect("未设置EZVIZ_APP_SECRET");

    let interval = std::env::var("APP_CAPTURE_INTERVAL_SECS")
        .unwrap_or_else(|_| "120".to_owned())
        .parse::<u64>()
        .expect("设置APP_CAPTURE_INTERVAL_SECS错误");
    let refresh_hours = std::env::var("APP_TOKEN_REFRESH_HOURS")
        .unwrap_or_else(|_| "120".to_owned())
        .parse::<u64>()
        .expect("设置APP_TOKEN_REFRESH_HOURS错误");
    let similarity_threshold = std::env::var("APP_SIMILARITY_THRESHOLD")
        .unwrap_or_else(|_| "0.95".to_owned())
        .parse::<f64>()
        .expect("设置APP_SIMILARITY_THRESHOLD错误");

    let start_time = std::env::var("APP_START_TIME").unwrap_or_else(|_| "7:00:00".to_string());
    let start_time =
        NaiveTime::parse_from_str(&start_time, "%H:%M:%S").expect("Invalid start time format");
    let end_time = std::env::var("APP_END_TIME").unwrap_or_else(|_| "23:00:00".to_string());
    let end_time =
        NaiveTime::parse_from_str(&end_time, "%H:%M:%S").expect("Invalid end time format");

    let max_pic_count = std::env::var("APP_MESSAGE_SIZE")
        .unwrap_or_else(|_| "50".to_owned())
        .parse::<u8>()
        .expect("设置APP_MESSAGE_SIZE错误!");

    // 自适应抓拍间隔: 白天无人基准 / 有人高频 / 夜间无人低频
    let active_interval = read_secs_env("APP_ACTIVE_CAPTURE_INTERVAL_SECS", 60);
    let night_interval = read_secs_env("APP_NIGHT_CAPTURE_INTERVAL_SECS", 900);
    // 无新保留图超过该秒数判定"到访结束", 触发图集收尾
    let visit_end_secs = read_secs_env("APP_VISIT_END_SECS", 600);
    // 到访期间长时间有人, 每隔该秒数补一条即时提醒
    let remind_secs = read_secs_env("APP_VISIT_REMIND_SECS", 1800);
    // 躺卧姿态即时提醒: 检出即单独推送(绕过到访批次节奏), 冷却期内同设备不重复防抖动刷屏
    let lying_alert_enabled = matches!(
        std::env::var("APP_LYING_ALERT_ENABLED")
            .unwrap_or_else(|_| "on".to_owned())
            .to_lowercase()
            .as_str(),
        "on" | "true" | "1"
    );
    let lying_cooldown = Duration::from_secs(read_secs_env("APP_LYING_ALERT_COOLDOWN_SECS", 1800));

    let img_server = std::env::var("APP_IMG_SERVER").expect("未设置APP_IMG_SERVER");
    let his_path =
        std::env::var("APP_IMG_PATH_HIS").unwrap_or_else(|_| "/data/his_path".to_owned());
    let last_path =
        std::env::var("APP_IMG_PATH_LAST").unwrap_or_else(|_| "/data/last_path".to_owned());

    // 本地人员检测(YOLOv8n): 开启后轮询抓拍的图无人则丢弃, 拍到人才入库推送
    let person_filter_enabled = matches!(
        std::env::var("APP_PERSON_ALERT_ENABLED")
            .unwrap_or_default()
            .to_lowercase()
            .as_str(),
        "on" | "true" | "1"
    );
    let person_confidence = std::env::var("APP_PERSON_CONFIDENCE_THRESHOLD")
        .unwrap_or_else(|_| "0.5".to_owned())
        .parse::<f32>()
        .unwrap_or(0.5);
    let model_path = std::env::var("APP_MODEL_PATH")
        .unwrap_or_else(|_| "models/yolov8n-pose.onnx".to_owned());
    let mut person_detector: Option<PersonDetector> = if person_filter_enabled {
        match PersonDetector::new(std::path::Path::new(&model_path), person_confidence) {
            Ok(d) => {
                log_info!("轮询抓拍人员过滤已启用, 模型: {model_path}, 置信度阈值: {person_confidence}");
                Some(d)
            }
            Err(e) => {
                log_error!("人员检测初始化失败(轮询抓拍不过滤): {}", e);
                None
            }
        }
    } else {
        None
    };

    // 有人出现即时提醒: 每次到访提醒一次, 长时间在场每隔 remind_secs 补一条
    let mut last_person_alert: Option<std::time::Instant> = None;
    // 缓冲中已随"即时提醒"发过的图片数, 到访结束时若无新增则不再发图集
    let mut alerted_count: usize = 0;
    // 每设备最近一次保留"有人"图的时间(高频抓拍判定)和下次抓拍时间
    let mut person_last: HashMap<String, tokio::time::Instant> = HashMap::new();
    let mut next_due: HashMap<String, tokio::time::Instant> = HashMap::new();
    // 每设备最近一次躺卧提醒时间
    let mut lying_alert_last: HashMap<String, std::time::Instant> = HashMap::new();

    log_info!(
        "轮询抓拍模式已开启: 无人{interval}秒/有人{active_interval}秒/夜间{night_interval}秒, 到访结束{visit_end_secs}秒, token刷新周期{refresh_hours}小时, 相似度阈值{similarity_threshold}, 时间窗{start_time}~{end_time}"
    );

    let mut app = ChanMsg::new(db::get_db_pool().await);
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .expect("构建HTTP客户端失败");

    let mut token = EzToken {
        access_token: String::new(),
        refresh_at: chrono::Local::now(),
    };

    // 每个设备上一张图片的pHash, 作为相似度对比基准
    let mut prev_hash: HashMap<String, String> = HashMap::new();

    let mut urls: Vec<String> = Vec::new();
    let mut pic_count: u8 = 0;
    let mut last_add = tokio::time::Instant::now();

    loop {
        // 睡到最近一台设备到点(无计划则立即开始一轮)
        let tick_now = tokio::time::Instant::now();
        let wait = next_due
            .values()
            .min()
            .map(|t| t.saturating_duration_since(tick_now))
            .unwrap_or(Duration::ZERO);
        tokio::time::sleep(wait).await;

        // 刷新accessToken: 周期到点或token失效(置空)时重新获取
        if token.access_token.is_empty() || chrono::Local::now() >= token.refresh_at {
            match fetch_token(&client, &app_key, &app_secret).await {
                Ok(t) => {
                    token.access_token = t;
                    token.refresh_at =
                        chrono::Local::now() + chrono::Duration::hours(refresh_hours as i64);
                    log_info!("accessToken已刷新, 下次刷新: {}", token.refresh_at);
                }
                Err(e) => {
                    log_error!("获取accessToken失败: {}", e);
                    tokio::time::sleep(Duration::from_secs(interval)).await;
                    continue;
                }
            }
        }

        // 到访结束(超窗无新保留图): 有即时提醒之外的新图才汇总推送, 否则只清空缓冲
        if pic_count > 0 && last_add.elapsed() >= Duration::from_secs(visit_end_secs) {
            if usize::from(pic_count) > alerted_count {
                log_info!("到访结束推送: {pic_count}张");
                flush(&mut app, &mut urls, &mut pic_count).await;
            } else {
                log_info!("到访结束: {pic_count}张均已即时提醒, 跳过图集");
                urls.clear();
                pic_count = 0;
            }
            alerted_count = 0;
        }

        // 夜间不跳过抓拍, 只是降频(夜间有人同样即时提醒)
        let night = {
            let t = chrono::Local::now().time();
            t < start_time || t > end_time
        };

        // 获取在线设备列表
        let devices = match fetch_device_list(&client, &token.access_token).await {
            Ok(list) => list,
            Err(EzError::TokenExpired) => {
                log_info!("accessToken已过期, 下轮重新获取");
                token.access_token.clear();
                continue;
            }
            Err(e) => {
                log_error!("获取设备列表失败: {}", e);
                tokio::time::sleep(Duration::from_secs(interval)).await;
                continue;
            }
        };
        if devices.is_empty() {
            next_due.clear();
            tokio::time::sleep(Duration::from_secs(interval)).await;
            continue;
        }
        // 清理已离线设备的过期计划, 避免空转
        next_due.retain(|s, _| devices.iter().any(|(serial, _)| serial == s));

        for (serial, name) in devices {
            // 未到该设备的抓拍时间
            if let Some(due) = next_due.get(&serial) {
                if *due > tick_now {
                    continue;
                }
            }

            let pic_url = match capture(&client, &token.access_token, &serial).await {
                Ok(url) => url,
                Err(EzError::TokenExpired) => {
                    log_info!("accessToken已过期, 下轮重新获取");
                    token.access_token.clear();
                    break;
                }
                Err(e) => {
                    log_error!("设备{}抓拍失败: {}", serial, e);
                    let iv = next_capture_interval(&person_last, &serial, night, visit_end_secs, interval, active_interval, night_interval);
                    next_due.insert(serial.clone(), tokio::time::Instant::now() + Duration::from_secs(iv));
                    continue;
                }
            };

            // 下载图片数据(只下载一次, 哈希计算和落盘复用同一份数据)
            // (失败也要安排下次抓拍, 否则设备一直"已到点"会连续计费重试)
            let bytes = match client
                .get(&pic_url)
                .timeout(Duration::from_secs(10))
                .send()
                .await
            {
                Ok(resp) => match resp.bytes().await {
                    Ok(b) => b,
                    Err(e) => {
                        log_error!("下载图片数据失败: {}", e);
                        let iv = next_capture_interval(&person_last, &serial, night, visit_end_secs, interval, active_interval, night_interval);
                        next_due.insert(serial.clone(), tokio::time::Instant::now() + Duration::from_secs(iv));
                        continue;
                    }
                },
                Err(e) => {
                    log_error!("下载图片数据失败: {}", e);
                    let iv = next_capture_interval(&person_last, &serial, night, visit_end_secs, interval, active_interval, night_interval);
                    next_due.insert(serial.clone(), tokio::time::Instant::now() + Duration::from_secs(iv));
                    continue;
                }
            };

            // 计算pHash并与该设备上一张图对比
            let hash = match compute_phash_from_memory(&bytes) {
                Ok(h) => h,
                Err(e) => {
                    log_error!("计算pHash失败: {}", e);
                    let iv = next_capture_interval(&person_last, &serial, night, visit_end_secs, interval, active_interval, night_interval);
                    next_due.insert(serial.clone(), tokio::time::Instant::now() + Duration::from_secs(iv));
                    continue;
                }
            };
            let similarity = match prev_hash.get(&serial) {
                Some(prev) => phash_similarity(prev, &hash).unwrap_or(0.0),
                None => 0.0,
            };

            if similarity >= similarity_threshold {
                // 丢弃的图不更新基准, 仍与上一张基准图对比
                // (避免渐进漂移: 每张都与前一张相差不大而被连续丢弃, 偏差累积过大的图推不出去)
                log_info!("设备{}图片与基准图相似度{:.2}, 丢弃", serial, similarity);
                let iv = next_capture_interval(&person_last, &serial, night, visit_end_secs, interval, active_interval, night_interval);
                next_due.insert(serial.clone(), tokio::time::Instant::now() + Duration::from_secs(iv));
                continue;
            }

            // 本地人员检测: 无人则丢弃, 有人才保留(检测失败时放行, 宁可多推不漏报)
            let mut person_confirmed = false;
            let mut lying_detected = false;
            if let Some(ref mut detector) = person_detector {
                match detector.detect(&bytes) {
                    Ok(r) => {
                        if !r.person {
                            log_info!("设备{}图片未检测到人员, 丢弃", serial);
                            let iv = next_capture_interval(&person_last, &serial, night, visit_end_secs, interval, active_interval, night_interval);
                            next_due.insert(serial.clone(), tokio::time::Instant::now() + Duration::from_secs(iv));
                            continue;
                        }
                        person_confirmed = true;
                        lying_detected = r.lying;
                    }
                    Err(e) => {
                        log_error!("设备{}人员检测失败({}), 保留图片", serial, e);
                    }
                }
            }

            // 只有保留(推送)的图片才成为新的对比基准
            prev_hash.insert(serial.clone(), hash);
            if person_confirmed {
                person_last.insert(serial.clone(), tokio::time::Instant::now());
            }

            // message_id 列为 varchar(31), 去连字符的UUID(32位)再截到31位
            let msg_id = uuid::Uuid::new_v4().simple().to_string()[..31].to_string();
            let date_dir = chrono::Local::now().format("%Y-%m-%d").to_string();
            let id = db::insert_image_url(
                app.get_db_pool(),
                &msg_id,
                &name,
                &pic_url,
                "Capture",
                &date_dir,
            )
            .await;
            util::save_image_bytes(id, &bytes, &last_path, &his_path, &date_dir).await;
            let img_url = format!("{img_server}/{id}.jpg");
            urls.push(img_url.clone());

            // 有人出现即时提醒: 到访开始时推一条(确认有人才提醒, 检测失败的图只走图集);
            // 到访期间长时间有人, 每隔 remind_secs 补一条
            let visit_start = last_add.elapsed() >= Duration::from_secs(visit_end_secs);
            if person_confirmed
                && (visit_start
                    || last_person_alert
                        .map_or(true, |t| t.elapsed() >= Duration::from_secs(remind_secs)))
            {
                let message = format!("<img src='{img_url}' />");
                app.send(format!("⚠️ 有人出现: {name}"), message).await;
                last_person_alert = Some(std::time::Instant::now());
                alerted_count += 1;
            }

            // 躺卧姿态即时提醒: 独立通道绕过到访批次节奏, 冷却期内同设备不重复
            // (文案用中性词"躺卧", 避免危险字眼)
            if lying_detected && lying_alert_enabled {
                let due = lying_alert_last
                    .get(&serial)
                    .map_or(true, |t| t.elapsed() >= lying_cooldown);
                if due {
                    let message = format!("<img src='{img_url}' />");
                    app.send(format!("⚠️ 请关注: {name} 有人躺卧"), message).await;
                    log_info!("设备{}躺卧姿态提醒已发送", serial);
                    lying_alert_last.insert(serial.clone(), std::time::Instant::now());
                }
            }

            pic_count += 1;
            last_add = tokio::time::Instant::now();

            // 保留过有人图的设备进入高频节奏, 其余按无人/夜间节奏
            let iv = next_capture_interval(&person_last, &serial, night, visit_end_secs, interval, active_interval, night_interval);
            next_due.insert(serial.clone(), tokio::time::Instant::now() + Duration::from_secs(iv));

            if pic_count >= max_pic_count {
                log_info!("轮询抓拍满额推送: {pic_count}张");
                flush(&mut app, &mut urls, &mut pic_count).await;
                alerted_count = 0;
            }
        }
    }
}
