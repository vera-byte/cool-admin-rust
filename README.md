# CoolAdminRust

> CoolAdminGo 的 Rust 重写版：**不再使用 Web 框架，对外统一 gRPC**。

📚 **完整文档（Mintlify）**：见 [`docs/`](./docs) 目录，本地预览 `cd docs && mint dev`。
> 完整实现方案见 [RUST_IMPLEMENTATION_PLAN.md](../cool-admin-go/RUST_IMPLEMENTATION_PLAN.md)（决策 D1–D5 已锁定）。

| | |
|---|---|
| 协议 | gRPC（tonic 0.12 + prost 0.13），启用 Server Reflection |
| 运行时 | tokio |
| 数据库 | SQLx（MySQL）+ `migrations/` SQL 迁移 |
| 状态 | **M0–M5 + sa-token 鉴权已完成** |

---

## 1. 目录结构

```
cool-admin-rust/
├── Cargo.toml                  # workspace 根
├── proto/                      # 共享契约（common.v1 / internal.v1）
├── crates/                     # 基础库（可复用）
│   ├── cool-proto/             #   proto 代码生成（聚合各 app 自有 proto）
│   ├── cool-core/              #   核心框架（CRUD / sa-token / 缓存 / 密码）
│   ├── cool-db/                #   SQLx 连接池
│   ├── cool-func/              #   分布式函数（Redis pub/sub）
│   ├── cool-file/              #   文件存储驱动（local/minio/oss）
│   ├── cool-micro/             #   微服务脚手架（分布式会话 / 服务鉴权）
│   └── cool-tools/             #   CLI
├── apps/                       # 业务应用（完全自包含）
│   ├── base/    :8001          #   proto + manifest + migrations + src(lib+bin)
│   ├── dict/    :8002          #   同上
│   ├── space/   :8003
│   ├── task/    :8004
│   ├── demo/                   #   CRUD 示范（仅 lib）
│   └── server/                 #   单体 all-in-one
└── docs/                       # Mintlify 文档
```

---

## 2. 快速开始

### 2.1 依赖

- Rust 1.85+（`rust-toolchain.toml` 锁定 stable）
- `protoc`（编译 proto；macOS: `brew install protobuf`）
- 可选 MySQL（未启动时服务降级运行，Hello/Health 仍可用）

### 2.2 构建 / 测试 / 运行

```bash
# 构建
cargo build

# 单元测试
cargo test

# 运行（默认读取各应用 `apps/<name>/manifest/config.yaml`）
cargo run -p app-server
# 或
./target/debug/cool-admin-rust
```

启动日志示例：

```text
INFO cool_admin_rust: starting cool-admin-rust mode=dev addr=0.0.0.0:8001
WARN cool_db: database unavailable, continue without db: ...
INFO cool_admin_rust: gRPC reflection enabled
INFO cool_admin_rust: gRPC server listening addr=0.0.0.0:8001
```

### 2.3 用 grpcurl 验收（M0 交付标准）

```bash
# 服务发现
grpcurl -plaintext localhost:8001 list
# grpc.reflection.v1.ServerReflection
# internal.v1.InternalService

# Hello
grpcurl -plaintext -d '{"name":"M0"}' localhost:8001 internal.v1.InternalService/Hello
# { "code": 1000, "message": "ok", "data": { "greeting": "Hello, M0! ...", "instanceId": "..." } }

# Health
grpcurl -plaintext -d '{}' localhost:8001 internal.v1.InternalService/Health
# { "code": 1000, "message": "ok", "data": { "status": "degraded", "mode": "dev", "instanceId": "..." } }
```

---

## 3. 核心约定

### 3.1 业务响应（对齐 cool-admin-go）

| 场景 | `code` | 传输 Status |
|---|---|---|
| 成功 | **1000** | `OK` |
| 业务失败 | **1001**（响应体） | `OK` 或业务码 |
| 未认证 | 1001 | `UNAUTHENTICATED` (16) |
| 无权限 | 1001 | `PERMISSION_DENIED` (7) |
| 内部错误 | 1001 | `INTERNAL` (13) |

决策 **D1**：各 RPC 使用 **typed data**（`HelloRes { code, message, HelloData data }`），不用 `google.protobuf.Any`。

### 3.2 鉴权架构（sa-token）

登录与权限由 **[sa-token-rust](https://github.com/sa-tokens/sa-token-rust)**
的 `sa-token-plugin-tonic` 承担（取代自研 JWT + AuthLayer）：

```
请求 → LoggingLayer → SaTokenGrpcLayer(PathAuthConfig) → gRPC Handler
                                ↓
                    StpUtil 会话 / 权限 / 角色
```

| 能力 | 实现 |
|---|---|
| 登录会话 | `StpUtil::login(user_id)` → UUID 令牌 |
| per-RPC 鉴权 | `PathAuthConfig`：`/base.v1.**` 等受保护，登录/验证码/健康检查 exclude |
| 取登录 ID | `get_login_id_from_request(&req)` |
| 权限/角色 | 登录时 `set_permissions` / `set_roles`；handler 内 `check_permission` / `check_role` |
| 登出 | `StpUtil::logout_by_login_id` |
| Token 读取 | `satoken` → `authorization: Bearer x`（自动识别） |

公开接口白名单：`Captcha` / `Login` / `RefreshToken` / `internal.*`；其余要求有效会话。

### 3.3 通用 CRUD 引擎（M1）

```
cool-core/
├── model.rs    # Model trait、ColumnMeta（可写/可过滤白名单）、FieldMap
├── query.rs    # safe_order_by（防注入）、QueryOp、WHERE/ORDER 构造
├── repo.rs     # CrudRepo<M>: add/delete/update/info/list/page
└── crud.rs     # CrudService trait（modify_before/after 钩子）
```

- **软删除**：查询自动过滤 `deleteTime IS NULL`，Delete = `UPDATE deleteTime = NOW(3)`
- **UniqueKey / NotNullKey**：Add/Update 时校验，错误提示可配置
- **列白名单**：Add/Update 只接受模型声明为 `rw` 的列，拒绝非法字段
- **排序白名单**：`safe_order_by` 对齐 Go 正则 `^[A-Za-z0-9_.]+(,[A-Za-z0-9_.]+)*$`
- **分页**：`page/size`，`isExport + maxExportLimit` 对齐 Go PageReq

demo 模块（`demo.v1.DemoSampleService`）暴露 6 + 1 接口：

| RPC | 说明 |
|---|---|
| `Add` | 新增（name 必填、唯一） |
| `Delete` | 批量软删 `ids[]` |
| `Update` | 按 id 局部更新 |
| `Info` | 详情 |
| `List` | 列表（keyword 模糊 name/remark + name 等值） |
| `Page` | 分页（order/sort + 导出上限） |
| `Welcome` | 扩展示例接口 |

grpcurl 示例（需带令牌，M1 仅校验 Bearer 存在）：

```bash
H='authorization: Bearer dev-token'
grpcurl -plaintext -H "$H" -d '{"name":"n1","remark":"r1"}' \
  localhost:8001 demo.v1.DemoSampleService/Add
grpcurl -plaintext -H "$H" -d '{"page":1,"size":5,"order":"id","sort":"desc"}' \
  localhost:8001 demo.v1.DemoSampleService/Page
```

### 3.4 base 权限体系（M2）

| 服务 | 接口 |
|---|---|
| `BaseOpenService` | Captcha / Login / RefreshToken / Logout / PersonInfo / PersonUpdate / PersonPassword |
| `BaseSysUserService` | Page / List / Info / Add / Update / Delete |
| `BaseSysRoleService` | Page / List / Info / Add / Update / Delete（含菜单/部门关联） |
| `BaseSysDepartmentService` | List / Tree / Info / Add / Update / Delete |
| `BaseSysMenuService` | List / Tree / Info / Add / Update / Delete / Perms |
| `BaseSysLogService` | Page |

**安全项**（对齐 cool-admin-go 1.5.12 加固清单）：

- [x] 密码 **argon2** 哈希；旧 MD5 存量登录后自动升级（决策 D5）
- [x] 列表/详情**不回传 password**
- [x] 验证码一次性、5 分钟过期
- [x] 同账号登录失败 5 次锁定 10 分钟
- [x] 统一「账户或密码不正确」防用户名枚举
- [x] refreshToken 校验服务端缓存；登出后立即失效
- [x] 改密 `passwordV+1`，旧 token 立即失效
- [x] JWT 弱密钥黑名单 + 最短 16 位
- [x] PersonUpdate 只能改自己（user_id 来自 token）
- [x] AuthLayer 真实 JWT 校验，未登录返回 `UNAUTHENTICATED`

默认管理员：`admin / 123456`（迁移种子写入）。

```bash
# 完整登录链路
grpcurl -plaintext -d '{}' localhost:8001 base.v1.BaseOpenService/Captcha
grpcurl -plaintext -d '{"username":"admin","password":"123456","captchaId":"...","verifyCode":"..."}' \
  localhost:8001 base.v1.BaseOpenService/Login
grpcurl -plaintext -H "authorization: Bearer <token>" -d '{}' \
  localhost:8001 base.v1.BaseOpenService/PersonInfo
```

### 3.5 M3 模块：dict / space / task / 分布式函数 / 文件

| 服务 | 接口 |
|---|---|
| `DictTypeService` / `DictInfoService` | 6 CRUD + `Data(types)` 聚合 |
| `SpaceTypeService` / `SpaceInfoService` | 6 CRUD（上传走 BaseComm） |
| `TaskInfoService` | 6 CRUD + Start/Stop/Once/Log |
| `BaseCommService` | UploadMode / Upload(bytes) / Download |

- **分布式函数**：`CoolFunc` trait（`is_singleton` / `is_all_worker`）、`run_func("Name(arg)")`、
  `cluster_run_func`（Redis PUBLISH `cool:func` 或本地）、`listen_func` SUBSCRIBE 循环。
  master 选举键 `cool:masterflag`（60s TTL）。内置：`BaseFuncClearLog` / `TaskStartFunc` /
  `TaskStopFunc` / `TaskAddTask` / `TaskTest`。
- **任务调度**：6 段 cron（含秒）+ `@every` + `@yearly/@daily/...`；interval 任务直接用毫秒间隔；
  startDate 门闩；singleton 防重叠；成功日志每任务保留最新 20 条。
- **文件**：`local`（`public/uploads/Ymd/`）/ `minio` / `oss`；`get_mode()` 对 minio/oss 返回
  `mode="local"`（对齐 Go，前端走代理上传）。静态目录由 nginx 提供（`cool.file.domain`）。

### 3.5.1 EPS 与数据范围（M4）

- **EPS**：`BaseOpenService.Eps` / `EpsApp` 返回接口权限元数据（module/prefix/api[method,path,summary]），
  供前端动态菜单/按钮权限使用。注册表 `cool-core::eps`，启动时由 `server/eps.rs` 全量登记。
- **部门数据范围**：用户 Page/List 按当前用户的「自身部门 + 角色关联部门」过滤；
  超管（userId=1 或角色含 1）不受限。对齐 Go 版 1.5.12 越权修复。

### 3.5.2 cool-tools CLI

```bash
cool-tools version           # 版本
cool-tools init my-proj      # 脚手架新项目
cool-tools run               # cargo run 透传
cool-tools build --release   # 发布构建
cool-tools app order         # 生成自包含应用（proto/manifest/migrations/src）
```

### 3.6 配置

`apps/<name>/manifest/config.yaml`，可用环境变量覆盖（前缀 `COOL_`，`__` 分隔层级）：

```bash
COOL_SERVER__ADDRESS=0.0.0.0:9001 ./target/debug/cool-admin-rust
```

关键字段与 cool-admin-go 保持同名：`server.address`、`database.default`、`cool.autoMigrate`、`modules.base.jwt` 等。

### 3.7 数据库

- 连接失败**不阻断启动**（`cool.dbRequired: true` 时才强制退出）
- `cool.autoMigrate: true` 时启动执行 `migrations/`
- 公共列命名对齐 Go 版：`createTime` / `updateTime` / `deleteTime`（软删）

---

## 4. crate 说明

| crate | 职责 | 对应 Go |
|---|---|---|
| `cool-proto` | proto 生成代码、`REFLECTION_DESCRIPTOR` | — |
| `cool-core` | CRUD 引擎、sa-token、错误码、缓存、密码、EPS | `cool/` |
| `cool-db` | SQLx 池、`migrate!`、探活 | `cool/cooldb` |
| `cool-func` | 分布式函数（注册表 / master 选举 / Redis pub/sub） | `cool/func.go` |
| `cool-file` | 文件驱动（local / minio / oss） | `cool/coolfile` |
| `cool-tools` | CLI 脚手架 | `cool-tools/` |
| `apps/base` | 登录 / 用户 / 角色 / 部门 / 菜单 / 日志微服务 | `modules/base` |
| `apps/demo` | CRUD 示范（lib） | `apps/demo` |
| `apps/dict` | 字典微服务 | `modules/dict` |
| `apps/space` | 文件空间微服务 | `modules/space` |
| `apps/task` | 定时任务微服务 | `modules/task` |
| `apps/server` | 单体 all-in-one | `internal/cmd` |

---

## 5. 测试

```bash
# 集成测试需要本地 MySQL
# docker run -d --name cool-mysql -e MYSQL_ROOT_PASSWORD=123456 -e MYSQL_DATABASE=cooltest -p 3306:3306 mysql:8
cargo test
```

当前覆盖 **64 项**：

| 类别 | 位置 | 数量 |
|---|---|---|
| 核心单测（CRUD/JWT/密码/缓存/查询/鉴权/sa-token） | `cool-core/src/**` | 28 |
| 安全回归（注入/越权/token/路径穿越） | `cool-core/tests/security_regression.rs` | 8 |
| 文件驱动（local 回环 / 路径穿越 / URL） | `cool-file` | 6 |
| 任务调度器（cron 解析 / @every / 描述符） | `apps/task/src/scheduler.rs` | 6 |
| 分布式函数（注册表 / 解析 / master） | `cool-func` | 4 |
| base 登录集成（验证码/token/改密） | `apps/base/tests/` | 3 |
| 配置解析（camelCase 绑定） | `apps/server/src/config.rs` | 3 |
| demo CRUD 集成 | `apps/demo/tests/` | 2 |
| task 集成（日志剪枝） | `apps/task/tests/` | 2 |
| dict 递归删除 / 聚合 | `apps/dict/tests/` | 1 |
| 模型列元信息 | `apps/demo` | 1 |

集成验收（本机已跑通）：grpcurl 全模块 23/23、Redis pub/sub 分布式函数、sa-token 登录链路。

---

## 6. 路线图

| 里程碑 | 内容 | 状态 |
|---|---|---|
| **M0** | workspace、proto、核心约定、配置、tracing、Hello/Health、reflection | ✅ |
| **M1** | CRUD 引擎（Model/Query/Repo/CrudService）+ demo 模块 6+1 接口 | ✅ |
| **M2** | base 权限体系（登录/JWT/用户/角色/部门/菜单/日志 + 安全项） | ✅ |
| **M3** | dict / space / task / 分布式函数（Redis pub/sub）/ 文件存储（local/minio/oss） | ✅ |
| **M4** | EPS 元数据、部门数据范围、cool-tools CLI、文档 | ✅ |
| **M5** | 安全回归、部署（Dockerfile/compose/Envoy）、CHANGELOG、gRPC-Web | ✅ |
| **sa-token** | 登录/会话/权限接入 `sa-token-plugin-tonic` | ✅ |

---

## 7. 部署

### docker-compose（MySQL + Redis + App）

```bash
docker compose up -d
# 默认账号 admin / 123456
```

### Dockerfile

`deploy/Dockerfile`：多阶段构建（rust:1.85 → debian-slim），产物 `/app/cool-admin-rust`。

### gRPC-Web 前端对接

浏览器无法直接调 gRPC，需代理。`deploy/envoy.yaml` 提供 **Envoy gRPC-Web** 示例：

```bash
# 8080 = gRPC-Web，转发到 app:8001
docker run --rm -p 8080:8080 \
  -v $(pwd)/deploy/envoy.yaml:/etc/envoy/envoy.yaml \
  envoyproxy/envoy:v1.29-latest
```

前端用 `connect-es` / `grpc-web` 客户端指向 `http://localhost:8080`，
元数据带 `authorization: Bearer <JWT>`。业务码约定：`code=1000` 成功、`1001` 失败。

### 版本脚本

```bash
./scripts/version.sh          # 查看
./scripts/version.sh 0.2.0   # 更新 workspace 版本
```

## 8. 前端对接（gRPC-Web）

后端纯 gRPC（无 HTTP Web 框架）。前端接入方式（决策 D4）：

1. **gRPC-Web**（推荐）：Envoy / `grpc-web` 代理，浏览器用 connect-es 或 grpc-web 客户端
2. **Connect-RPC**：tonic 侧加 connect 适配
3. **临时网关**：REST → gRPC 转换，前端几乎不改

统一约定：`authorization: Bearer <JWT>` 元数据、`code=1000/1001` 业务码、typed data。

## 9. 文档站（Mintlify）

项目文档采用 [Mintlify](https://mintlify.com) 管理，位于 `docs/`：

```text
docs/
├── docs.json          # 站点配置（导航 / 主题 / SEO）
├── index.mdx          # 介绍
├── quickstart.mdx     # 快速开始
├── architecture.mdx   # 架构总览
├── concepts/          # 核心概念（响应 / 鉴权 / CRUD / 配置）
├── modules/           # 业务模块（base / dict / space / task / file）
├── api/               # API 参考（gRPC / EPS）
└── guides/            # 指南（部署 / CLI / 测试 / 前端对接）
```

### 本地预览

```bash
npm i -g mint
cd docs
mint dev          # http://localhost:3000
```

### 校验

```bash
cd docs
mint validate     # 构建校验
mint broken-links # 失效链接检查
```

### 部署

```bash
mint login        # 或 mint signup
cd docs
git push          # 推送到生产分支自动部署
```

---

## 10. 相关文档

- 实现方案（Go → Rust 对照、架构、阶段计划）：`cool-admin-go/RUST_IMPLEMENTATION_PLAN.md`
- CoolAdminGo 使用文档：`cool-admin-go/docs/`
