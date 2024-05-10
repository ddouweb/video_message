drop table if exists video;
CREATE TABLE `message_receive` (
  `id` int(1) unsigned NOT NULL AUTO_INCREMENT,
  `message_id` int(1) NOT NULL,
  `device_id` varchar(31) DEFAULT NULL,
  `channel_no` int(1) DEFAULT NULL,
  `type` varchar(31) DEFAULT NULL,
  `message_time` timestamp NULL DEFAULT NULL,
  `body` text DEFAULT NULL,
  `data_type` varchar(31) DEFAULT NULL,
  PRIMARY KEY (`id`)
) ENGINE=InnoDB AUTO_INCREMENT=2 DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_general_ci;