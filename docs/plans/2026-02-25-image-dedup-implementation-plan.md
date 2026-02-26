# 图片去重功能实现计划

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 为 Video Message 项目添加图片去重功能，使用 pHash 算法识别视觉相似的图片

**Architecture:** 新增 `image_dedup` 模块，实现 pHash 算法并在图片保存和发送两个阶段进行去重

**Tech Stack:** Rust, image crate (图像处理), img_hash crate (pHash 实现)

---

## Task 1: 添加 pHash 依赖

**Files:**
- Modify: `Cargo.toml:6`

**Step 1: 添加依赖**

```toml
[dependencies]
img_hash = "3.2"
image = "0.24"
```

**Step 2: 验证依赖可以编译**

Run: `cargo check`
Expected: SUCCESS

---

## Task 2: 创建 image_dedup 模块

**Files:**
- Create: `src/image_dedup/mod.rs`
- Create: `src/image_dedup/phash.rs`
- Create: `src/image_dedup/dedup.rs`

**Step 1: 创建模块入口**

```rust
// src/image_dedup/mod.rs
pub mod phash;
pub mod dedup;

pub use dedup::ImageDedup;
```

**Step 2: 创建 dedup.rs**

```rust
// src/image_dedup/dedup.rs
use std::collections::VecDeque;

pub struct ImageDedup {
    threshold: u8,
    hashes: VecDeque<(String, String)>,  // (图片URL, pHash值)
    max_size: usize,
}

impl ImageDedup {
    pub fn new(threshold: u8, max_size: usize) -> Self {
        Self {
            threshold,
            hashes: VecDeque::new(),
            max_size,
        }
    }

    /// 检查图片是否与缓存中的图片相似
    /// 返回 true 表示相似（应该过滤掉）
    pub fn is_duplicate(&self, hash: &str) -> bool {
        for (_, existing_hash) in &self.hashes {
            let distance = hamming_distance(hash, existing_hash);
            if distance <= self.threshold {
                return true;
            }
        }
        false
    }

    /// 添加图片到缓存
    pub fn add(&mut self, url: String, hash: String) {
        if self.hashes.len() >= self.max_size {
            self.hashes.pop_front();
        }
        self.hashes.push_back((url, hash));
    }

    /// 清空缓存
    pub fn clear(&mut self) {
        self.hashes.clear();
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
```

**Step 3: 创建 phash.rs**

```rust
// src/image_dedup/phash.rs
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
    let temp_dir = std::env::temp_dir();
    let temp_file = temp_dir.join(format!("phash_{}.jpg", uuid::Uuid::new_v4()));

    let response = client.get(url).send().await?;
    let bytes = response.bytes().await?;

    std::fs::write(&temp_file, &bytes)?;

    let hash = compute_phash(&temp_file)?;

    // 清理临时文件
    let _ = std::fs::remove_file(&temp_file);

    Ok(hash)
}
```

**Step 4: 添加 uuid 依赖**

在 Cargo.toml 中添加:
```toml
uuid = { version = "1.0", features = ["v4"] }
```

**Step 5: 验证编译**

Run: `cargo check`
Expected: SUCCESS

---

## Task 3: 集成去重到 handlers.rs

**Files:**
- Modify: `src/handlers.rs:1-213`

**Step 1: 添加模块引用和初始化**

在文件开头添加:
```rust
mod image_dedup;
use image_dedup::{compute_phash_from_url, ImageDedup};
```

在 `handle_message` 函数中添加初始化:
```rust
let p_hash_threshold = env::var("APP_P_HASH_THRESHOLD")
    .unwrap_or_else(|_| "5".to_owned())
    .parse::<u8()
    .unwrap_or(5);
let mut image_dedup = ImageDedup::new(p_hash_threshold, 1000);
let client = reqwest::Client::new();
```

**Step 2: 修改图片处理逻辑**

在 `Body::WarnBody(data)` 处理中:
```rust
Body::WarnBody(data) => {
    for picture in data.get_picture_list() {
        // 计算 pHash
        let p_hash = match compute_phash_from_url(&client, picture.get_url()).await {
            Ok(h) => h,
            Err(e) => {
                eprintln!("计算pHash失败: {}", e);
                None
            }
        };

        // 检查是否重复
        let is_dup = p_hash.as_ref().map(|h| image_dedup.is_duplicate(h)).unwrap_or(false);

        if is_dup {
            println!("过滤重复图片: {}", picture.get_url());
            continue;  // 跳过，不计入 pic_count
        }

        // 保存图片
        let id = crate::db::insert_image_url(
            app.get_db_pool(),
            msg_id,
            data.get_channel_name(),
            picture.get_url(),
            body.get_name(),
            &date_dir,
        )
        .await;
        app.save_image(id, picture.get_url_string(), &date_dir).await;
        urls.push(format!("{img_server}/{id}.jpg"));

        // 添加到去重缓存
        if let Some(ref h) = p_hash {
            image_dedup.add(picture.get_url_string(), h.clone());
        }

        pic_count += 1;
    }
}
```

**Step 3: 修改发送后清空缓存逻辑**

在 `pic_count >= max_pic_count` 发送后:
```rust
if pic_count >= max_pic_count {
    let combined_messages = urls
        .iter()
        .map(|url| format!("<img src='{}' />", url))
        .collect::<Vec<String>>()
        .join(" ");
    app.send(format!("{}张图片抓拍", pic_count), combined_messages)
        .await;
    pic_count = 0;
    urls.clear();
    image_dedup.clear();  // 清空去重缓存
}
```

在超时发送后同样清空:
```rust
if pic_count >= current_compare_pic_count {
    let combined_messages = urls
        .iter()
        .map(|url| format!("<img src='{}' />", url))
        .collect::<Vec<String>>()
        .join(" ");
    app.send(format!("{}张图片抓拍", pic_count), combined_messages)
        .await;
    pic_count = 0;
    urls.clear();
    image_dedup.clear();  // 清空去重缓存
}
```

**Step 4: 验证编译**

Run: `cargo check`
Expected: SUCCESS

---

## Task 4: 编译和测试

**Step 1: 完整编译**

Run: `cargo build --release`
Expected: SUCCESS

**Step 2: 测试运行**

Run: `cargo run`
Expected: 启动成功，日志中显示 "图片去重功能已启用"

---

## 总结

实现计划包含 4 个任务:
1. 添加 pHash 依赖 (Cargo.toml)
2. 创建 image_dedup 模块 (3个新文件)
3. 集成去重逻辑到 handlers.rs
4. 编译测试
