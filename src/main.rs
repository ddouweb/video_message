mod handlers;
mod routers;
mod util;
mod models;
mod db;

#[actix_web::main]
async fn main() -> std::io::Result<()>{
    dotenv::dotenv().ok();
    let db_pool = crate::db::get_db_pool().await;
    let bind_address = std::env::var("BIND_ADDRESS").unwrap_or_else(|_| "0.0.0.0:8000".to_owned());
    actix_web::HttpServer::new(move || {
        actix_web::App::new()
            .app_data(actix_web::web::Data::new(models::models::AppState {
                db_pool: db_pool.clone(),
                http_client: reqwest::Client::new(),
            }))
            .service(
                actix_web::web::scope("/video_message")
                    .configure(routers::webhook),
            )
    })
    .bind(bind_address)?
    .run()
    .await
}