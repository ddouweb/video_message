#!/bin/bash

GITHUB_USER="ddouweb"
REPO="video_message"
ARTIFACT_NAME="videoMsg"

# 创建临时目录用于下载和安装
TEMP_DIR=$(mktemp -d)
cd $TEMP_DIR

echo "Fetching latest release..."
curl -sL -o "$ARTIFACT_NAME" "https://github.com/$GITHUB_USER/$REPO/releases/download/latest/$ARTIFACT_NAME"
chmod +x "$ARTIFACT_NAME"

# 移动二进制文件到 /usr/local/bin 或其他安装目录
echo "Installing the binary..."
sudo mv "$ARTIFACT_NAME" /usr/local/bin/

# 清理临时目录
cd -
rm -rf $TEMP_DIR

# 创建 Docker Compose 配置文件
echo "Setting up Docker Compose..."
cat <<EOF > docker-compose.yml
version: '3.8'
services:
  app:
    image: ubuntu:latest  # 替换为你需要的基础镜像或其他依赖镜像
    container_name: video_app
    volumes:
      - /usr/local/bin/$ARTIFACT_NAME:/usr/local/bin/$ARTIFACT_NAME
    command: ["/usr/local/bin/$ARTIFACT_NAME"]
EOF

# 启动 Docker Compose
echo "Starting Docker Compose..."
docker-compose up -d

echo "Installation complete. The application is running."
