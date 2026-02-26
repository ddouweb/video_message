# 图片去重功能设计文档

## 概述

为 Video Message 项目增加图片去重功能，使用感知哈希(pHash)算法识别视觉上相似的图片，避免重复保存和发送相同的图片。

## 需求背景

- 萤石云摄像头可能频繁触发相似告警
- 重复图片浪费存储空间和微信推送配额
- 用户希望过滤视觉上相似的图片

## 技术方案

### 1. pHash 算法

感知哈希(Perceptual Hash)的工作原理：
1. 将图片缩小到 32x32 像素
2. 转换为灰度图
3. 计算 DCT 变换，取低频部分
4. 计算中值，得到64位哈希值
5. 比较两个哈希值的汉明距离(Hamming Distance)

### 2. 相似度阈值

- 阈值范围: 0-64
- 默认值: 5
- 差异值 ≤ 阈值视为相似

## 架构设计

### 模块结构

```
src/
├── image_dedup/          # 新增模块
│   ├── mod.rs           # 模块入口
│   ├── phash.rs         # pHash算法实现
│   └── dedup.rs         # 去重逻辑
```

### 核心结构

```rust
// src/image_dedup/dedup.rs
pub struct ImageDedup {
    threshold: u8,           // 相似度阈值(环境变量配置)
    hashes: Vec<(String, String)>,  // (图片URL, pHash值)
}

impl ImageDedup {
    pub fn new(threshold: u8) -> Self
    pub fn add_image(&mut self, url: String, hash: String) -> bool  // 返回是否重复
    pub fn is_similar(&self, new_hash: &str) -> bool
    pub fn clear(&mut self)  // 清空缓存
}
```

## 数据流设计

### 1. 收到图片消息时

```
收到新图片URL
    ↓
下载图片到临时文件
    ↓
计算pHash值
    ↓
与缓存中的图片比较
    ↓                      ↓
  相似(≤阈值)            不相似(>阈值)
    ↓                      ↓
  丢弃(不计数)          保留 → 加入缓存 + pic_count+1
```

### 2. 发送消息时

```
达到发送条件(pic_count >= max_pic_count)
    ↓
发送微信消息
    ↓
清空去重缓存
```

### 3. 超时时

```
超时触发发送
    ↓
发送微信消息
    ↓
清空去重缓存
```

## 配置项

| 环境变量 | 说明 | 默认值 |
|---------|------|-------|
| APP_P_HASH_THRESHOLD | pHash相似度阈值(0-64) | 5 |

## 实现步骤

1. 新增 `image_dedup` 模块
2. 实现 pHash 算法
3. 实现去重逻辑
4. 集成到 handlers.rs 的图片处理流程
5. 在发送消息后清空缓存

## 注意事项

- 被过滤的图片不计入 `pic_count`
- 缓存生命周期与 `APP_MESSAGE_SIZE` 发送周期同步
- 下载图片失败时，记录日志但不中断流程
