use actix_web::{web, App, HttpServer};
use models::models::AppState;
use tokio::sync::mpsc;
use std::sync::Arc;
use std::sync::Mutex;

mod util;
mod models;
mod db;
mod handlers;
mod routers;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let (sender, receiver) = mpsc::channel(100);
    let state = Arc::new(Mutex::new(AppState { sender }));

    // 启动消息处理线程
    let state_clone = Arc::clone(&state);
    tokio::spawn(async move {
        handlers::handle_message(receiver).await;
    });

    println!("应用即将启动：{:?}",chrono::Local::now().naive_local());


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