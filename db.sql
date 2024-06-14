drop table if exists video;
CREATE TABLE `message_receive` (
  `id` int(1) unsigned NOT NULL AUTO_INCREMENT,
  `message_id` varchar(31) NOT NULL,
  `device_id` varchar(31) DEFAULT NULL,
  `channel_no` int(1) DEFAULT NULL,
  `type` varchar(31) DEFAULT NULL,
  `message_time` bigint(1) unsigned DEFAULT NULL,
  `body` text DEFAULT NULL,
  `data_type` varchar(31) DEFAULT NULL,
  `data_dir` varchar(12) DEFAULT NULL,
  `create_time` datetime DEFAULT current_timestamp(),
  PRIMARY KEY (`id`)
) ENGINE=InnoDB AUTO_INCREMENT=1 DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;


CREATE TABLE `message_img` (
  `id` int(1) unsigned NOT NULL AUTO_INCREMENT,
  `message_id` varchar(31) NOT NULL,
  `channel_name` varchar(255) DEFAULT NULL,
  `url` varchar(2048) DEFAULT NULL,
  `data_type` varchar(31) DEFAULT NULL,
  `create_time` datetime DEFAULT current_timestamp(),
  `read_count` int(1) unsigned DEFAULT 0,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB AUTO_INCREMENT=18 DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;