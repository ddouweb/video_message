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

pub(crate) async fn save_image(
    _client: &reqwest::Client,
    id: u64,
    img_url: String,
    last_path: &str,
    his_path: &str,
) {
    let filename = format!("{id}.jpg");
    let file_path = Path::new(last_path).join(&filename);

    let mut file = match File::create(&file_path).await {
        Ok(file) => file,
        Err(err) => {
            println!("无法在{}创建文件: {}", last_path, err);
            return;
        }
    };
    //let empty_certificates: Vec<reqwest::Certificate> = Vec::new();
    let response = match reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .unwrap()
        .get(&img_url)
        //.ssl_certs(empty_certificates)
        .timeout(std::time::Duration::from_secs(3))
        .send()
        .await
    {
        //let response = match reqwest::Client::new().get(&img_url).timeout(std::time::Duration::from_secs(3)).send().await {
        Ok(response) => response,
        Err(err) => {
            println!("请求图片失败: {}", err);
            return;
        }
    };

    if response.status().is_success() {
        let bytes = match response.bytes().await {
            Ok(bytes) => bytes,
            Err(err) => {
                println!("无法获取图片数据: {}", err);
                return;
            }
        };

        if let Err(err) = file.write_all(&bytes).await {
            println!("写入文件失败: {}", err);
        }
    } else {
        println!("无法下载图片: {}", response.status());
    }

    if let Err(err) = file.flush().await {
        println!("刷新文件缓冲区失败: {}", err);
        return;
    }

    // 复制文件到 history_img 文件夹
    let history_dir = format!("{}/{}",his_path,chrono::Local::now().format("%Y-%m-%d").to_string());
    // 创建或检查目标文件夹是否存在
    let _= std::fs::create_dir_all(&history_dir);
    let history_path: std::path::PathBuf = Path::new(&history_dir).join(&filename);
    if let Err(err) = std::fs::copy(&file_path, &history_path) {
        println!("复制文件失败: {}", err);
    }
}
