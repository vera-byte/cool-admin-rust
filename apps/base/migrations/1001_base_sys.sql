-- 0002_base_sys.sql
-- base 权限体系表（对齐 cool-admin-go modules/base）
-- 公共列：id / createTime / updateTime / deleteTime

CREATE TABLE IF NOT EXISTS `base_sys_user` (
  `id` BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
  `departmentId` BIGINT UNSIGNED NOT NULL DEFAULT 0 COMMENT '部门ID',
  `name` VARCHAR(255) NOT NULL DEFAULT '' COMMENT '姓名',
  `username` VARCHAR(100) NOT NULL COMMENT '用户名',
  `password` VARCHAR(255) NOT NULL COMMENT '密码哈希',
  `passwordV` INT NOT NULL DEFAULT 1 COMMENT '密码版本',
  `nickName` VARCHAR(255) NOT NULL DEFAULT '' COMMENT '昵称',
  `headImg` VARCHAR(255) NOT NULL DEFAULT '' COMMENT '头像',
  `phone` VARCHAR(20) NOT NULL DEFAULT '' COMMENT '手机',
  `email` VARCHAR(255) NOT NULL DEFAULT '' COMMENT '邮箱',
  `status` TINYINT NOT NULL DEFAULT 1 COMMENT '状态 0禁用 1启用',
  `remark` VARCHAR(255) NOT NULL DEFAULT '' COMMENT '备注',
  `createTime` DATETIME(3) NOT NULL,
  `updateTime` DATETIME(3) NOT NULL,
  `deleteTime` DATETIME(3) NULL DEFAULT NULL,
  PRIMARY KEY (`id`),
  UNIQUE KEY `uk_username` (`username`),
  KEY `idx_departmentId` (`departmentId`),
  KEY `idx_deleteTime` (`deleteTime`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COMMENT='系统用户';

CREATE TABLE IF NOT EXISTS `base_sys_role` (
  `id` BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
  `name` VARCHAR(255) NOT NULL COMMENT '角色名',
  `label` VARCHAR(255) NOT NULL DEFAULT '' COMMENT '角色标识',
  `remark` VARCHAR(255) NOT NULL DEFAULT '' COMMENT '备注',
  `createTime` DATETIME(3) NOT NULL,
  `updateTime` DATETIME(3) NOT NULL,
  `deleteTime` DATETIME(3) NULL DEFAULT NULL,
  PRIMARY KEY (`id`),
  UNIQUE KEY `uk_name` (`name`),
  KEY `idx_deleteTime` (`deleteTime`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COMMENT='角色';

CREATE TABLE IF NOT EXISTS `base_sys_department` (
  `id` BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
  `name` VARCHAR(255) NOT NULL COMMENT '名称',
  `parentId` BIGINT UNSIGNED NOT NULL DEFAULT 0 COMMENT '父ID',
  `sort` INT NOT NULL DEFAULT 0 COMMENT '排序',
  `createTime` DATETIME(3) NOT NULL,
  `updateTime` DATETIME(3) NOT NULL,
  `deleteTime` DATETIME(3) NULL DEFAULT NULL,
  PRIMARY KEY (`id`),
  KEY `idx_parentId` (`parentId`),
  KEY `idx_deleteTime` (`deleteTime`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COMMENT='部门';

CREATE TABLE IF NOT EXISTS `base_sys_menu` (
  `id` BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
  `name` VARCHAR(255) NOT NULL COMMENT '名称',
  `parentId` BIGINT UNSIGNED NOT NULL DEFAULT 0 COMMENT '父ID',
  `router` VARCHAR(255) NOT NULL DEFAULT '' COMMENT '路由',
  `perms` VARCHAR(255) NOT NULL DEFAULT '' COMMENT '权限标识',
  `type` TINYINT NOT NULL DEFAULT 1 COMMENT '类型 1目录 2菜单 3权限',
  `icon` VARCHAR(255) NOT NULL DEFAULT '' COMMENT '图标',
  `orderNum` INT NOT NULL DEFAULT 0 COMMENT '排序',
  `viewPath` VARCHAR(255) NOT NULL DEFAULT '' COMMENT '视图路径',
  `keepAlive` TINYINT NOT NULL DEFAULT 1 COMMENT '缓存',
  `isShow` TINYINT NOT NULL DEFAULT 1 COMMENT '显示',
  `isAuth` TINYINT NOT NULL DEFAULT 1 COMMENT '鉴权',
  `remark` VARCHAR(255) NOT NULL DEFAULT '' COMMENT '备注',
  `createTime` DATETIME(3) NOT NULL,
  `updateTime` DATETIME(3) NOT NULL,
  `deleteTime` DATETIME(3) NULL DEFAULT NULL,
  PRIMARY KEY (`id`),
  KEY `idx_parentId` (`parentId`),
  KEY `idx_deleteTime` (`deleteTime`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COMMENT='菜单权限';

CREATE TABLE IF NOT EXISTS `base_sys_user_role` (
  `id` BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
  `userId` BIGINT UNSIGNED NOT NULL,
  `roleId` BIGINT UNSIGNED NOT NULL,
  `createTime` DATETIME(3) NOT NULL,
  `updateTime` DATETIME(3) NOT NULL,
  `deleteTime` DATETIME(3) NULL DEFAULT NULL,
  PRIMARY KEY (`id`),
  UNIQUE KEY `uk_user_role` (`userId`,`roleId`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COMMENT='用户角色关联';

CREATE TABLE IF NOT EXISTS `base_sys_role_menu` (
  `id` BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
  `roleId` BIGINT UNSIGNED NOT NULL,
  `menuId` BIGINT UNSIGNED NOT NULL,
  `createTime` DATETIME(3) NOT NULL,
  `updateTime` DATETIME(3) NOT NULL,
  `deleteTime` DATETIME(3) NULL DEFAULT NULL,
  PRIMARY KEY (`id`),
  UNIQUE KEY `uk_role_menu` (`roleId`,`menuId`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COMMENT='角色菜单关联';

CREATE TABLE IF NOT EXISTS `base_sys_role_department` (
  `id` BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
  `roleId` BIGINT UNSIGNED NOT NULL,
  `departmentId` BIGINT UNSIGNED NOT NULL,
  `createTime` DATETIME(3) NOT NULL,
  `updateTime` DATETIME(3) NOT NULL,
  `deleteTime` DATETIME(3) NULL DEFAULT NULL,
  PRIMARY KEY (`id`),
  UNIQUE KEY `uk_role_dept` (`roleId`,`departmentId`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COMMENT='角色部门关联';

CREATE TABLE IF NOT EXISTS `base_sys_log` (
  `id` BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
  `userId` BIGINT UNSIGNED NOT NULL DEFAULT 0 COMMENT '用户ID',
  `username` VARCHAR(100) NOT NULL DEFAULT '' COMMENT '用户名',
  `action` VARCHAR(255) NOT NULL DEFAULT '' COMMENT '动作',
  `method` VARCHAR(255) NOT NULL DEFAULT '' COMMENT '方法',
  `params` TEXT COMMENT '参数(已脱敏)',
  `ip` VARCHAR(64) NOT NULL DEFAULT '' COMMENT 'IP',
  `result` TEXT COMMENT '结果',
  `createTime` DATETIME(3) NOT NULL,
  `updateTime` DATETIME(3) NOT NULL,
  `deleteTime` DATETIME(3) NULL DEFAULT NULL,
  PRIMARY KEY (`id`),
  KEY `idx_userId` (`userId`),
  KEY `idx_deleteTime` (`deleteTime`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COMMENT='操作日志';

-- 种子：超级管理员（密码为 MD5(123456)，首次登录后由服务升级为 argon2）
INSERT INTO `base_sys_user`
  (`departmentId`,`name`,`username`,`password`,`passwordV`,`nickName`,`status`,`remark`,`createTime`,`updateTime`)
SELECT 1, '超级管理员', 'admin', 'e10adc3949ba59abbe56e057f20f883e', 1, 'Admin', 1, '内置管理员', NOW(3), NOW(3)
WHERE NOT EXISTS (SELECT 1 FROM `base_sys_user` WHERE `username` = 'admin');

INSERT INTO `base_sys_department`
  (`name`,`parentId`,`sort`,`createTime`,`updateTime`)
SELECT '总公司', 0, 0, NOW(3), NOW(3)
WHERE NOT EXISTS (SELECT 1 FROM `base_sys_department` WHERE `name` = '总公司');

INSERT INTO `base_sys_role`
  (`name`,`label`,`remark`,`createTime`,`updateTime`)
SELECT '超级管理员', 'admin', '内置角色', NOW(3), NOW(3)
WHERE NOT EXISTS (SELECT 1 FROM `base_sys_role` WHERE `name` = '超级管理员');

-- admin ↔ 超级管理员 关联（取当前各自行 ID）
INSERT INTO `base_sys_user_role` (`userId`,`roleId`,`createTime`,`updateTime`)
SELECT u.`id`, r.`id`, NOW(3), NOW(3)
FROM `base_sys_user` u
JOIN `base_sys_role` r ON r.`name` = '超级管理员'
WHERE u.`username` = 'admin'
  AND NOT EXISTS (
    SELECT 1 FROM `base_sys_user_role` ur
    WHERE ur.`userId` = u.`id` AND ur.`roleId` = r.`id` AND ur.`deleteTime` IS NULL
  );
