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

### 2.2 管理服务

#### 启动服务

进入安装目录后，运行以下命令启动服务：

```bash
docker compose up
```

#### 停止服务/卸载服务

进入安装目录后，运行以下命令启动服务：

```bash
docker compose down
```

----
# 致谢
[pushPlus](https://www.pushplus.plus/)