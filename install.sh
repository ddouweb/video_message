#!/usr/bin/env bash
# 介绍信息
echo -e "\e[32m
1.您至少已经准备好了push_plus的token:
2.建议您设置好push_plus要接收消息的群组编码为:video
3.建议您配置你的nginx为https访问。（需要您提供给域名证书和私钥）
\e[0m"

command -v curl > /dev/null || { echo "错误: 请安装 curl 工具"; exit 1; }
command -v docker > /dev/null || { echo "错误: 请安装 Docker 工具"; exit 1; }
docker info > /dev/null 2>&1 || { echo "错误: Docker 服务未启动"; exit 1; }

# 下载文件的函数，带进度条和速度显示
download_file() {
    local url=$1
    local output=$2
    # 检查目标文件是否已存在
    if [ -f "$output" ]; then
        read -p "文件 $output 已存在,是否覆盖？(y/n)[默认: n]: " choice
        choice=${choice:-n}
        case "$choice" in
            y|Y )
                echo "覆盖文件 $output..."
                ;;
            * )
                echo "保留现有文件 $output，跳过下载。"
                return
                ;;
        esac
    fi
    for attempt in {1..3}; do
        echo "正在下载 $url (尝试 $attempt/3)..."
        curl -# --fail --progress-bar -L -o "$output" "$url" && return
        sleep 2
    done
    echo "错误: 无法下载文件 $url"
    [ -f "$output" ] && rm "$output"
    exit 1
}

GITHUB_USER="ddouweb"
REPO="video_message"
ARTIFACT_NAME="videoMsg"
DEFAULT_INSTALL_DIR=$(pwd)
MY_IP=$(curl -s http://ifconfig.me)
# 拷贝证书和私钥文件到目标目录，并生成新的路径
CERT_BASENAME="cert.pem" #nginx cert
KEY_BASENAME="key.pem" #nginx key
read -p "请输入安装目录 [默认: $DEFAULT_INSTALL_DIR]: " INSTALL_DIR
INSTALL_DIR=${INSTALL_DIR:-$DEFAULT_INSTALL_DIR}
echo "安装目录: $INSTALL_DIR"
echo

CFG_DIR="$INSTALL_DIR/cfg"

if [ -e "$INSTALL_DIR/docker-compose.yml" ]; then
  echo "尝试先停止目标服务"
  docker compose -f "$INSTALL_DIR/docker-compose.yml" down
fi
echo

# 检查是否有权限创建目录
if [ ! -d "$INSTALL_DIR" ]; then
    echo "安装目录不存在，正在创建..."
    mkdir -p "$INSTALL_DIR" || { echo "无法创建目录，请检查权限！"; exit 1; }
else
    echo "目录已存在: $INSTALL_DIR"
fi

if [ ! -d "$CFG_DIR" ]; then
    echo "配置目录不存在，正在创建..."
    mkdir -p "$CFG_DIR" || { echo "无法创建目录，请检查权限！"; exit 1; }
else
    echo "配置目录已存在: $CFG_DIR"
fi
echo


# 是否配置 SSL
read -p "是否使用更安全的https？(y/n) [默认: n]: " CONFIGURE_SSL
CONFIGURE_SSL=${CONFIGURE_SSL:-n}
MY_DOMAIN=${MY_DOMAIN:-${MY_IP}}

read -p "请输入对外暴露端口号 [默认: 8000]: " BASE_PORT
BASE_PORT=${BASE_PORT:-8000}
IMG_URL="http://${MY_IP}:${BASE_PORT}/img" 
HOOK_URL="http://${MY_IP}:${BASE_PORT}/video_message/webhook"

if [[ "$CONFIGURE_SSL" =~ ^[Yy]$ ]]; then
  read -p "请输入服务器域名[默认:localhost]: " MY_DOMAIN
  MY_DOMAIN=${MY_DOMAIN:-localhost}
  IMG_URL="https://${MY_DOMAIN}:${BASE_PORT}/img"
  HOOK_URL="https://${MY_DOMAIN}:${BASE_PORT}/video_message/webhook"
fi
echo

# 生成 Nginx 配置
if [[ "$CONFIGURE_SSL" =~ ^[Yy]$ ]]; then
    read -p "请输入 SSL 证书文件的路径 (.cert 或 .pem 或 .crt): " CERT_PATH
    while [[ ! -f "$CERT_PATH" ]]; do
        read -p "证书文件不存在，请重新输入 SSL 证书文件的路径 (.cert 或 .pem 或 .crt): " CERT_PATH
    done
    echo "证书路径: $CERT_PATH"

    if [[ "$CERT_PATH" == *.crt ]]; then
      NEW_CERT_PATH="${CFG_DIR}/cert.pem"
      cp "$CERT_PATH" "$NEW_CERT_PATH"
      CERT_BASENAME="cert.pem"
      echo "转换 .crt 文件为 .pem 格式: $NEW_CERT_PATH"
    fi


    read -p "请输入 SSL 私钥文件的路径 (.key 或者 .pem): " KEY_PATH
    while [[ ! -f "$KEY_PATH" ]]; do
        read -p "私钥文件不存在，请重新输入 SSL 私钥文件的路径 (.key 或者 .pem): " KEY_PATH
    done
    echo "私钥路径: $KEY_PATH"

    CERT_BASENAME=$(basename "$CERT_PATH")
    KEY_BASENAME=$(basename "$KEY_PATH")

    cp "$CERT_PATH" "$CFG_DIR/$CERT_BASENAME" || { echo "拷贝证书失败！"; exit 1; }
    cp "$KEY_PATH" "$CFG_DIR/$KEY_BASENAME" || { echo "拷贝私钥失败！"; exit 1; }
fi
echo

while true; do
    read -p "请输入 pushPlus 的 token: " PUSH_TOKEN
    if [[ -z "$PUSH_TOKEN" ]]; then
        echo "错误: token 不能为空，请重新输入！"
    else
        break
    fi
done
read -p "请输入pushPlus的群组编码 [默认: video]: " PUSH_CODE
PUSH_CODE=${PUSH_CODE:-video}
echo

echo "即将下载安装所需的文件..."
# 下载二进制文件
download_file "https://github.com/$GITHUB_USER/$REPO/releases/download/latest/$ARTIFACT_NAME" $INSTALL_DIR/$ARTIFACT_NAME
download_file "https://raw.githubusercontent.com/$GITHUB_USER/$REPO/master/mariadb-init.sql" "$CFG_DIR/mariadb-init.sql"
#download_file "https://raw.githubusercontent.com/$GITHUB_USER/$REPO/master/nginx.conf" "$CFG_DIR/nginx-default.conf"
echo
# 为下载的二进制文件添加可执行权限
chmod +x "$INSTALL_DIR/$ARTIFACT_NAME"

# 更新 Nginx 配置文件为 SSL 版本
SSL_CONFIG="
server {
    listen $([[ "$CONFIGURE_SSL" =~ ^[Yy]$ ]] && echo "443 ssl http2" || echo "80");
    server_name $MY_DOMAIN;
    $([[ "$CONFIGURE_SSL" =~ ^[Yy]$ ]] && echo "ssl_certificate /etc/nginx/ssl/$CERT_BASENAME;")
    $([[ "$CONFIGURE_SSL" =~ ^[Yy]$ ]] && echo "ssl_certificate_key /etc/nginx/ssl/$KEY_BASENAME;")
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
echo "$SSL_CONFIG" > "$CFG_DIR/nginx-default.conf"
echo "Nginx 配置文件已保存到 $CFG_DIR/nginx-default.conf"
chmod 600 "$CFG_DIR/nginx-default.conf"
echo

echo "创建 Docker Compose 配置文件..."
cat <<EOF > $INSTALL_DIR/docker-compose.yml
services:
  mariadb:
    image: mariadb:10.6.14
    container_name: mariadb
    environment:
      MYSQL_ROOT_PASSWORD: 123456
      TZ: Asia/Shanghai
    command: --lower-case-table-names=1
    volumes:
      - $CFG_DIR/mariadb-init.sql:/docker-entrypoint-initdb.d/init.sql  # 挂载初始化 SQL 文件
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
  nginx:
    image: nginx:1.20.2-alpine
    container_name: nginx
    ports:
      - "${BASE_PORT}:$([[ "$CONFIGURE_SSL" =~ ^[Yy]$ ]] && echo 443 || echo 80)"
    volumes:
      - $CFG_DIR/nginx-default.conf:/etc/nginx/conf.d/default.conf:rs  # 挂载 Nginx 配置文件
      - $INSTALL_DIR/last_path:/usr/share/nginx/html/img:ro #通过nginx访问图片资源    
EOF

if [[ "$CONFIGURE_SSL" =~ ^[Yy]$ ]]; then
  echo "      - $CFG_DIR/$CERT_BASENAME:/etc/nginx/ssl/$CERT_BASENAME:ro" >> $INSTALL_DIR/docker-compose.yml
  echo "      - $CFG_DIR/$KEY_BASENAME:/etc/nginx/ssl/$KEY_BASENAME:ro" >> $INSTALL_DIR/docker-compose.yml
fi
echo "Docker Compose 文件已保存到 $INSTALL_DIR/docker-compose.yml"
echo "启动 Docker Compose..."
docker compose -f $INSTALL_DIR/docker-compose.yml up -d
echo "检查服务是否启动..."
sleep 5

echo "验证 webhook 地址: $HOOK_URL"
HTTP_STATUS=$(curl -o /dev/null -s -w "%{http_code}" --request POST --header 'Content-Type: application/json' -d '{}' --connect-timeout 8 --max-time 8  "$HOOK_URL")
if [ "$HTTP_STATUS" -ne 200 ]; then
    echo "错误: 无法访问 $HOOK_URL，HTTP 状态码: $HTTP_STATUS,请检查服务启动状态和防火墙"
    exit 1
else
    echo "服务运行正常，webhook 地址可访问。"
fi

docker compose -f "$INSTALL_DIR/docker-compose.yml" logs --tail 200 video_message
echo "请配置萤石云/云信令/消息推送的webhook回调地址为: $HOOK_URL"
echo "如需卸载，请执行：docker compose -f "$INSTALL_DIR/docker-compose.yml" down"
