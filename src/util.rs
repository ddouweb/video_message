use std::path::Path;
use tokio::{fs::File, io::AsyncWriteExt};
pub async fn send_post_request(
    client: &reqwest::Client,
    body: String,
) -> reqwest::Result<serde_json::Value> {
    let url = "http://www.pushplus.plus/send";
    let response = client
        .post(url)
        .timeout(std::time::Duration::from_secs(3))
        .header("User-Agent", "rust_video")
        .header("Content-Type", "application/json")
        .body(body.to_owned())
        .send()
        .await?;

    let response_text = response.json().await.unwrap();
    Ok(response_text)
}

pub(crate) async fn save_image_bytes(
    id: u64,
    bytes: &[u8],
    last_path: &str,
    his_path: &str,
    data_dir: &str,
) {
    let filename = format!("{id}.jpg");
    let file_path = Path::new(last_path).join(&filename);

    let mut file = match File::create(&file_path).await {
        Ok(file) => file,
        Err(err) => {
            log_info!("无法在{}创建文件: {}", last_path, err);
            return;
        }
    };

    if let Err(err) = file.write_all(bytes).await {
        log_info!("写入文件失败: {}", err);
        return;
    }
    if let Err(err) = file.flush().await {
        log_info!("刷新文件缓冲区失败: {}", err);
        return;
    }

    // 复制文件到 history_img 文件夹
    let history_dir = format!("{}/{}", his_path, data_dir);
    let _ = std::fs::create_dir_all(&history_dir);
    let history_path: std::path::PathBuf = Path::new(&history_dir).join(&filename);
    if let Err(err) = std::fs::copy(&file_path, &history_path) {
        log_info!("复制文件失败: {}", err);
    }
}