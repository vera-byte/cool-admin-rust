-- 0005_task.sql
-- 定时任务（对齐 cool-admin-go modules/task）

CREATE TABLE IF NOT EXISTS `task_info` (
  `id` BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
  `jobId` VARCHAR(255) NOT NULL DEFAULT '' COMMENT '调度器JobId',
  `repeatConf` VARCHAR(255) NOT NULL DEFAULT '' COMMENT '重复配置',
  `name` VARCHAR(255) NOT NULL COMMENT '任务名',
  `cron` VARCHAR(255) NOT NULL DEFAULT '' COMMENT 'cron 表达式(6段)或 @every',
  `limit` INT NOT NULL DEFAULT 0 COMMENT '执行次数限制',
  `every` INT NOT NULL DEFAULT 0 COMMENT '间隔毫秒(taskType=1)',
  `remark` VARCHAR(255) NOT NULL DEFAULT '' COMMENT '备注',
  `status` TINYINT NOT NULL DEFAULT 0 COMMENT '状态 0停用 1启用',
  `startDate` DATETIME(3) NULL DEFAULT NULL COMMENT '开始时间',
  `endDate` DATETIME(3) NULL DEFAULT NULL COMMENT '结束时间',
  `data` VARCHAR(255) NOT NULL DEFAULT '' COMMENT '附加数据',
  `service` VARCHAR(255) NOT NULL DEFAULT '' COMMENT '函数串 Name(param)',
  `type` TINYINT NOT NULL DEFAULT 0 COMMENT '类型 0系统 1用户',
  `nextRunTime` DATETIME(3) NULL DEFAULT NULL COMMENT '下次运行',
  `taskType` TINYINT NOT NULL DEFAULT 0 COMMENT '0=cron 1=间隔',
  `createTime` DATETIME(3) NOT NULL,
  `updateTime` DATETIME(3) NOT NULL,
  `deleteTime` DATETIME(3) NULL DEFAULT NULL,
  PRIMARY KEY (`id`),
  UNIQUE KEY `uk_name` (`name`),
  KEY `idx_status` (`status`),
  KEY `idx_deleteTime` (`deleteTime`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COMMENT='定时任务';

CREATE TABLE IF NOT EXISTS `task_log` (
  `id` BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
  `taskId` BIGINT UNSIGNED NOT NULL COMMENT '任务ID',
  `status` TINYINT NOT NULL DEFAULT 0 COMMENT '0失败 1成功',
  `detail` TEXT COMMENT '详情',
  `createTime` DATETIME(3) NOT NULL,
  `updateTime` DATETIME(3) NOT NULL,
  `deleteTime` DATETIME(3) NULL DEFAULT NULL,
  PRIMARY KEY (`id`),
  KEY `idx_taskId` (`taskId`),
  KEY `idx_deleteTime` (`deleteTime`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COMMENT='任务日志';

-- 种子：清理日志任务（对齐 Go initjson）
INSERT INTO `task_info`
  (`name`,`cron`,`service`,`status`,`type`,`taskType`,`remark`,`createTime`,`updateTime`)
SELECT '清理日志', '1 2 3 * * *', 'BaseFuncClearLog(false)', 1, 0, 0, '每天清理过期操作日志', NOW(3), NOW(3)
WHERE NOT EXISTS (SELECT 1 FROM `task_info` WHERE `name` = '清理日志');
