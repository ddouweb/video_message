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

/// 返回账号下设备 (设备序列号, 设备名称, 是否在线) 列表, 含离线设备(供上下线比对)
async fn fetch_device_list(
    client: &reqwest::Client,
    access_token: &str,
) -> Result<Vec<(String, String, bool)>, EzError> {
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
                let serial = dev["deviceSerial"].as_str().unwrap_or("").to_owned();
                let name = dev["deviceName"].as_str().unwrap_or("").to_owned();
                // status: 1-在线, 其他-离线
                let online = dev["status"].as_i64() == Some(1);
                if !serial.is_empty() {
                    devices.push((serial, name, online));
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

/// 日间无人计时的基线: 取"最近一次确认有人"与"今日时间窗起点"的较晚者.
/// 昨夜有人、今晨窗起点之后没人 → 从今早窗起点起算(夜间无人属正常, 不计入连续时长);
/// 有人出现在今日窗起点之后 → 从该时刻起算
fn inactivity_baseline(
    last_seen: chrono::DateTime<chrono::Local>,
    now: chrono::DateTime<chrono::Local>,
    window_start: NaiveTime,
) -> chrono::DateTime<chrono::Local> {
    match now
        .date_naive()
        .and_time(window_start)
        .and_local_timezone(chrono::Local)
        .single()
    {
        Some(day_start) => last_seen.max(day_start),
        None => last_seen,
    }
}

/// 设备上下线提醒: 与上一轮状态比对, 返回发生翻转的设备 (序列号, 名称, 当前是否在线).
/// 首轮(prev 为空)作基线初始化不产生变化(服务重启不补报停机期间的事件);
/// 从列表消失的设备(解绑/删除)不算离线, 由调用方遗忘
fn device_status_changes(
    prev: &HashMap<String, bool>,
    current: &[(String, String, bool)],
) -> Vec<(String, String, bool)> {
    current
        .iter()
        .filter(|(serial, _, online)| prev.get(serial).map_or(false, |was| *was != *online))
        .cloned()
        .collect()
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
    // 日间长时间无人提醒: 白天连续该秒数未确认有人则推送(需人员检测开启, 检测关闭无法确认有人)
    let inactivity_alert_enabled = matches!(
        std::env::var("APP_INACTIVITY_ALERT_ENABLED")
            .unwrap_or_else(|_| "on".to_owned())
            .to_lowercase()
            .as_str(),
        "on" | "true" | "1"
    );
    let inactivity_secs = read_secs_env("APP_INACTIVITY_ALERT_SECS", 7200);
    // 设备上下线提醒: 每轮设备列表与上一轮在线状态比对, 翻转即推送; 冷却期防网络抖动刷屏
    let offline_alert_enabled = matches!(
        std::env::var("APP_OFFLINE_ALERT_ENABLED")
            .unwrap_or_else(|_| "on".to_owned())
            .to_lowercase()
            .as_str(),
        "on" | "true" | "1"
    );
    let offline_cooldown = Duration::from_secs(read_secs_env("APP_OFFLINE_ALERT_COOLDOWN_SECS", 300));

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
    // 设备上下线提醒: 每设备上一轮在线状态与最近一次提醒时刻
    let mut device_online: HashMap<String, bool> = HashMap::new();
    let mut offline_alert_last: HashMap<String, std::time::Instant> = HashMap::new();
    // 日间长时间无人: 任一设备最近一次确认有人时刻(服务启动时刻起算), 与上次无人提醒时刻
    let mut last_person_seen = chrono::Local::now();
    let mut inactivity_alert_last: Option<chrono::DateTime<chrono::Local>> = None;

    log_info!(
        "轮询抓拍模式已开启: 无人{interval}秒/有人{active_interval}秒/夜间{night_interval}秒, 到访结束{visit_end_secs}秒, token刷新周期{refresh_hours}小时, 相似度阈值{similarity_threshold}, 时间窗{start_time}~{end_time}, 日间无人提醒{}, 设备上下线提醒{}",
        if inactivity_alert_enabled { format!("{inactivity_secs}秒") } else { "关闭".to_owned() },
        if offline_alert_enabled { "开" } else { "关闭" }
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

        // 日间长时间无人提醒: 白天连续 inactivity_secs 未确认有人则推送, 持续无人每隔
        // 该时长重提一次. 基线=最近一次有人与今日窗起点的较晚者(昨夜有人今晨没人→从窗起点起算)
        if inactivity_alert_enabled && !night && person_detector.is_some() {
            let now_local = chrono::Local::now();
            let baseline = inactivity_baseline(last_person_seen, now_local, start_time);
            let absent = now_local.signed_duration_since(baseline).num_seconds();
            let re_due = inactivity_alert_last.map_or(true, |t| {
                now_local.signed_duration_since(t).num_seconds() >= inactivity_secs as i64
            });
            if absent >= inactivity_secs as i64 && re_due {
                let label = if inactivity_secs >= 3600 {
                    format!("{}小时", inactivity_secs / 3600)
                } else {
                    format!("{}分钟", inactivity_secs / 60)
                };
                app.send(
                    format!("⏰ 提醒: 日间已连续{label}未发现人员出现"),
                    "白天长时间未监测到人员活动, 请关注".to_owned(),
                )
                .await;
                log_info!("日间连续{label}无人提醒已发送");
                inactivity_alert_last = Some(now_local);
            }
        }

        // 获取设备列表(含离线设备)
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

        // 设备上下线提醒: 与上一轮比对, 状态翻转即推送; 冷却期内同设备不重复(防网络抖动刷屏)
        // device/list 调用失败不会走到这里, 自身网络故障不会误报"全部离线"
        if offline_alert_enabled {
            for (serial, name, online) in device_status_changes(&device_online, &devices) {
                let display = if name.is_empty() { serial.clone() } else { name.clone() };
                let cooled = offline_alert_last
                    .get(&serial)
                    .map_or(true, |t| t.elapsed() >= offline_cooldown);
                if cooled {
                    let now = chrono::Local::now().format("%Y-%m-%d %H:%M");
                    if online {
                        app.send(
                            format!("✅ 设备恢复在线: {display}"),
                            format!("设备已恢复在线, 抓拍监控继续<br/>{now}"),
                        )
                        .await;
                    } else {
                        app.send(
                            format!("⚠️ 设备离线: {display}"),
                            format!("设备已离线, 抓拍监控暂停, 请检查电源或网络<br/>{now}"),
                        )
                        .await;
                    }
                    log_info!("设备{serial}({display})状态变化: {}", if online { "恢复在线" } else { "离线" });
                    offline_alert_last.insert(serial.clone(), std::time::Instant::now());
                } else {
                    log_info!("设备{serial}状态变化(冷却期内不重复提醒)");
                }
            }
        }
        // 更新状态基线; 从列表消失的设备(解绑/删除)遗忘
        device_online.retain(|s, _| devices.iter().any(|(serial, _, _)| serial == s));
        for (serial, _, online) in &devices {
            device_online.insert(serial.clone(), *online);
        }

        if devices.is_empty() {
            next_due.clear();
            tokio::time::sleep(Duration::from_secs(interval)).await;
            continue;
        }
        // 只对在线设备抓拍; 全部离线时按基准间隔重查列表(等恢复), 防止 next_due 清空后热循环
        let online_devices: Vec<(String, String)> = devices
            .iter()
            .filter(|(_, _, online)| *online)
            .map(|(serial, name, _)| (serial.clone(), name.clone()))
            .collect();
        if online_devices.is_empty() {
            next_due.clear();
            tokio::time::sleep(Duration::from_secs(interval)).await;
            continue;
        }
        // 清理已离线设备的过期计划, 避免空转
        next_due.retain(|s, _| online_devices.iter().any(|(serial, _)| serial == s));

        for (serial, name) in online_devices {
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
                last_person_seen = chrono::Local::now();
                inactivity_alert_last = None; // 新一轮无人计时, 不受上次提醒时刻压制
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

            // 躺卧姿态即时提醒: 独立通道绕过到访批次节奏, 冷却期内同设备不重复
            // (文案用中性词"躺卧", 避免危险字眼)
            // 该图已推躺卧提醒时不再叠加"有人出现"(同图双推是噪音, 躺卧提醒已告知有人在场),
            // 且计入已提醒数, 到访收尾图集不再重复发它
            let mut lying_sent = false;
            if lying_detected && lying_alert_enabled {
                let due = lying_alert_last
                    .get(&serial)
                    .map_or(true, |t| t.elapsed() >= lying_cooldown);
                if due {
                    let message = format!("<img src='{img_url}' />");
                    app.send(format!("⚠️ 请关注: {name} 有人躺卧"), message).await;
                    log_info!("设备{}躺卧姿态提醒已发送", serial);
                    lying_alert_last.insert(serial.clone(), std::time::Instant::now());
                    lying_sent = true;
                    alerted_count += 1;
                    last_person_alert = Some(std::time::Instant::now());
                }
            }

            // 有人出现即时提醒: 到访开始时推一条(确认有人才提醒, 检测失败的图只走图集);
            // 到访期间长时间有人, 每隔 remind_secs 补一条
            let visit_start = last_add.elapsed() >= Duration::from_secs(visit_end_secs);
            if person_confirmed
                && !lying_sent
                && (visit_start
                    || last_person_alert
                        .map_or(true, |t| t.elapsed() >= Duration::from_secs(remind_secs)))
            {
                let message = format!("<img src='{img_url}' />");
                app.send(format!("⚠️ 有人出现: {name}"), message).await;
                last_person_alert = Some(std::time::Instant::now());
                alerted_count += 1;
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn baseline_is_today_window_start_when_person_seen_yesterday() {
        // 昨晚21:00最后见人, 今日窗起点7:00后无人: 夜间不算, 基线=今日07:00, 09:00触发
        let now = chrono::Local.with_ymd_and_hms(2026, 11, 18, 10, 0, 0).unwrap();
        let last_seen = chrono::Local.with_ymd_and_hms(2026, 11, 17, 21, 0, 0).unwrap();
        let b = inactivity_baseline(last_seen, now, NaiveTime::from_hms_opt(7, 0, 0).unwrap());
        assert_eq!(b, chrono::Local.with_ymd_and_hms(2026, 11, 18, 7, 0, 0).unwrap());
    }

    #[test]
    fn baseline_is_last_seen_when_inside_today_window() {
        // 今日09:30见过人, 之后无人: 基线=09:30(比窗起点晚, 取有人时刻)
        let now = chrono::Local.with_ymd_and_hms(2026, 11, 18, 12, 0, 0).unwrap();
        let last_seen = chrono::Local.with_ymd_and_hms(2026, 11, 18, 9, 30, 0).unwrap();
        let b = inactivity_baseline(last_seen, now, NaiveTime::from_hms_opt(7, 0, 0).unwrap());
        assert_eq!(b, last_seen);
    }

    #[test]
    fn baseline_is_window_start_when_service_started_before_window() {
        // 服务启动于06:00(窗起点前), 一直无人: 基线=07:00窗起点而非启动时刻
        let now = chrono::Local.with_ymd_and_hms(2026, 11, 18, 9, 0, 0).unwrap();
        let started = chrono::Local.with_ymd_and_hms(2026, 11, 18, 6, 0, 0).unwrap();
        let b = inactivity_baseline(started, now, NaiveTime::from_hms_opt(7, 0, 0).unwrap());
        assert_eq!(b, chrono::Local.with_ymd_and_hms(2026, 11, 18, 7, 0, 0).unwrap());
    }

    #[test]
    fn status_changes_first_scan_is_baseline() {
        // 首轮扫描只建立基线, 不产生变化(重启不补报停机期间的事件)
        let cur = vec![
            ("S1".into(), "门口".into(), true),
            ("S2".into(), "客厅".into(), false),
        ];
        assert!(device_status_changes(&HashMap::new(), &cur).is_empty());
    }

    #[test]
    fn status_changes_detect_both_directions() {
        // S2 离线->在线, S3 在线->离线, S1 不变
        let prev = HashMap::from([
            ("S1".to_string(), true),
            ("S2".to_string(), false),
            ("S3".to_string(), true),
        ]);
        let cur = vec![
            ("S1".into(), "门口".into(), true),
            ("S2".into(), "客厅".into(), true),
            ("S3".into(), "院子".into(), false),
        ];
        let changes = device_status_changes(&prev, &cur);
        assert_eq!(changes.len(), 2);
        assert!(changes.iter().any(|(s, _, on)| s == "S2" && *on));
        assert!(changes.iter().any(|(s, _, on)| s == "S3" && !*on));
    }

    #[test]
    fn status_changes_ignore_vanished_device() {
        // 从列表消失(解绑/删除)不算离线事件
        let prev = HashMap::from([("S1".to_string(), true)]);
        let cur: Vec<(String, String, bool)> = vec![("S2".into(), "新设备".into(), true)];
        assert!(device_status_changes(&prev, &cur).is_empty());
    }
}
