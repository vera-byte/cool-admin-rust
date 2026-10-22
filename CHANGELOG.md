# Changelog

## 0.5.0 (2026-10-22)

### 架构升级：APISIX Service + 服务发现

**cool-apisix 重构**：每微服务一个 APISIX Service，路由绑定 Service，上游走服务发现。

| 变更 | 说明 |
| --- | --- |
| `MicroService` | 微服务定义（id/package/scheme/discovery） |
| `ServiceDiscovery` | DNS / Kubernetes / Nacos / ConsulKV / Eureka / 静态节点 |
| `register_microservice` | 一键创建 Service + gRPC 路由 + gRPC-Web 路由 |
| `register_service` / `bind_route` | Service 与路由分离管理 |
| 路由绑定 | `service_id` 引用，不内联 upstream |
| 多实例 | 服务发现自动管理（无需手工合并节点） |

```rust
// Kubernetes 服务发现（生产推荐）
let svc = MicroService::grpc_k8s("dict-service", "dict.v1", "dict-service", "default");
register_microservice(&cfg, &svc).await?;
```

### 验收

- 11 项 cool-apisix 测试（Service 生成 / 路由绑定 / 多服务发现类型）
- 75 测试全绿、clippy 零告警

---

## 0.4.0 (2026-10-22)

### 新增：APISIX 网关集成（cool-apisix）

- **按需引入**：仅依赖 `cool-apisix` 的微服务才注册路由，未引入零副作用
- `auto_register(service_id, package, port)`：一键注册 gRPC 路由（`/package/*`）
- `register_routes` / `deregister_route`：批量 upsert / 删除
- gRPC 代理：`upstream.scheme: grpc` + Ant 风格 URI 匹配
- 配置：`apisix` 段（adminUrl/adminKey/upstreamHost/scheme/prefix）或环境变量
- 兼容 APISIX **3.19.0**：遵守 `uri`/`uris` 二选一校验

### 验收

- 单测 7 项（报文构造 / URL 拼接 / mock Admin API 集成）
- 真实 APISIX 3.19.0 注册验证：`cool-base-service` + `cool-space-service` 两条 gRPC 路由
- 71 测试全绿、clippy 零告警

---

## 0.3.0 (2026-10-21)

### 架构演进：apps/ 微服务化

**目录重构**：`modules/` + `services/` 合并为 `apps/`，每个应用完全自包含。

```text
apps/<name>/ = proto/ + manifest/ + migrations/ + src(lib+bin)
```

| 变更 | 说明 |
| --- | --- |
| `apps/` | 业务应用单元（base/dict/space/task/demo/server） |
| `crates/` | 只保留基础库（cool-proto/core/db/func/file/micro/tools） |
| `proto/` | 仅存共享契约（common.v1 / internal.v1） |
| 包名 | `mod-*` → `app-*` |
| proto 归位 | `apps/<name>/proto/<pkg>/v1/`，cool-proto 聚合生成 |
| manifest 归位 | `apps/<name>/manifest/config.yaml` |
| migrations 归位 | `apps/<name>/migrations/`，版本号分段（base=10xx/demo=20xx/dict=30xx/space=40xx/task=50xx） |

### 微服务架构

- `base-service :8001` / `dict-service :8002` / `space-service :8003` / `task-service :8004`
- sa-token **分布式会话**：共享 Redis 登录态，base 登录 token 全服务可用
- **服务凭证**：`x-service-id` + `x-service-secret`（DistributedSessionManager）
- 单体 `cool-admin-rust` 保留，与微服务共享同一套业务代码

### 迁移平滑升级

- 各应用 Migrator 启用 `set_ignore_missing(true)`
- **已有数据库无需清理**：旧版本记录（1–6）自动忽略，新版本号（1001+）追加执行
- 表结构 `CREATE TABLE IF NOT EXISTS`、种子 `WHERE NOT EXISTS` 幂等安全

### 文档与工程清理

- Mintlify 文档全面对齐 apps/ 结构
- 新增「微服务架构」「数据库迁移」章节
- 代码注释 / 默认路径 / Dockerfile 残留路径统一为 apps/ 布局
- cool-tools 脚手架模板、cool-micro 配置默认路径同步修正

### 验收

- 64 测试全绿、clippy 零告警、文档零失效链接
- 4 微服务独立启动 + 跨服务分布式会话验证通过
- 迁移平滑升级实测：旧记录 1-6 保留、新 1001+ 追加、服务正常

---

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
| gRPC 服务 | 14 个业务服务（base/demo/dict/space/task/internal）+ Reflection |
| gRPC RPC | 82 |
| crate | 12（cool-proto/core/db/func/file/tools + 5 modules + server） |
| SQL 迁移 | 6（demo/base/dict/space/task/conf） |
| 测试 | 64 项（含 8 项安全回归），clippy 零告警 |
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

- `cargo test`：64 项全绿（core 28 + security 8 + 集成/单测 28）
- `cargo clippy --workspace`：零告警；`cargo fmt --all` 已格式化
- 本地部署冒烟 23/23：服务发现 / 登录 / 安全拦截 / 用户角色部门菜单 / 字典 / 空间 / 任务 / 文件上传 / EPS / demo
- Redis pub/sub：`PUBLISH cool:func` 触发 `TaskTest` 执行成功
- grpcurl 全量接口验收通过

### 已知限制

- minio/oss 驱动需真实对象存储验收（本地已用 local 驱动验证全链路）
- gRPC-Web 前端对接需 Envoy/网关（见 README §7）
- 多节点无 Redis 时每节点各自调度（与 Go 版行为一致）
- 操作日志参数脱敏为占位实现（完整脱敏见 Go 版 1.5.12）
