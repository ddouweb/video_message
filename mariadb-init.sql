CREATE DATABASE IF NOT EXISTS video;
USE video;

-- 删除已有的表
DROP TABLE IF EXISTS message_receive;
DROP TABLE IF EXISTS message_img;

-- 消息接收记录
CREATE TABLE `message_receive` (
  `id` INT UNSIGNED NOT NULL AUTO_INCREMENT,
  `message_id` VARCHAR(31) NOT NULL,
  `device_id` VARCHAR(31) DEFAULT NULL,
  `channel_no` INT DEFAULT NULL,
  `type` VARCHAR(31) DEFAULT NULL,
  `message_time` BIGINT UNSIGNED DEFAULT NULL,
  `body` TEXT DEFAULT NULL,
  `data_type` VARCHAR(31) DEFAULT NULL,
  `data_dir` VARCHAR(12) DEFAULT NULL,
  `create_time` DATETIME DEFAULT CURRENT_TIMESTAMP,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB AUTO_INCREMENT=1 DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

-- 图片分析结果
CREATE TABLE `message_img` (
  `id` INT UNSIGNED NOT NULL AUTO_INCREMENT,
  `message_id` VARCHAR(31) NOT NULL,
  `channel_name` VARCHAR(255) DEFAULT NULL,
  `url` VARCHAR(2048) DEFAULT NULL,
  `data_type` VARCHAR(31) DEFAULT NULL,
  `create_time` DATETIME DEFAULT CURRENT_TIMESTAMP,
  `read_count` INT UNSIGNED DEFAULT 0,
  `data_dir` varchar(10) DEFAULT NULL,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB AUTO_INCREMENT=18 DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;

-- 创建用户并设置密码
CREATE USER IF NOT EXISTS 'video'@'%' IDENTIFIED BY '123456';

-- 授予用户对数据库的所有权限
GRANT ALL PRIVILEGES ON video.* TO 'video'@'%';
