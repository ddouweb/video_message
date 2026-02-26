use img_hash::{HasherConfig, HashAlg};
use std::path::Path;

/// 计算图片的 pHash 值
pub fn compute_phash(image_path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let hasher = HasherConfig::new()
        .hash_alg(HashAlg::DoubleGradient)
        .hash_size(8, 8)
        .to_hasher();

    let hash = hasher.hash_image(image_path);
    Ok(hash.to_base64())
}

/// 从 URL 下载图片并计算 pHash
pub async fn compute_phash_from_url(client: &reqwest::Client, url: &str) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    use uuid::Uuid;

    let temp_dir = std::env::temp_dir();
    let temp_file = temp_dir.join(format!("phash_{}.jpg", Uuid::new_v4()));

    let response = client.get(url).send().await?;
    let bytes = response.bytes().await?;

    std::fs::write(&temp_file, &bytes)?;

    let hash = compute_phash(&temp_file)?;

    // 清理临时文件
    let _ = std::fs::remove_file(&temp_file);

    Ok(hash)
}
