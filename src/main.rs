use std::sync::{mpsc::channel, Arc};

use actix_web::{
    rt,
    web::{scope, Bytes, Data},
    App, HttpServer,
};
use models::models::AppState;
use reqwest::Client;

mod db;
mod handlers;
mod models;
mod routers;
mod util;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let db_pool = crate::db::get_db_pool().await;
    let (sender, receiver) = channel::<Bytes>();
    let app_state = AppState::new(db_pool, Client::new(), sender);
    let app_state = Arc::new(std::sync::Mutex::new(app_state));
    //let app_state = Arc::new(app_state);
    rt::spawn(handlers::push_messages_to_third_party(
        Arc::clone(&app_state), //app_state.clone(),
        receiver,
    ));
    let bind_address =
        std::env::var("APP_SERVER_ADDR").unwrap_or_else(|_| "0.0.0.0:8000".to_owned());
    HttpServer::new(move || {
        App::new()
            .app_data(Data::new(app_state.clone()))
            .service(scope("/video_message").configure(routers::webhook))
    })
    .bind(bind_address)?
    .run()
    .await
}