# 项目现状描述

## 项目概述

- **项目名称**: videoMsg
- **项目类型**: Rust Web 服务 (actix-web)，静态编译 (x86_64-musl) 运行于 scratch 容器
- **核心功能**: 接收萤石云摄像头消息并推送到微信 (PushPlus)。支持两条通道：云推送回调 (webhook) 与轮询抓拍 (poller，萤石云停止推送时的自建通道)，均带图片去重与本地人员检测

## 技术栈

- **后端框架**: actix-web 4.0.0
- **运行时**: tokio (异步)
- **数据库**: MySQL/MariaDB (sqlx)
- **HTTP 客户端**: reqwest
- **ML 框架**: tract-onnx 0.21 (ONNX 推理)
- **图片处理**: image 0.23 (须与 img_hash 3.2 依赖上限配套，勿升 0.24), img_hash
- **构建**: Docker 多阶段 (rust:latest → scratch)；GitHub Actions 推送 feature 分支/tags 自动构建并发 Release

## 核心功能

### 1. Webhook 接收（云推送回调通道）
- 端点: `POST /video_message/webhook`
- 接收消息格式: JSON (Message 结构)，支持 WarnBody / OnOffLine / Call
- WarnBody 告警附图默认为 `hikencod` 加密格式（需在萤石 App 关闭图片加密才能解码）：加密图不入库不推送，整事件仅剩加密图时降级为纯文字提醒
- 即时提醒冷却期 `APP_ALERT_COOLDOWN_SECS` (默认 600s)

### 2. 轮询抓拍模式（poller，自建通道）
- `APP_POLLER=on` 启用；accessToken 自动刷新（周期 `APP_TOKEN_REFRESH_HOURS`，失效自动重取）
- 每轮拉取在线设备列表 (device/list，仅 status==1)，**按设备独立到点调度**抓拍 (device/capture)
- 自适应抓拍间隔：有人（该设备 `APP_VISIT_END_SECS` 内保留过有人图）60s / 白天无人 120s / 夜间无人 900s；**夜间不停拍**，时间窗仅作昼夜分界
- 双重过滤后图才保留入库：① pHash 相似度（基准=该设备上一张**保留**图，被丢弃的图不更新基准，防渐进漂移）≥ `APP_SIMILARITY_THRESHOLD` 则丢弃；② YOLOv8n 检测无人则丢弃（检测失败放行，宁多推不漏报）
- `message_id` 列为 varchar(31)，msg_id 用去连字符 UUID 截 31 位

### 3. 到访批次推送（poller 通道推送策略）
- 一次"到访" = 首张保留有人图 → 超过 `APP_VISIT_END_SECS` (600s) 无新保留图
- 到访开始发一条"⚠️ 有人出现: 设备名"（须检测确认有人）；到访持续超 `APP_VISIT_REMIND_SECS` (1800s) 每隔该时长补报一条
- 到访结束：若缓冲中有提醒之外的新图则发一条"N张图片抓拍"图集（上限 `APP_MESSAGE_SIZE`），否则静默清空
- webhook 通道仍沿用原批量规则（min/max/timeout）

### 3.1 躺卧姿态即时提醒（独立通道）
- 检出躺卧姿态的图**立即**推送"⚠️ 请关注: 设备名 有人躺卧"+ 该图，**不受到访批次节奏约束**（及时性优先）
- 文案使用中性词"躺卧"，不出现危险字眼
- 同设备冷却期 `APP_LYING_ALERT_COOLDOWN_SECS`（默认 1800s）内不重复；`APP_LYING_ALERT_ENABLED=off` 可整体关闭

### 3.2 日间长时间无人提醒
- 白天（时间窗内）连续 `APP_INACTIVITY_ALERT_SECS`（默认 7200s）无确认有人 → 推送"⏰ 提醒: 日间已连续X小时未发现人员出现"；持续无人每隔该时长重提
- 计时基线 = max(最近一次确认有人, 当日时间窗起点)：任一设备确认有人即重置（跨设备并集）；夜间不计时不提醒；服务重启从启动时刻起算
- 依赖人员检测开启（检测关闭无法确认"有人"，此时本提醒不生效）；开关 `APP_INACTIVITY_ALERT_ENABLED`

### 4. 人员检测
- YOLOv8n-pose（官方 ultralytics 导出约 13MB，不进 git，下载地址见 README 3.1；第三方转存实测有损坏）。带 17 个人体关键点头，同时兼容旧纯检测版 yolov8n.onnx（自动识别通道数）
- 预处理：letterbox 640×640 居中灰底 114，RGB /255，**NCHW 逐平面填充**（按像素交错填充会导致输入错乱、检测失效——历史踩坑）
- 输出 [1,56,8400]（pose：4 框 + 1 person 分数 + 17×3 关键点；person 分数固定在第 4 行）；纯检测版为 [1,84,8400]（dims[1]<dims[2] 时类别维在前）。无特征头，去重自动回退纯 pHash
- 配置: `APP_PERSON_ALERT_ENABLED` (默认 true)、`APP_PERSON_CONFIDENCE_THRESHOLD` (默认 0.5)、`APP_MODEL_PATH` (默认 models/yolov8n-pose.onnx)

### 4.1 躺卧姿态判定（detector 内部，四级 + 兜底）
- 候选框先过 **NMS 去重**（IoU>0.5 抑制；YOLO 对同一人输出大量重叠锚框——实测 1 人 9 框——各带独立关键点回归，是误报放大器），只对去重后每个目标的最高分框做姿态判定
- 四级判定（全部通过才判躺卧，坐标取双侧关键点中点）：
  1. **躯干水平**：肩中点(5/6)→髋中点(11/12) 横向 > 纵向 × 1.5（`TORSO_RATIO`；蹲/坐前倾投影可达 35°，实测 33/23）
  2. **大腿水平**：髋→膝中点(13/14) 横向 > 纵向 × 1.7（`LEG_RATIO`；排除弯腰/弓步——躯干水平但腿竖直/斜向，实测夹角 36°~68°）
  3. **小腿水平**：膝→踝中点(15/16) 横向 > 纵向 × 1.5（`ANKLE_RATIO`；蹲/坐者小腿竖直——踝在膝下方）
  4. **全身共线**：肩→踝"体轴"横向 > 纵向 × 1.5（`AXIS_RATIO`），且躯干/大腿/小腿三段与体轴夹角均 < ~24°（`COLINEAR_SIN` 0.40）。躺卧 = 肩髋膝踝一条直线；坐地伸腿从高处俯拍呈低-高-低 Z 字折线（实测膝高于髋和踝，逐段比值全部通过），仅共线判定可识别（2026-11-18 坐姿抱盆误报后引入）
- **尺度守卫**：躯干/大腿/小腿两点距离 < 30/15/12px（640 输入坐标系）说明目标太小太远，关键点比值被噪声主导，退回框判定
- 关键点置信度 ≥ 0.3 才参与；踝不可用或尺度不足时回退**人形框宽 > 高 × 1.8**（`LYING_BOX_RATIO`）兜底
- 判定为躺卧时打印判定依据日志（各段横纵分量 / 体轴）
- 已知几何极限：身体朝向与摄像头视线接近平行的躺卧在 2D 投影上与站姿不可区分，会漏报（宁少报不误报）；误报/漏报还与光照、遮挡、拍摄角度相关，阈值（detector.rs 常量）可按实际场景调；test_assets/ 下有真实误报图作回归测试（bent_over*.jpg、sitting_basin.jpg）

### 5. 图片去重
- **pHash 感知哈希**: DoubleGradient 8×8，快速相似图片检测
- 配置: `APP_P_HASH_THRESHOLD` (默认 5)、`APP_AI_SIMILARITY_THRESHOLD` (默认 0.95)

## 项目结构

```
src/
├── main.rs          # 入口，Web 服务器启动 + poller 线程(panic 守护，异常 10s 重启)
├── logging.rs       # 带时间戳日志宏 log_info!/log_error!（全项目打印统一走这里）
├── routers.rs       # 路由配置
├── handlers.rs      # webhook 请求处理（去重/检测/推送）
├── poller.rs        # 轮询抓拍模式（token 刷新/设备调度/自适应间隔/到访批次推送）
├── db.rs            # 数据库操作
├── util.rs          # 工具函数（PushPlus 推送、图片落盘）
├── models/          # 数据模型
│   ├── models.rs    # AppState, Message, Body 等
│   ├── chan_msg.rs  # 消息通道（推送、图片落盘）
│   ├── warn.rs      # 告警消息
│   └── ...
├── image_dedup/     # 图片去重模块
│   ├── dedup.rs     # 去重缓存
│   ├── phash.rs     # pHash 计算（内存数据）
│   └── mod.rs
└── person_detector/ # 人员检测模块
    ├── detector.rs  # YOLOv8n ONNX 推理
    └── mod.rs
```

## 环境变量

| 变量 | 说明 | 默认值 |
|------|------|--------|
| APP_SERVER_ADDR | 服务地址 | 0.0.0.0:8000 |
| DATABASE_URL | 数据库连接串 | (必填) |
| APP_API_TOKEN / APP_API_TOPIC | PushPlus 密钥 / 群组编码 | (必填) |
| APP_IMG_SERVER | 图片 URL 前缀 | (必填) |
| APP_API_TIMEOUT | webhook 批量推送超时(秒) | 600 |
| APP_START_TIME / APP_END_TIME | 工作时间窗 / 昼夜分界 | 7:00:00 / 23:00:00 |
| APP_MIN_PIC_COUNT | webhook 单条推送最少图片数 | 10 |
| APP_MESSAGE_SIZE | 单条推送最多图片数 | 50 |
| APP_P_HASH_THRESHOLD | pHash 汉明距离阈值 | 5 |
| APP_AI_SIMILARITY_THRESHOLD | AI 相似度阈值 | 0.95 |
| APP_PERSON_ALERT_ENABLED | 本地人员检测开关 | true |
| APP_PERSON_CONFIDENCE_THRESHOLD | 人员检测置信度阈值 | 0.5 |
| APP_MODEL_PATH | ONNX 模型路径 | models/yolov8n-pose.onnx |
| APP_LYING_ALERT_ENABLED | 躺卧姿态即时提醒开关 | on |
| APP_LYING_ALERT_COOLDOWN_SECS | 同设备躺卧提醒冷却期(秒) | 1800 |
| APP_INACTIVITY_ALERT_ENABLED | 日间长时间无人提醒开关 | on |
| APP_INACTIVITY_ALERT_SECS | 日间连续无人多久提醒(秒) | 7200 |
| APP_ALERT_COOLDOWN_SECS | webhook 即时提醒冷却期(秒) | 600 |
| APP_POLLER | 轮询抓拍模式开关 | off |
| EZVIZ_APP_KEY / EZVIZ_APP_SECRET | 萤石开放平台凭证 (poller 必填) | (必填) |
| APP_CAPTURE_INTERVAL_SECS | 白天无人抓拍间隔(秒) | 120 |
| APP_ACTIVE_CAPTURE_INTERVAL_SECS | 有人期间抓拍间隔(秒) | 60 |
| APP_NIGHT_CAPTURE_INTERVAL_SECS | 夜间无人抓拍间隔(秒) | 900 |
| APP_VISIT_END_SECS | 无新图判定"到访结束"(秒) | 600 |
| APP_VISIT_REMIND_SECS | 长到访补报提醒间隔(秒) | 1800 |
| APP_TOKEN_REFRESH_HOURS | accessToken 刷新周期(小时) | 120 |
| APP_SIMILARITY_THRESHOLD | poller 与基准图相似度阈值 | 0.95 |

## 部署要点

- docker-compose: nginx (443/80) + mariadb + video_message (scratch 镜像，二进制只读挂载，models/ 与截图目录挂载)；video_message 需设 `TZ=Asia/Shanghai`（镜像含 zoneinfo），否则日志时间戳为 UTC
- 真实密钥只放服务器 compose / 本地 `.env`（均已 gitignore）；仓库 compose 用占位符
- 抓拍接口按次计费（日 1 万次免费，同设备间隔 ≥4s）；当前最坏用量 ~2000 次/天

## 待开发/优化方向

> 可在此记录未来提案方向

- RTSP 子码流本地抽帧检测（摆脱萤石云抓拍计费与 4s 限制）
- 套餐到期后换新 appKey 恢复云推送通道（每应用独立 120 天试用）
- 图集消息带时间轴/设备分组，长到访摘要更易读
