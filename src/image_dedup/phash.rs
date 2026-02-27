use img_hash::{HasherConfig, HashAlg};

/// 计算图片的 pHash 值（从内存数据）
pub fn compute_phash_from_memory(image_data: &[u8]) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let hasher = HasherConfig::new()
        .hash_alg(HashAlg::DoubleGradient)
        .hash_size(8, 8)
        .to_hasher();

    // 从内存加载图片
    let img = img_hash::image::load_from_memory(image_data)?;
    let hash = hasher.hash_image(&img);
    Ok(hash.to_base64())
}

/// 从 URL 下载图片并计算 pHash
pub async fn compute_phash_from_url(client: &reqwest::Client, url: &str) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let response = client.get(url).send().await?;
    let bytes = response.bytes().await?;

    compute_phash_from_memory(&bytes)
}
