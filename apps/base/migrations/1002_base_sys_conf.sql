-- 0006_base_sys_conf.sql
-- 系统配置 kv（BaseFuncClearLog 依赖 logKeep）

CREATE TABLE IF NOT EXISTS `base_sys_conf` (
  `id` BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
  `key` VARCHAR(100) NOT NULL COMMENT '键',
  `value` VARCHAR(255) NOT NULL DEFAULT '' COMMENT '值',
  `createTime` DATETIME(3) NOT NULL,
  `updateTime` DATETIME(3) NOT NULL,
  `deleteTime` DATETIME(3) NULL DEFAULT NULL,
  PRIMARY KEY (`id`),
  UNIQUE KEY `uk_key` (`key`),
  KEY `idx_deleteTime` (`deleteTime`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COMMENT='系统配置';

INSERT INTO `base_sys_conf` (`key`, `value`, `createTime`, `updateTime`)
SELECT 'logKeep', '7', NOW(3), NOW(3)
WHERE NOT EXISTS (SELECT 1 FROM `base_sys_conf` WHERE `key` = 'logKeep');
