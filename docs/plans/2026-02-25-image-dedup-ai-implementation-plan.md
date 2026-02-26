# 图片去重 AI 优化实现计划

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 将 AI 特征相似度添加到图片去重功能，实现 pHash + AI 组合去重

**Architecture:** 扩展 PersonDetector 添加特征提取功能，修改 ImageDedup 同时存储 pHash 和 AI 特征，实现组合去重逻辑

**Tech Stack:** Rust, ort (ONNX Runtime), image

---

## Task 1: 扩展 PersonDetector 添加特征提取功能

**Files:**
- Modify: `src/person_detector/detector.rs`

**Step 1: 添加特征提取方法**

在 `PersonDetector` 结构体中添加方法:

```rust
/// 提取图片的特征向量（用于去重）
pub fn extract_feature(&self, image_data: &[u8]) -> Result<Vec<f32>, Box<dyn std::error::Error + Send + Sync>> {
    let session = match &self.session {
        Some(s) => s,
        None => return Ok(vec![]),
    };

    // 预处理图片
    let input_tensor = self.preprocess_image(image_data)?;

    // 运行推理，获取特征向量
    let outputs = session.run(ort::inputs! {
        self.input_name.clone() => input_tensor
    })?;

    // 获取输出
    let output = outputs.get(&self.output_name)?;
    let data = output.view();

    // MobileNet V2 输出是 [1, 1280]，提取特征向量
    let mut feature = Vec::with_capacity(1280);
    for i in 0..1280 {
        feature.push(data[i]);
    }

    Ok(feature)
}
```

**Step 2: 添加余弦相似度计算函数**

```rust
/// 计算两个特征向量的余弦相似度
pub fn cosine_similarity(vec1: &[f32], vec2: &[f32]) -> f32 {
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
```

---

## Task 2: 扩展 ImageDedup 存储 AI 特征

**Files:**
- Modify: `src/image_dedup/dedup.rs`

**Step 1: 修改缓存结构**

```rust
use std::collections::VecDeque;

pub struct DedupItem {
    pub url: String,
    pub p_hash: String,
    pub ai_feature: Vec<f32>,  // 新增：AI 特征向量
}

pub struct ImageDedup {
    threshold: u8,           // pHash 阈值
    ai_threshold: f32,      // AI 相似度阈值
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
}
```

**Step 2: 修改去重检查逻辑**

```rust
/// 检查图片是否重复（pHash + AI 组合）
pub fn is_duplicate(&self, p_hash: &str, ai_feature: &[f32]) -> bool {
    for item in &self.items {
        // 先检查 pHash
        let p_hash_dist = hamming_distance(p_hash, &item.p_hash);
        if p_hash_dist <= self.threshold {
            return true;  // pHash 相似，直接判定为重复
        }

        // pHash 不相似时，检查 AI 特征
        if !ai_feature.is_empty() && !item.ai_feature.is_empty() {
            let similarity = cosine_similarity(ai_feature, &item.ai_feature);
            if similarity >= self.ai_threshold {
                return true;  // AI 特征相似，判定为重复
            }
        }
    }
    false
}

/// 添加到缓存
pub fn add(&mut self, url: String, p_hash: String, ai_feature: Vec<f32>) {
    if self.items.len() >= self.max_size {
        self.items.pop_front();
    }
    self.items.push_back(DedupItem { url, p_hash, ai_feature });
}
```

**Step 3: 添加余弦相似度函数**

```rust
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
```

---

## Task 3: 修改 handlers.rs 集成组合去重

**Files:**
- Modify: `src/handlers.rs`

**Step 1: 修改 ImageDedup 初始化**

```rust
// 初始化图片去重
let p_hash_threshold = env::var("APP_P_HASH_THRESHOLD")
    .unwrap_or_else(|_| "5".to_owned())
    .parse::<u8>()
    .unwrap_or(5);

let ai_similarity_threshold = env::var("APP_AI_SIMILARITY_THRESHOLD")
    .unwrap_or_else(|_| "0.95".to_owned())
    .parse::<f32>()
    .unwrap_or(0.95);

let mut image_dedup = ImageDedup::new(p_hash_threshold, ai_similarity_threshold, 1000);
let http_client = reqwest::Client::new();
println!("图片去重功能已启用，pHash阈值: {}, AI相似度阈值: {}", p_hash_threshold, ai_similarity_threshold);
```

**Step 2: 修改图片处理逻辑**

```rust
Body::WarnBody(data) => {
    for picture in data.get_picture_list() {
        // 计算 pHash
        let p_hash = match compute_phash_from_url(&http_client, picture.get_url()).await {
            Ok(h) => Some(h),
            Err(e) => {
                eprintln!("计算pHash失败: {}", e);
                None
            }
        };

        // 提取 AI 特征（用于去重）
        let ai_feature = if let Some(ref detector) = person_detector {
            detector.extract_feature_from_url(&http_client, picture.get_url()).ok()
        } else {
            None
        };

        // 检查是否重复（pHash + AI 组合）
        let is_dup = if let (Some(ref ph), Some(ref feat)) = (&p_hash, &ai_feature) {
            image_dedup.is_duplicate(ph, feat)
        } else if let Some(ref ph) = p_hash {
            image_dedup.is_duplicate_phash_only(ph)
        } else {
            false
        };

        if is_dup {
            println!("过滤重复图片: {}", picture.get_url());
            continue;
        }

        // ... 保存图片逻辑 ...

        // 添加到去重缓存
        if let Some(ref ph) = p_hash {
            image_dedup.add(
                picture.get_url_string(),
                ph.clone(),
                ai_feature.unwrap_or_default(),
            );
        }

        // ... 人员检测逻辑 ...
    }
}
```

**Step 3: 在 PersonDetector 中添加 extract_feature_from_url**

```rust
/// 从 URL 下载图片并提取特征向量
pub async fn extract_feature_from_url(
    &mut self,
    client: &reqwest::Client,
    url: &str,
) -> Result<Vec<f32>, Box<dyn std::error::Error + Send + Sync>> {
    let response = client.get(url).send().await?;
    let bytes = response.bytes().await?;
    self.extract_feature(&bytes)
}
```

---

## Task 4: 编译验证

**Step 1: 检查编译**

Run: `cargo check`
Expected: SUCCESS (可能会有一些类型错误需要修复)

---

## 总结

实现计划包含 4 个任务:
1. 扩展 PersonDetector 添加特征提取功能
2. 扩展 ImageDedup 存储 AI 特征并修改去重逻辑
3. 修改 handlers.rs 集成组合去重
4. 编译验证

## 新增环境变量

| 变量名 | 说明 | 默认值 |
|-------|------|-------|
| APP_AI_SIMILARITY_THRESHOLD | AI 特征相似度阈值 | 0.95 |
