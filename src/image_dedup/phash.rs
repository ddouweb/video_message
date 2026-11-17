use img_hash::{HasherConfig, HashAlg};
use image::GenericImageView;

/// 计算图片的 pHash 值（从内存数据）
pub fn compute_phash_from_memory(image_data: &[u8]) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let hasher = HasherConfig::new()
        .hash_alg(HashAlg::DoubleGradient)
        .hash_size(8, 8)
        .to_hasher();

    // 从内存加载图片
    let img = image::load_from_memory(image_data)?;
    let hash = hasher.hash_image(&img);
    Ok(hash.to_base64())
}

/// 从 URL 下载图片并计算 pHash
pub async fn compute_phash_from_url(client: &reqwest::Client, url: &str) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let response = client.get(url).send().await?;
    let bytes = response.bytes().await?;

    compute_phash_from_memory(&bytes)
}

/// 计算两个 pHash 字符串的相似度（0.0~1.0，1.0 表示完全相同）
pub fn phash_similarity(hash1: &str, hash2: &str) -> Result<f64, Box<dyn std::error::Error + Send + Sync>> {
    let h1 = img_hash::ImageHash::<Box<[u8]>>::from_base64(hash1)
        .map_err(|e| format!("无效的pHash(base64解码失败): {e:?}"))?;
    let h2 = img_hash::ImageHash::<Box<[u8]>>::from_base64(hash2)
        .map_err(|e| format!("无效的pHash(base64解码失败): {e:?}"))?;
    let bits = (h1.as_bytes().len() * 8) as f64;
    if bits == 0.0 {
        return Ok(0.0);
    }
    Ok(1.0 - h1.dist(&h2) as f64 / bits)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 生成一张灰度渐变测试图(32x32 PNG)
    fn gradient_png(vertical: bool) -> Vec<u8> {
        let mut img = image::GrayImage::new(32, 32);
        for y in 0..32u32 {
            for x in 0..32u32 {
                let v = if vertical { (y * 8) as u8 } else { (x * 8) as u8 };
                img.put_pixel(x, y, image::Luma([v]));
            }
        }
        let img = image::DynamicImage::ImageLuma8(img);
        let mut buf = std::io::Cursor::new(Vec::new());
        img.write_to(&mut buf, image::ImageOutputFormat::Png).unwrap();
        buf.into_inner()
    }

    #[test]
    fn same_image_similarity_is_one() {
        let bytes = gradient_png(true);
        let h1 = compute_phash_from_memory(&bytes).unwrap();
        let h2 = compute_phash_from_memory(&bytes).unwrap();
        let sim = phash_similarity(&h1, &h2).unwrap();
        assert!((sim - 1.0).abs() < 1e-9, "同一张图相似度应为1.0, 实际: {sim}");
    }

    #[test]
    fn different_gradient_similarity_below_one() {
        let h1 = compute_phash_from_memory(&gradient_png(true)).unwrap();
        let h2 = compute_phash_from_memory(&gradient_png(false)).unwrap();
        let sim = phash_similarity(&h1, &h2).unwrap();
        assert!(sim < 1.0, "不同方向的渐变图相似度应小于1.0, 实际: {sim}");
    }
}
