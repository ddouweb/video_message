services:
  redis:
    image: redis
    container_name: redis
    environment:
      REDIS_PASSWORD: 123456
    command: redis-server --requirepass 123456
    ports:
      - "6379:6379"
    volumes:
      - /data/docker-data/redis:/data

  mariadb:
    image: mariadb:10.6.14
    container_name: mariadb
    environment:
      MYSQL_ROOT_PASSWORD: 123456
      TZ: Asia/Shanghai
    #  LOWER_CASE_TABLE_NAMES: 1
    ports:
      - "3306:3306"
    command: --lower-case-table-names=1
    volumes:
      - /data/docker-data/mariadb:/var/lib/mysql

  video_message:
    image: webtao/scratch:zoneinfo
    container_name: video_message
    environment:
      DATABASE_URL: mysql://video:123456@mariadb:3306/video?timezone=Asia/Shanghai
      APP_API_TOKEN: xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
      APP_IMG_SERVER: https://img.xxxxxxxxxxxxxxx.cn/img
      APP_API_TIMEOUT: 600
      APP_MESSAGE_SIZE: 50
    ports:
      - "8000:8000"
    volumes:
      - /data/apps/app:/app:ro
      - /data/his_path:/data/his_path:rw
      - /data/last_path:/data/last_path:rw
