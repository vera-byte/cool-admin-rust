-- 0004_space.sql
-- 文件空间（对齐 cool-admin-go modules/space）

CREATE TABLE IF NOT EXISTS `space_type` (
  `id` BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
  `name` VARCHAR(255) NOT NULL COMMENT '名称',
  `parentId` BIGINT UNSIGNED NULL DEFAULT NULL COMMENT '父ID',
  `createTime` DATETIME(3) NOT NULL,
  `updateTime` DATETIME(3) NOT NULL,
  `deleteTime` DATETIME(3) NULL DEFAULT NULL,
  PRIMARY KEY (`id`),
  KEY `idx_parentId` (`parentId`),
  KEY `idx_deleteTime` (`deleteTime`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COMMENT='空间分类';

CREATE TABLE IF NOT EXISTS `space_info` (
  `id` BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
  `url` VARCHAR(500) NOT NULL COMMENT '文件URL',
  `type` VARCHAR(50) NOT NULL DEFAULT 'file' COMMENT '类型 image/video/audio/file',
  `classifyId` BIGINT UNSIGNED NULL DEFAULT NULL COMMENT '分类ID',
  `createTime` DATETIME(3) NOT NULL,
  `updateTime` DATETIME(3) NOT NULL,
  `deleteTime` DATETIME(3) NULL DEFAULT NULL,
  PRIMARY KEY (`id`),
  KEY `idx_classifyId` (`classifyId`),
  KEY `idx_type` (`type`),
  KEY `idx_deleteTime` (`deleteTime`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COMMENT='空间文件索引';
