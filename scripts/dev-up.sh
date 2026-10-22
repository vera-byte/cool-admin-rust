#!/usr/bin/env bash
# ============================================================
# cool-admin-rust 本地一键启动
#
# 拉起依赖（MySQL/Redis/etcd/APISIX）+ 4 微服务 + 冒烟测试
#
# 用法:
#   ./scripts/dev-up.sh          # 启动全部 + 冒烟
#   ./scripts/dev-up.sh up       # 仅启动
#   ./scripts/dev-up.sh smoke    # 仅冒烟
#   ./scripts/dev-up.sh down     # 停止
# ============================================================
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
export PATH="$HOME/go/bin:$PATH"

NETWORK="cool-net"
MYSQL="cool-mysql-test"
REDIS="cool-redis"
ETCD="cool-rust-etcd"
APISIX="cool-rust-apisix"
APISIX_ADMIN="http://127.0.0.1:9180/apisix/admin"
APISIX_KEY="edd1c9f034335f136f87ad84b625c8f1"
NACOS_IMG="swr.cn-north-4.myhuaweicloud.com/ddn-k8s/docker.io/apache/apisix:3.19.0-debian"
ETCD_IMG="quay.io/coreos/etcd:v3.5.13"

log() { echo -e "\033[36m[demo]\033[0m $*"; }
ok()  { echo -e "\033[32m  ✅\033[0m $*"; }
err() { echo -e "\033[31m  ❌\033[0m $*"; }

# ---------- 基础设施 ----------
start_infra() {
  log "启动基础设施 (MySQL / Redis / etcd / APISIX)"
  docker network create "$NETWORK" 2>/dev/null || true

  # MySQL
  if ! docker ps --format '{{.Names}}' | grep -q "^$MYSQL$"; then
    docker rm -f "$MYSQL" 2>/dev/null || true
    docker run -d --name "$MYSQL" --network "$NETWORK" \
      -e MYSQL_ROOT_PASSWORD=123456 -e MYSQL_DATABASE=cooltest \
      -p 3306:3306 mysql:8 \
      --default-authentication-plugin=mysql_native_password >/dev/null
    ok "MySQL 启动"
  else
    ok "MySQL 已运行"
  fi

  # Redis
  if ! docker ps --format '{{.Names}}' | grep -q "^$REDIS$"; then
    docker rm -f "$REDIS" 2>/dev/null || true
    docker run -d --name "$REDIS" --network "$NETWORK" -p 6379:6379 redis:7 >/dev/null
    ok "Redis 启动"
  else
    ok "Redis 已运行"
  fi

  # etcd
  if ! docker ps --format '{{.Names}}' | grep -q "^$ETCD$"; then
    docker rm -f "$ETCD" 2>/dev/null || true
    docker run -d --name "$ETCD" --network "$NETWORK" -p 2379:2379 \
      "$ETCD_IMG" /usr/local/bin/etcd \
      --advertise-client-urls http://cool-rust-etcd:2379 \
      --listen-client-urls http://0.0.0.0:2379 >/dev/null
    ok "etcd 启动"
  else
    ok "etcd 已运行"
  fi

  # APISIX（etcd provider + host.docker.internal 映射）
  if ! docker ps --format '{{.Names}}' | grep -q "^$APISIX$"; then
    docker rm -f "$APISIX" 2>/dev/null || true
    docker run -d --name "$APISIX" --network "$NETWORK" \
      --add-host=host.docker.internal:host-gateway \
      -p 9080:9080 -p 9180:9180 \
      -v "$ROOT/deploy/apisix.yaml:/usr/local/apisix/conf/config.yaml" \
      "$NACOS_IMG" >/dev/null
    ok "APISIX 启动"
  else
    ok "APISIX 已运行"
  fi

  log "等待依赖就绪..."
  for i in $(seq 1 30); do
    if curl -sf -m 2 "$APISIX_ADMIN/routes" -H "X-API-KEY: $APISIX_KEY" >/dev/null 2>&1; then
      ok "APISIX Admin API 就绪"
      return 0
    fi
    sleep 2
  done
  err "APISIX 启动超时，查看: docker logs $APISIX"
  return 1
}

# ---------- 构建 ----------
build() {
  log "构建微服务"
  cargo build -p app-base -p app-dict -p app-space -p app-task 2>&1 | tail -2
  ok "构建完成"
}

# ---------- 启动微服务 ----------
start_services() {
  log "启动 4 个微服务（APISIX_ENABLED=1 自动注册）"
  pkill -f "target/debug/.*-service" 2>/dev/null || true
  sleep 1
  for pair in "base:8001" "dict:8002" "space:8003" "task:8004"; do
    name="${pair%%:*}"; port="${pair##*:}"
    APISIX_ENABLED=1 \
    APISIX_ADMIN_URL="$APISIX_ADMIN" \
    APISIX_ADMIN_KEY="$APISIX_KEY" \
    APISIX_UPSTREAM_HOST=host.docker.internal \
    MICRO_CONFIG="apps/${name}/manifest/config.yaml" \
    ./target/debug/${name}-service > "/tmp/${name}-svc.log" 2>&1 &
    ok "${name}-service :${port} (PID=$!)"
  done
  sleep 6
  for pair in "base:8001" "dict:8002" "space:8003" "task:8004"; do
    port="${pair##*:}"
    lsof -tiTCP:$port -sTCP:LISTEN >/dev/null 2>&1 && ok ":${port} LISTEN" || err ":${port} DOWN"
  done
}

# ---------- 冒烟测试 ----------
smoke() {
  log "运行网关冒烟测试"
  local GW=localhost:9080
  local P=(-import-path proto -import-path apps/base/proto -import-path apps/base/proto/base/v1
    -import-path apps/dict/proto -import-path apps/space/proto -import-path apps/task/proto
    -proto proto/common/v1/common.proto -proto proto/internal/v1/internal.proto
    -proto apps/base/proto/base/v1/base_open.proto -proto apps/base/proto/base/v1/base_sys_user.proto
    -proto apps/dict/proto/dict/v1/dict.proto
    -proto apps/space/proto/space/v1/space.proto
    -proto apps/task/proto/task/v1/task.proto)

  local PASS=0 FAIL=0

  # 登录
  local CAP CID B64 CODE TOKEN
  CAP=$(grpcurl -plaintext "${P[@]}" -d '{}' $GW base.v1.BaseOpenService/Captcha 2>&1)
  CID=$(echo "$CAP" | grep captchaId | sed 's/.*: "\(.*\)".*/\1/')
  B64=$(echo "$CAP" | grep '"data": "data:image' | sed 's/.*base64,//;s/"//')
  CODE=$(echo "$B64" | base64 -d | sed -n 's/.*fill="#fff">\([0-9]*\)<.*/\1/p')
  TOKEN=$(grpcurl -plaintext "${P[@]}" \
    -d "{\"username\":\"admin\",\"password\":\"123456\",\"captchaId\":\"$CID\",\"verifyCode\":\"$CODE\"}" \
    $GW base.v1.BaseOpenService/Login | grep '"token":' | head -1 | sed 's/.*: "\(.*\)".*/\1/')

  if [ "${#TOKEN}" -ge 16 ]; then
    ok "登录成功 token=${TOKEN:0:13}..."
    PASS=$((PASS+1))
  else
    err "登录失败"
    FAIL=$((FAIL+1))
  fi

  local H="authorization: Bearer $TOKEN"
  for t in \
    "base.v1.BaseOpenService/PersonInfo|{}" \
    "base.v1.BaseSysUserService/Page|{\"page\":1,\"size\":1}" \
    "dict.v1.DictInfoService/Data|{\"types\":[\"type\"]}" \
    "space.v1.SpaceInfoService/Page|{\"page\":1,\"size\":1}" \
    "task.v1.TaskInfoService/Page|{\"page\":1,\"size\":1}"; do
    local rpc="${t%%|*}" body="${t##*|}"
    local r
    r=$(grpcurl -plaintext "${P[@]}" -H "$H" -d "$body" $GW "$rpc" 2>&1)
    if echo "$r" | grep -q '"code": 1000'; then
      ok "$rpc"
      PASS=$((PASS+1))
    else
      err "$rpc"
      FAIL=$((FAIL+1))
    fi
  done

  # 鉴权拦截
  local NT
  NT=$(grpcurl -plaintext "${P[@]}" -d '{}' $GW base.v1.BaseOpenService/PersonInfo 2>&1 || true)
  if echo "$NT" | grep -qiE "unauthenticated|缺少|失效"; then
    ok "无 token 拦截"
    PASS=$((PASS+1))
  else
    err "无 token 未拦截: $(echo "$NT" | head -2 | tr '\n' ' ')"
    FAIL=$((FAIL+1))
  fi

  echo
  log "冒烟结果: PASS=$PASS FAIL=$FAIL"
  [ "$FAIL" -eq 0 ] && ok "全部通过 🎉" || err "存在失败"
  return $FAIL
}

# ---------- 停止 ----------
down() {
  log "停止微服务"
  pkill -f "target/debug/.*-service" 2>/dev/null || true
  log "停止容器（保留 MySQL/Redis 数据卷）"
  docker rm -f "$APISIX" "$ETCD" 2>/dev/null || true
  ok "已停止（MySQL/Redis 保留）"
}

# ---------- main ----------
case "${1:-all}" in
  up)     start_infra; build; start_services ;;
  smoke)  smoke ;;
  down)   down ;;
  all|*)  start_infra; build; start_services; smoke ;;
esac
