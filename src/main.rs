use actix_web::{web, App, HttpServer};
use models::models::AppState;
use tokio::sync::mpsc;
use std::sync::Arc;
use std::sync::Mutex;

#[macro_use]
mod logging;
mod util;
mod models;
mod db;
mod handlers;
mod routers;
mod image_dedup;
mod person_detector;
mod poller;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let (sender, receiver) = mpsc::channel(100);
    let state = Arc::new(Mutex::new(AppState { sender }));

    // 启动萤石轮询抓拍线程(APP_POLLER控制开关), panic后10秒自动重启
    tokio::spawn(async {
        loop {
            let handle = tokio::spawn(poller::run_poller());
            match handle.await {
                Ok(()) => break, // 正常返回(模式未开启)则不再重启
                Err(_) => {
                    log_error!("轮询抓拍线程异常退出, 10秒后重启");
                    tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                }
            }
        }
    });

    // 启动消息处理线程
    let state_clone = Arc::clone(&state);
    tokio::spawn(async move {
        handlers::handle_message(receiver).await;
    });

    let bind_address =
        std::env::var("APP_SERVER_ADDR").unwrap_or_else(|_| "0.0.0.0:8000".to_owned());
    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(state_clone.clone()))
            .service(web::scope("/video_message").configure(routers::webhook))
    })
    .bind(bind_address)?
    .run()
    .await
}