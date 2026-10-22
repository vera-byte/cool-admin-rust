#!/usr/bin/env bash
# 构建全部微服务 Docker 镜像
# 用法: ./scripts/build-images.sh [tag] [registry]
#   ./scripts/build-images.sh 0.1.0
#   ./scripts/build-images.sh 0.1.0 registry.example.com/cool
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

TAG="${1:-0.1.0}"
REGISTRY="${2:-}"
PLATFORMS="${PLATFORMS:-}"  # 留空=本机架构；可设 linux/amd64,linux/arm64

declare -A SERVICES=(
  ["base-service"]="app-base"
  ["dict-service"]="app-dict"
  ["space-service"]="app-space"
  ["task-service"]="app-task"
)

log() { echo -e "\033[36m[build]\033[0m $*"; }

# 国内镜像覆盖（网络受限时）
# export BUILD_IMAGE=swr.cn-north-4.myhuaweicloud.com/.../rust:1.85-bookworm
# export RUNTIME_IMAGE=swr.cn-north-4.myhuaweicloud.com/.../debian:bookworm-slim
BUILD_ARGS=""
[ -n "${BUILD_IMAGE:-}" ] && BUILD_ARGS="$BUILD_ARGS --build-arg BUILD_IMAGE=$BUILD_IMAGE"
[ -n "${RUNTIME_IMAGE:-}" ] && BUILD_ARGS="$BUILD_ARGS --build-arg RUNTIME_IMAGE=$RUNTIME_IMAGE"

for SVC in "${!SERVICES[@]}"; do
  PKG="${SERVICES[$SVC]}"
  IMAGE="${REGISTRY:+$REGISTRY/}cool/${SVC}:${TAG}"
  log "构建 ${IMAGE} (${PKG})"
  if [ -n "$PLATFORMS" ]; then
    docker buildx build --platform "$PLATFORMS" \
      -f deploy/Dockerfile.service \
      --build-arg SERVICE="$SVC" --build-arg PACKAGE="$PKG" \
      -t "$IMAGE" --push .
  else
    docker build -f deploy/Dockerfile.service \
      --build-arg SERVICE="$SVC" --build-arg PACKAGE="$PKG" $BUILD_ARGS \
      -t "$IMAGE" .
  fi
  ok "✅ ${IMAGE}"
done

log "构建完成"
docker images | grep "cool/" | head -10
