use std::collections::VecDeque;

pub struct DedupItem {
    pub p_hash: String,
    pub ai_feature: Vec<f32>,
}

pub struct ImageDedup {
    threshold: u8,
    ai_threshold: f32,
    items: VecDeque<DedupItem>,
    max_size: usize,
}

impl ImageDedup {
    pub fn new(threshold: u8, ai_threshold: f32, max_size: usize) -> Self {
        Self {
            threshold,
            ai_threshold,
            items: VecDeque::new(),
            max_size,
        }
    }

    /// 检查图片是否重复（pHash + AI 组合）
    pub fn is_duplicate(&self, p_hash: &str, ai_feature: &[f32]) -> bool {
        for item in &self.items {
            // 先检查 pHash
            let p_hash_dist = hamming_distance(p_hash, &item.p_hash);
            if p_hash_dist <= self.threshold {
                return true;
            }

            // pHash 不相似时，检查 AI 特征
            if !ai_feature.is_empty() && !item.ai_feature.is_empty() {
                let similarity = cosine_similarity(ai_feature, &item.ai_feature);
                if similarity >= self.ai_threshold {
                    return true;
                }
            }
        }
        false
    }

    /// 只用 pHash 检查重复（当没有 AI 特征时使用）
    pub fn is_duplicate_phash(&self, p_hash: &str) -> bool {
        for item in &self.items {
            let p_hash_dist = hamming_distance(p_hash, &item.p_hash);
            if p_hash_dist <= self.threshold {
                return true;
            }
        }
        false
    }

    /// 添加图片到缓存
    pub fn add(&mut self, p_hash: String, ai_feature: Vec<f32>) {
        if self.items.len() >= self.max_size {
            self.items.pop_front();
        }
        self.items.push_back(DedupItem { p_hash, ai_feature });
    }

    /// 清空缓存
    pub fn clear(&mut self) {
        self.items.clear();
    }
}

/// 计算两个哈希值的汉明距离
fn hamming_distance(hash1: &str, hash2: &str) -> u8 {
    let bytes1 = hash1.as_bytes();
    let bytes2 = hash2.as_bytes();
    let mut distance = 0u8;

    for (b1, b2) in bytes1.iter().zip(bytes2.iter()) {
        let xor = b1 ^ b2;
        distance += xor.count_ones() as u8;
    }

    distance
}

/// 计算两个特征向量的余弦相似度
fn cosine_similarity(vec1: &[f32], vec2: &[f32]) -> f32 {
    if vec1.is_empty() || vec2.is_empty() {
        return 0.0;
    }

    let dot_product: f32 = vec1.iter().zip(vec2.iter()).map(|(a, b)| a * b).sum();
    let magnitude1: f32 = vec1.iter().map(|x| x * x).sum::<f32>().sqrt();
    let magnitude2: f32 = vec2.iter().map(|x| x * x).sum::<f32>().sqrt();

    if magnitude1 == 0.0 || magnitude2 == 0.0 {
        return 0.0;
    }

    dot_product / (magnitude1 * magnitude2)
}
