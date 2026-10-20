-- 0003_dict.sql
-- 字典（对齐 cool-admin-go modules/dict）

CREATE TABLE IF NOT EXISTS `dict_type` (
  `id` BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
  `name` VARCHAR(255) NOT NULL COMMENT '名称',
  `key` VARCHAR(255) NOT NULL COMMENT '标识',
  `createTime` DATETIME(3) NOT NULL,
  `updateTime` DATETIME(3) NOT NULL,
  `deleteTime` DATETIME(3) NULL DEFAULT NULL,
  PRIMARY KEY (`id`),
  UNIQUE KEY `uk_key` (`key`),
  KEY `idx_deleteTime` (`deleteTime`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COMMENT='字典类型';

CREATE TABLE IF NOT EXISTS `dict_info` (
  `id` BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
  `typeId` BIGINT UNSIGNED NOT NULL COMMENT '类型ID',
  `name` VARCHAR(255) NOT NULL COMMENT '名称',
  `orderNum` INT NOT NULL DEFAULT 0 COMMENT '排序',
  `remark` VARCHAR(255) NULL DEFAULT NULL COMMENT '备注',
  `parentId` BIGINT UNSIGNED NULL DEFAULT NULL COMMENT '父ID',
  `createTime` DATETIME(3) NOT NULL,
  `updateTime` DATETIME(3) NOT NULL,
  `deleteTime` DATETIME(3) NULL DEFAULT NULL,
  PRIMARY KEY (`id`),
  KEY `idx_typeId` (`typeId`),
  KEY `idx_parentId` (`parentId`),
  KEY `idx_deleteTime` (`deleteTime`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COMMENT='字典条目';

-- 种子（对齐 Go initjson，不写死 id）
INSERT INTO `dict_type` (`name`, `key`, `createTime`, `updateTime`)
SELECT '类别', 'type', NOW(3), NOW(3)
WHERE NOT EXISTS (SELECT 1 FROM `dict_type` WHERE `key` = 'type');

INSERT INTO `dict_type` (`name`, `key`, `createTime`, `updateTime`)
SELECT '品牌', 'brand', NOW(3), NOW(3)
WHERE NOT EXISTS (SELECT 1 FROM `dict_type` WHERE `key` = 'brand');

INSERT INTO `dict_info` (`typeId`, `name`, `orderNum`, `createTime`, `updateTime`)
SELECT t.`id`, x.`name`, x.`orderNum`, NOW(3), NOW(3)
FROM `dict_type` t
JOIN (SELECT '衣服' AS `name`, 1 AS `orderNum`, 'type' AS `tkey`
      UNION ALL SELECT '裤子', 2, 'type'
      UNION ALL SELECT '鞋子', 3, 'type') x ON x.`tkey` = t.`key`
WHERE NOT EXISTS (SELECT 1 FROM `dict_info` d WHERE d.`name` = x.`name` AND d.`typeId` = t.`id`);

INSERT INTO `dict_info` (`typeId`, `name`, `orderNum`, `createTime`, `updateTime`)
SELECT t.`id`, x.`name`, x.`orderNum`, NOW(3), NOW(3)
FROM `dict_type` t
JOIN (SELECT '闪酷' AS `name`, 1 AS `orderNum`, 'brand' AS `tkey`
      UNION ALL SELECT 'COOL', 2, 'brand') x ON x.`tkey` = t.`key`
WHERE NOT EXISTS (SELECT 1 FROM `dict_info` d WHERE d.`name` = x.`name` AND d.`typeId` = t.`id`);
