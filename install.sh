#!/bin/bash
# 介绍信息
echo -e "\e[32m
1.您至少已经准备好了push_plus的token:
2.建议您设置好push_plus要接收消息的群组编码为:video
3.建议您配置你的nginx为https访问。（需要您提供给域名证书和私钥）
\e[0m"

GITHUB_USER="ddouweb"
REPO="video_message"
ARTIFACT_NAME="videoMsg"
DEFAULT_INSTALL_DIR=$(pwd) #/workspaces/video_root
MY_IP=$(curl -s http://ifconfig.me)
read -p "请输入安装目录 [默认: $DEFAULT_INSTALL_DIR]: " INSTALL_DIR
INSTALL_DIR=${INSTALL_DIR:-$DEFAULT_INSTALL_DIR}
echo "安装目录: $INSTALL_DIR"

if [ -e "$INSTALL_DIR/docker-compose.yml" ]; then
  echo "尝试先停止目标服务"
  docker compose -f "$INSTALL_DIR/docker-compose.yml" down
fi

# 检查是否有权限创建目录
if [ ! -d "$INSTALL_DIR" ]; then
    echo "安装目录不存在，正在创建..."
    mkdir -p "$INSTALL_DIR" || { echo "无法创建目录，请检查权限！"; exit 1; }
else
    echo "目录已存在: $INSTALL_DIR"
fi

read -p "请输入接收消息端口号 [默认: 80]: " MSG_PORT
MSG_PORT=${MSG_PORT:-80}
echo "接收消息端口号: $MSG_PORT"

read -p "请输入访问图片资源端口号 [默认: 443]（如果不使用ssl,则公用消息接收端口号）: " IMG_PORT
IMG_PORT=${IMG_PORT:-443}
echo "访问图片资源端口号: $IMG_PORT"

IMG_URL="http://${MY_IP}:${MSG_PORT}/img" 
HOOK_URL="http://${MY_IP}:${MSG_PORT}/video_message/webhook"

while true; do
    read -p "请输入 pushPlus 的 token: " PUSH_TOKEN
    if [[ -z "$PUSH_TOKEN" ]]; then
        echo "错误: token 不能为空，请重新输入！"
    else
        break
    fi
done
echo "pushPlus token: $PUSH_TOKEN"

read -p "请输入pushPlus的群组编码 [默认: video]: " PUSH_CODE
PUSH_CODE=${PUSH_CODE:-video}
echo "pushPlus的群组编码: $PUSH_CODE"

echo "下载安装所需的文件："

# 下载二进制文件
if ! curl -sL -f --retry 3 --retry-delay 5 -o "$INSTALL_DIR/$ARTIFACT_NAME" "https://github.com/$GITHUB_USER/$REPO/releases/download/latest/$ARTIFACT_NAME"; then
    echo "错误: 无法下载 $ARTIFACT_NAME。请检查 URL 和网络连接。"
    exit 1
fi

# 下载 MariaDB 初始化文件
if ! curl -sL -f --retry 3 --retry-delay 5 -o "$INSTALL_DIR/mariadb-init.sql" "https://raw.githubusercontent.com/$GITHUB_USER/$REPO/master/mariadb-init.sql"; then
    echo "错误: 无法下载 MariaDB 初始化文件。请检查 URL 和网络连接。"
    exit 1
fi

# 下载 nginx 配置文件
if ! curl -sL -f --retry 3 --retry-delay 5 -o "$INSTALL_DIR/nginx-default.conf" "https://raw.githubusercontent.com/$GITHUB_USER/$REPO/master/nginx.conf"; then
    echo "错误: 无法下载 MariaDB 初始化文件。请检查 URL 和网络连接。"
    exit 1
fi

# 为下载的二进制文件添加可执行权限
chmod +x "$INSTALL_DIR/$ARTIFACT_NAME"


# 是否配置 SSL
read -p "是否需要使用更安全的https？(y/n) [默认: n]: " CONFIGURE_SSL
CONFIGURE_SSL=${CONFIGURE_SSL:-n}

# 生成 Nginx 配置
if [[ "$CONFIGURE_SSL" =~ ^[Yy]$ ]]; then

    read -p "请输入服务器域名: " MY_DOMAIN
    IMG_URL="https://${MY_DOMAIN}:${IMG_PORT}/img"
    HOOK_URL="https://${MY_DOMAIN}:${IMG_PORT}/video_message/webhook"

    read -p "请输入 SSL 证书文件的完整路径: " CERT_PATH
    while [[ ! -f "$CERT_PATH" ]]; do
        echo "证书文件不存在，请重新输入。"
        read -p "请输入 SSL 证书文件的完整路径: " CERT_PATH
    done
    echo "证书路径: $CERT_PATH"

    read -p "请输入 SSL 私钥文件的完整路径: " KEY_PATH
    while [[ ! -f "$KEY_PATH" ]]; do
        echo "私钥文件不存在，请重新输入。"
        read -p "请输入 SSL 私钥文件的完整路径: " KEY_PATH
    done
    echo "私钥路径: $KEY_PATH"

    # 更新 Nginx 配置文件为 SSL 版本
    SSL_CONFIG="
server {
    listen 80;
    listen 443 ssl http2;
    server_name _;
    ssl_certificate $CERT_PATH;
    ssl_certificate_key $KEY_PATH;
    root /usr/share/nginx/html;
    location ^~/img/ {
        try_files \$uri \$uri/ =404;
    }
    location ^~/video_message {
        proxy_redirect off;
        proxy_connect_timeout 3000s;
        proxy_send_timeout 3000s;
        proxy_read_timeout 3000s;
        proxy_pass http://video_message:8000;
    }
}
"
echo "重新生成nginx ssl 配置..."
echo "$DEFAULT_CONFIG" > "$INSTALL_DIR/nginx-default.conf"
echo "Nginx 配置已保存到 $INSTALL_DIR/nginx-default.conf"
fi

echo "创建 Docker Compose 配置文件..."
cat <<EOF > $INSTALL_DIR/docker-compose.yml
services:
  nginx:
    image: nginx:1.20.2-alpine
    container_name: nginx
    volumes:
      - $INSTALL_DIR/nginx-default.conf:/etc/nginx/conf.d/default.conf  # 挂载 Nginx 配置文件
    ports:
      - "$IMG_PORT:443"
      - "$MSG_PORT:80"
    
  redis:
    image: redis:7.4.1-alpine
    container_name: redis
    environment:
      REDIS_PASSWORD: 123456
    command: redis-server --requirepass 123456
    volumes:
      - $INSTALL_DIR/docker-data/redis:/data

  mariadb:
    image: mariadb:10.6.14
    container_name: mariadb
    environment:
      MYSQL_ROOT_PASSWORD: 123456
      TZ: Asia/Shanghai
    command: --lower-case-table-names=1
    volumes:
      - $INSTALL_DIR/mariadb-init.sql:/docker-entrypoint-initdb.d/init.sql  # 挂载初始化 SQL 文件
      - $INSTALL_DIR/docker-data/mariadb:/var/lib/mysql

  video_message:
    image: webtao/scratch:zoneinfo
    container_name: video_message
    environment:
      DATABASE_URL: mysql://video:123456@mariadb:3306/video?timezone=Asia/Shanghai
      APP_API_TOKEN: $PUSH_TOKEN
      APP_API_TOPIC: $PUSH_CODE
      APP_IMG_SERVER: $IMG_URL
      APP_API_TIMEOUT: 600
      APP_MESSAGE_SIZE: 50
    volumes:
      - $INSTALL_DIR/$ARTIFACT_NAME:/app:ro #应用可执行文件
      - $INSTALL_DIR/his_path:/data/his_path:rw  #存放历史截图
      - $INSTALL_DIR/last_path:/data/last_path:rw #存放最新的截图
EOF

echo "启动 Docker Compose..."
docker compose -f $INSTALL_DIR/docker-compose.yml up -d
echo "Installation complete. The application is running."

echo "请配置萤石云/云信令/消息推送的webhook回调地址为: $HOOK_URL"