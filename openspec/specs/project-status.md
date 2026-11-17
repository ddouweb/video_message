# 项目现状描述

## 项目概述

- **项目名称**: videoMsg
- **项目类型**: Rust Web 服务 (actix-web)
- **核心功能**: 视频消息/图片抓拍处理系统，接收 webhook 消息，处理图片去重、人员检测等功能

## 技术栈

- **后端框架**: actix-web 4.0.0
- **运行时**: tokio (异步)
- **数据库**: MySQL (sqlx)
- **HTTP 客户端**: reqwest
- **ML 框架**: tract-onnx (ONNX 推理)
- **图片处理**: image, img_hash

## 核心功能

### 1. Webhook 接收
- 端点: `POST /video_message/webhook`
- 接收消息格式: JSON (Message 结构)

### 2. 图片去重
- **pHash 感知哈希**: 快速相似图片检测
- **AI 特征提取**: 使用 ONNX 模型提取特征进行相似度判断
- 配置参数:
  - `APP_P_HASH_THRESHOLD`: pHash 阈值 (默认 5)
  - `APP_AI_SIMILARITY_THRESHOLD`: AI 相似度阈值 (默认 0.95)

### 3. 人员检测
- 使用 MobileNetV2 ONNX 模型检测图片中是否有人
- 检测到人员时发送告警消息
- 配置参数:
  - `APP_PERSON_ALERT_ENABLED`: 是否启用 (默认 true)
  - `APP_PERSON_CONFIDENCE_THRESHOLD`: 置信度阈值 (默认 0.7)
  - `APP_MODEL_PATH`: 模型路径 (默认 models/mobilenetv2.onnx)

### 4. 消息处理
- 支持多种消息类型: WarnBody, OnOffLine, Call
- 定时批量发送图片 (达到数量或超时)
- 工作时间控制 (APP_START_TIME / APP_END_TIME)

## 项目结构

```
src/
├── main.rs          # 入口，Web 服务器启动
├── routers.rs       # 路由配置
├── handlers.rs      # 请求处理逻辑
├── db.rs            # 数据库操作
├── util.rs          # 工具函数
├── models/          # 数据模型
│   ├── models.rs    # AppState, Message, Body 等
│   ├── warn.rs      # 告警消息
│   ├── call.rs      # 呼叫消息
│   └── ...
├── image_dedup/     # 图片去重模块
│   ├── dedup.rs     # 去重逻辑
│   ├── phash.rs     # pHash 计算
│   └── mod.rs
└── person_detector/ # 人员检测模块
    ├── detector.rs  # ONNX 推理
    └── mod.rs
```

## 环境变量

| 变量 | 说明 | 默认值 |
|------|------|--------|
| APP_SERVER_ADDR | 服务地址 | 0.0.0.0:8000 |
| APP_IMG_SERVER | 图片服务器地址 | (必填) |
| APP_API_TIMEOUT | API 超时时间(秒) | 600 |
| APP_START_TIME | 工作开始时间 | 7:00:00 |
| APP_END_TIME | 工作结束时间 | 23:00:00 |
| APP_MIN_PIC_COUNT | 最小图片数量 | 10 |
| APP_MESSAGE_SIZE | 最大图片数量 | 50 |

## 待开发/优化方向

> 可在此记录未来提案方向
