-- 0001_init_demo_sample.sql
-- 示例表：对应 Go 版 modules/demo model.DemoSample
-- 公共列命名对齐 cool-admin-go：createTime / updateTime / deleteTime

CREATE TABLE IF NOT EXISTS `demo_sample` (
  `id` BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
  `name` VARCHAR(255) NOT NULL DEFAULT '' COMMENT '名称',
  `remark` VARCHAR(500) NOT NULL DEFAULT '' COMMENT '备注',
  `createTime` DATETIME(3) NOT NULL COMMENT '创建时间',
  `updateTime` DATETIME(3) NOT NULL COMMENT '更新时间',
  `deleteTime` DATETIME(3) NULL DEFAULT NULL COMMENT '软删除时间',
  PRIMARY KEY (`id`),
  KEY `idx_deleteTime` (`deleteTime`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COMMENT='演示示例';
