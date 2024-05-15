pub(crate) async fn insert_message(
    db_pool: &sqlx::Pool<sqlx::MySql>,
    header: &crate::models::models::Header,
    body: Option<&crate::models::models::Body>,
    data_type: &str,
) {
    match body {
        Some(body) => {
            let query = r#"
        INSERT INTO message_receive 
        (message_id, device_id, channel_no, `type`, message_time, body, data_type)
        VALUES (?, ?, ?, ?, ?, ?, ?)
        RETURNING id, message_id, device_id, channel_no, type"#;
            let message_id = header.get_message_id();
            let device_id = header.get_device_id();
            let channel_no = header.get_channel_no();
            let r#type = header.get_type();
            let message_time = header.get_message_time();
            let body = serde_json::to_string(body).unwrap();
            let data_type = data_type;
            let _row = sqlx::query(query)
                .bind(message_id)
                .bind(device_id)
                .bind(channel_no)
                .bind(r#type)
                .bind(message_time)
                .bind(body)
                .bind(data_type)
                .fetch_one(db_pool)
                .await
                .unwrap();
        }
        None => {}
    }
}

pub(crate) async fn insert_image_url(
    db_pool: &sqlx::Pool<sqlx::MySql>,
    msg_id: &str,
    channel_name: &str,
    url: &str,
    data_type: &str,
) {
    sqlx::query(
        r#"INSERT INTO message_img 
    (message_id, channel_name, url, `data_type`)
    VALUES(?, ?, ?, ?) 
    RETURNING message_id, channel_name, url, data_type,id"#,
    )
    .bind(msg_id)
    .bind(channel_name)
    .bind(url)
    .bind(data_type)
    .fetch_one(db_pool)
    .await
    .unwrap();
}

pub(crate) async fn get_db_pool() -> sqlx::Pool<sqlx::MySql> {
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL 没有在 .env 文件里设置");
    sqlx::MySqlPool::connect(&database_url)
            .await
            .expect("Failed to connect to database")
}

// pub(crate) fn get_db_pool() -> sqlx::Pool<sqlx::MySql> {
//     dotenv::dotenv().ok();
//     let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL 没有在 .env 文件里设置");

//     let rt = tokio::runtime::Builder::new_current_thread()
//         .enable_all()
//         .build()
//         .expect("Failed to create Tokio runtime.");

//     rt.block_on(async {
//         sqlx::Pool::connect(&database_url)
//             .await
//             .expect("Failed to connect to database")
//     })
// }