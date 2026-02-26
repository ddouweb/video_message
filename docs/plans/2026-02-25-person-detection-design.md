# 人员检测提醒功能设计文档

## 概述

在图片去重功能的基础上，增加人员检测提醒功能。当在一次消息生命周期中首次检测到人物时，立即发送单独的提醒消息。

## 需求背景

- 用户希望第一时间知道摄像头检测到人员出现
- 在批量发送图片前先发送人员提醒，提高时效性
- 与现有的图片去重功能配合使用

## 技术方案

### 1. AI 图像识别

使用图像分类模型进行人员检测：
- 使用预训练的 MobileNet V2 模型
- 识别图片中是否包含"人物"类别
- 置信度阈值可配置

### 2. 相似度阈值

- 默认置信度阈值: 0.7 (70%)
- 可通过环境变量配置

## 架构设计

### 模块结构

```
src/
├── person_detector/      # 新增模块
│   ├── mod.rs           # 模块入口
│   └── detector.rs      # 人员检测逻辑
```

### 核心结构

```rust
// src/person_detector/detector.rs
pub struct PersonDetector {
    model: ...,              // 图像分类模型
    class_labels: Vec<String>,  // 类别标签
    confidence_threshold: f32,   // 置信度阈值
    has_sent_alert: bool,       // 当前周期是否已发送提醒
}

impl PersonDetector {
    pub fn new(confidence_threshold: f32) -> Self
    pub async fn detect(&mut self, image_data: &[u8]) -> bool
    pub fn reset(&mut self)  // 发送后重置
    pub fn has_person(&self) -> bool  // 当前周期是否已检测到人
}
```

## 数据流设计

### 1. 图片处理流程（修改版）

```
收到图片
    ↓
并行执行:
    ├── pHash 计算（去重）
    └── 人员检测
    ↓
人员检测结果:
    ↓                           ↓
首次检测到人               非首次/未检测到人
    ↓                           ↓
立即发送"人员出现"提醒     继续
    ↓
重置 has_sent_alert = true
    ↓
继续批量流程（图片仍保留）
```

### 2. 批量发送后

```
批量发送成功后
    ↓
重置 has_sent_alert = false（开始新周期）
```

### 3. 超时发送后

```
超时发送后
    ↓
重置 has_sent_alert = false（开始新周期）
```

## 配置项

| 环境变量 | 说明 | 默认值 |
|---------|------|-------|
| APP_PERSON_ALERT_ENABLED | 是否启用人员检测提醒 | true |
| APP_PERSON_CONFIDENCE_THRESHOLD | 人员检测置信度阈值 | 0.7 |

## 消息格式

**人员出现提醒:**
- 标题: "⚠️ 人员出现"
- 内容: 带图片的 HTML，格式与批量消息相同

## 实现步骤

1. 新增 `person_detector` 模块
2. 实现人员检测逻辑（使用图像分类模型）
3. 集成到 handlers.rs 的图片处理流程
4. 首次检测到人时立即发送提醒
5. 发送后/周期结束时重置状态

## 注意事项

- 人员检测与图片去重并行执行，提高效率
- 检测到人的图片同时保留在批量消息中
- 只在一次周期的第一次检测到人时发送提醒
- 批量发送后自动开始新的检测周期
