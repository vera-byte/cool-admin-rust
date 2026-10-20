# Changelog

## 0.2.0 (2026-05-28)

### 变更

- **鉴权重构：接入 [sa-token-rust](https://github.com/sa-tokens/sa-token-rust)**
  - 使用 `sa-token-plugin-tonic`（Tower `SaTokenGrpcLayer` + `PathAuthConfig`）
  - 登录会话：`StpUtil::login` / `logout` / `set_permissions` / `set_roles`
  - handler 取登录 ID：`get_login_id_from_request`
  - 权限/角色校验：`check_permission` / `check_role`
  - Token 读取优先级：`satoken` → `authorization: Bearer`
  - 移除自研 `AuthLayer` JWT 中间件（`cool-core::jwt` 仅保留 refreshToken 占位与密码学工具）
- 保留业务契约：`Login` 仍返回 `{token, refreshToken, expire, refreshExpire}`
- 前端兼容：`authorization: Bearer <token>` 与 `satoken: <token>` 均可

### 验收

- 64 项测试全绿（含 sa-token 登录/权限/登出单测）
- 本地冒烟 23/23（登录 → 权限接口 → 全模块 → 安全拦截）

---

## 0.1.0 (2026-05-28)

M0–M5 里程碑完成，cool-admin-go 的 Rust + gRPC 重写版。

### 概览

| 维度 | 规模 |
|---|---|
| gRPC 服务 | 15 个（base/demo/dict/space/task/internal） |
| gRPC RPC | 70+ |
| crate | 11（cool-proto/core/db/func/file + 5 modules + server + cool-tools） |
| SQL 迁移 | 6（demo/base/dict/space/task/conf） |
| 测试 | 62 项（含 8 项安全回归），clippy 零告警 |
| 本地冒烟 | 23/23 通过（含 Redis pub/sub 分布式函数） |

### 新增

**核心框架（cool-core）**
- 通用 CRUD 引擎：`Model` / `CrudRepo` / `CrudService`（Add/Delete/Update/Info/List/Page）
- 软删除（`deleteTime`）、UniqueKey/NotNullKey、列白名单、排序白名单（防 ORDER BY 注入）
- JWT（HS256 + 弱密钥黑名单 + passwordVersion）、argon2 密码（旧 MD5 自动升级）
- 异步 `CacheStore`（内存 / Redis，2s 超时回退）
- 鉴权中间件（`AuthLayer` tower layer，Bearer JWT 注入 `AuthUser`）
- EPS 元数据注册表、统一错误码（1000/1001）

**业务模块**
- `base`：登录/验证码/刷新/登出/个人信息/改密、用户/角色/部门/菜单/日志、
  部门数据范围过滤、`BaseComm` 文件上传下载
- `demo`：CRUD 示范（含 Welcome 扩展）
- `dict`：类型/条目 + `Data` 聚合 + 递归删除
- `space`：分类/文件索引 CRUD
- `task`：定时任务（6 段 cron + `@every` + 间隔）+ Start/Stop/Once/Log + 日志剪枝

**基础设施**
- 分布式函数（`cool-func`）：`CoolFunc` 注册表、master 选举、Redis pub/sub `cool:func`
- 文件存储（`cool-file`）：local / minio / oss 三驱动
- 配置加载（camelCase 绑定修复，`redis.cool` / `cool.file`）
- gRPC Server Reflection、EPS 查询接口

**工具与工程**
- `cool-tools` CLI：init / run / build / module / version
- SQLx 迁移 6 个（demo/base/dict/space/task/conf）+ 种子数据
- 安全回归测试 8 项（对齐 cool-admin-go 1.5.12 加固清单）

### 安全加固（对齐 cool-admin-go 1.5.12）

- 密码 argon2 哈希；旧 MD5 登录后自动升级
- 列表/详情不回传 password
- 验证码一次性、失败 5 次锁定、统一错误防枚举
- refreshToken 校验服务端缓存；登出/改密后旧 token 立即失效
- JWT 弱密钥黑名单 + 最短 16 位
- ORDER BY 注入防护、列名白名单、文件路径穿越防护
- PersonUpdate 只能改自己（user_id 来自 token）
- 部门数据范围过滤（普通管理员不可越权）

### 验收记录（本地）

- `cargo test`：62 项全绿（core 26 + security 8 + 集成/单测 28）
- `cargo clippy --workspace`：零告警；`cargo fmt --all` 已格式化
- 本地部署冒烟 23/23：服务发现 / 登录 / 安全拦截 / 用户角色部门菜单 / 字典 / 空间 / 任务 / 文件上传 / EPS / demo
- Redis pub/sub：`PUBLISH cool:func` 触发 `TaskTest` 执行成功
- grpcurl 全量接口验收通过

### 已知限制

- minio/oss 驱动需真实对象存储验收（本地已用 local 驱动验证全链路）
- gRPC-Web 前端对接需 Envoy/网关（见 README §7）
- 多节点无 Redis 时每节点各自调度（与 Go 版行为一致）
- 操作日志参数脱敏为占位实现（完整脱敏见 Go 版 1.5.12）
