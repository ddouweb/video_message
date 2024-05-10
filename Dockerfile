# 设置基础镜像为 Rust 官方提供的 Rust 编译环境
FROM rust:latest as builder
# 创建一个工作目录
WORKDIR /app
# 复制整个项目到容器中
COPY . .
# 编译应用程序
RUN cargo build --release
# 创建一个新的镜像来运行应用程序
FROM debian:buster-slim

# 安装必要的运行时库
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# 从构建阶段复制构建的二进制文件到容器中
COPY --from=builder /app/target/release/hellorust /app/video_message

# 设置容器的工作目录
WORKDIR /app

# 指定容器的入口命令
CMD ["./video_message"]