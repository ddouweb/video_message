# Video Message 安装与使用手册

**Video Message** 是一款用于接收萤石云消息并将其发送到微信的工具。本手册将指导你如何在自己的机器上安装和使用它。

---

## 1. 准备工作

在安装 **Video Message** 之前，请确保以下条件已满足：

1. **Docker 环境**  
   确保机器上已安装 Docker 客户端，并能够正常拉取镜像。

2. **PushPlus 配置**  
   准备好以下信息：
    - PushPlus 用户 Token
    - 群组编码

3. **HTTPS 支持（推荐）**  
   建议使用 HTTPS 访问服务：
    - 准备好域名解析。
    - 提供 Nginx 所需的 SSL 证书。

---

## 2. 安装与使用

### 2.1 一键安装

运行以下命令，根据提示输入必要信息，即可完成安装：

```bash
bash <(curl -s https://raw.githubusercontent.com/ddouweb/video_message/simple/install.sh)
```

### 2.2 一键安装(如果你需要记录你的消息事件)

运行以下命令，根据提示输入必要信息，即可完成安装：

```bash
bash <(curl -s https://raw.githubusercontent.com/ddouweb/video_message/master/install.sh)
```

### 2.3 管理服务

#### 启动服务

进入安装目录后，运行以下命令：

```bash
docker compose up
```

#### 停止服务/卸载服务

进入安装目录后，运行以下命令：

```bash
docker compose down
```

---

## 3. 萤石轮询抓拍模式

当萤石云推送不可用时（如个人版套餐到期后不再下发消息），可启用轮询抓拍模式：服务自己定时获取 accessToken，对在线设备手动抓拍，图片与该设备上一张对比，相似度过高则丢弃，剩余图片按原有规则推送。

### 3.1 配置

在 `docker-compose.yaml` 的 `video_message` 服务环境变量中添加：

| 环境变量 | 说明 | 默认值 |
| --- | --- | --- |
| `APP_POLLER` | 轮询抓拍模式开关，`on`/`off` | off |
| `EZVIZ_APP_KEY` | 萤石开放平台 appKey（必填，开启时） | 无 |
| `EZVIZ_APP_SECRET` | 萤石开放平台 appSecret（必填，开启时） | 无 |
| `APP_CAPTURE_INTERVAL_SECS` | 白天无人时抓拍间隔（秒） | 120 |
| `APP_ACTIVE_CAPTURE_INTERVAL_SECS` | 有人期间（最近保留过有人图）抓拍间隔（秒） | 60 |
| `APP_NIGHT_CAPTURE_INTERVAL_SECS` | 夜间无人时抓拍间隔（秒） | 900 |
| `APP_VISIT_END_SECS` | 无新保留图超过该秒数判定"到访结束"，触发图集收尾（秒） | 600 |
| `APP_VISIT_REMIND_SECS` | 到访期间长时间有人，每隔该秒数补一条即时提醒（秒） | 1800 |
| `APP_TOKEN_REFRESH_HOURS` | accessToken 刷新周期（小时），token 有效期 7 天 | 120 |
| `APP_SIMILARITY_THRESHOLD` | 与上一张图的相似度阈值（0.0~1.0），高于则丢弃（低于的都保留） | 0.95 |
| `APP_PERSON_ALERT_ENABLED` | 本地人员检测开关（YOLOv8n-pose），开启后轮询抓拍只保留有人的图 | true |
| `APP_MODEL_PATH` | 人员检测 ONNX 模型路径 | models/yolov8n-pose.onnx |
| `APP_PERSON_CONFIDENCE_THRESHOLD` | 人员检测置信度阈值 | 0.5 |
| `APP_LYING_ALERT_ENABLED` | 躺卧姿态即时提醒开关（需 pose 模型） | on |
| `APP_LYING_ALERT_COOLDOWN_SECS` | 同设备躺卧提醒冷却期（秒），期内不重复提醒 | 1800 |

复用的原有配置：`APP_START_TIME`/`APP_END_TIME`（白天/夜间分界，默认 7:00:00~23:00:00，夜间降频但不停止）、`APP_MESSAGE_SIZE`（单条图集最多图片数）、`APP_IMG_SERVER`/`APP_IMG_PATH_LAST`/`APP_IMG_PATH_HIS`（图片存储）。

模型文件不进 git，首次部署需下载（ultralytics 官方导出，约 13MB；自带 17 个人体关键点头，可判断躺卧姿态）：

```bash
curl -L -o models/yolov8n-pose.onnx https://github.com/ultralytics/assets/releases/download/v8.4.0/yolov8n-pose.onnx
```

> 注意：请勿使用 hf-mirror 等第三方转存的模型，实测存在导出损坏（输出全为噪音）。旧的纯检测版 `yolov8n.onnx` 也兼容（只检人、无姿态判断）。

### 3.2 行为说明

- 只抓 `status == 1`（在线）的设备；抓拍按**设备各自到点调度**，不全局同步；
- 抓拍间隔自适应：某设备最近 `APP_VISIT_END_SECS` 秒内保留过有人图 → `APP_ACTIVE_CAPTURE_INTERVAL_SECS`；夜间（时间窗外）无人 → `APP_NIGHT_CAPTURE_INTERVAL_SECS`；其余 → `APP_CAPTURE_INTERVAL_SECS`。**夜间不停止抓拍**，检出有人同样即时提醒；
- 相似度对比基准是**每个设备上一张被保留（推送）的图**；被丢弃的图不更新基准，场景的渐进变化会累积、最终超过阈值被推出去；
- 本地人员检测开启时，抓拍图先过 pHash 去重、再过 YOLOv8n 人员过滤，**无人的图直接丢弃**；有人的图按"即时提醒 + 到访图集"推送（见 3.3）；
- 手动抓拍接口为按次计费（免费额度 10000 次/天），抓拍间隔不宜过短（同一设备须 ≥4 秒）。

### 3.3 有人出现推送策略（到访批次：即时提醒 + 到访图集）

一次"到访"= 从第一张保留的有人图开始，到 `APP_VISIT_END_SECS` 秒内再无新的保留图结束（人走了/画面静止）。到访期间该设备保持高频抓拍。微信通知每个到访最多两条：

1. **即时提醒**：到访开始时推送一条"⚠️ 有人出现: 设备名"+ 首图；到访持续超过 `APP_VISIT_REMIND_SECS` 秒时每隔该时长补一条（人还在）。检测失败放行的图不做即时提醒（不能断言有人），只进图集；
2. **到访图集**：到访结束时把这一批图汇总成一条"N张图片抓拍"（`N < APP_MESSAGE_SIZE`）；若这批图都已随即时提醒发过、没有新图，则跳过，避免同图发两遍。

即短到访 = 提醒 1 条 + 图集 1 条；检测关闭时无即时提醒，全部图走图集。

### 3.4 躺卧姿态即时提醒

使用 pose 模型（56 通道输出，17 关键点）时，检测到躺卧姿态的人会**立即**推送一条"⚠️ 请关注: 设备名 有人躺卧"，不受到访批次节奏约束。同设备冷却期 `APP_LYING_ALERT_COOLDOWN_SECS`（默认 1800 秒）内不重复提醒。文案使用中性表述，不出现危险字眼。

躺卧判定规则（两级 + 兜底，防弯腰劳作误报）：

1. **躯干接近水平**：肩中点→髋中点连线的横向分量 > 纵向分量 × 1.2；
2. **腿也接近水平**：髋中点→膝中点连线横向 > 纵向 × 1.7（躺卧者全身横卧；弯腰/弓步劳作时躯干同样水平，但腿是竖直或斜向的，据此排除）；
3. **尺度守卫**：躯干/腿两点距离过小（远处小目标，<30px/<15px）时关键点噪声占主导，不采信比值，退回框判定；
4. **框兜底**：关键点不可用或尺度不足时，人形框宽 > 高 × 1.8 才判躺卧。

适用场景：独居老人长时间躺在地上/沙发上无人扶助等异常姿态的及时告知。误报率与光照、遮挡、拍摄角度相关，若提醒过多可调大冷却期或 `APP_LYING_ALERT_ENABLED=off` 关闭。

注意：萤石云消息回调（WarnBody 通道）若仍开启，附图默认是 `hikencod` 加密格式（需在萤石 App 关闭图片加密才能明文展示），加密图不会入库推送；该通道的即时提醒冷却期由 `APP_ALERT_COOLDOWN_SECS` 控制（默认 600）。

----
# 致谢
[pushPlus](https://www.pushplus.plus/)
