#!/bin/bash
set -e

# 用法：
#   ./run_in_container.sh [--work_dir 目录] "命令"
#
# 示例：
#   ./run_in_container.sh --work_dir project/code "ls -la"
#   ./run_in_container.sh "echo Hello, world!"

# 未指定时，工作目录默认为 WORKSPACE_ROOT_DIR。
WORK_DIR="${WORKSPACE_ROOT_DIR:-$(pwd)}"
# 默认允许域名，可通过 OPENAI_ALLOWED_DOMAINS 覆盖
OPENAI_ALLOWED_DOMAINS="${OPENAI_ALLOWED_DOMAINS:-api.openai.com}"

# 解析可选参数。
if [ "$1" = "--work_dir" ]; then
  if [ -z "$2" ]; then
    echo "错误：指定了 --work_dir 但未提供目录。"
    exit 1
  fi
  WORK_DIR="$2"
  shift 2
fi

WORK_DIR=$(realpath "$WORK_DIR")

# 根据规范化的工作目录生成唯一容器名
CONTAINER_NAME="codex_$(echo "$WORK_DIR" | sed 's/\//_/g' | sed 's/[^a-zA-Z0-9_-]//g')"

# 定义清理逻辑，脚本退出时删除容器，避免残留
cleanup() {
  docker rm -f "$CONTAINER_NAME" >/dev/null 2>&1 || true
}
# 捕获 EXIT，无论如何退出都执行清理
trap cleanup EXIT

# 确保传入了命令。
if [ "$#" -eq 0 ]; then
  echo "用法：$0 [--work_dir 目录] \"命令\""
  exit 1
fi

# 检查 WORK_DIR 是否已设置。
if [ -z "$WORK_DIR" ]; then
  echo "错误：未提供工作目录且未设置 WORKSPACE_ROOT_DIR。"
  exit 1
fi

# 校验 OPENAI_ALLOWED_DOMAINS 不能为空
if [ -z "$OPENAI_ALLOWED_DOMAINS" ]; then
  echo "错误：OPENAI_ALLOWED_DOMAINS 为空。"
  exit 1
fi

# 使用 cleanup() 清理该工作目录已有容器，统一删除逻辑。
cleanup

# 运行容器，并将指定目录以相同路径挂载到容器内。
docker run --name "$CONTAINER_NAME" -d \
  -e OPENAI_API_KEY \
  --cap-add=NET_ADMIN \
  --cap-add=NET_RAW \
  -v "$WORK_DIR:/app$WORK_DIR" \
  codex \
  sleep infinity

# 将允许的域名写入容器内文件
docker exec --user root "$CONTAINER_NAME" bash -c "mkdir -p /etc/codex"
for domain in $OPENAI_ALLOWED_DOMAINS; do
  # 校验域名格式，避免注入
  if [[ ! "$domain" =~ ^[a-zA-Z0-9][a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$ ]]; then
    echo "错误：域名格式无效：$domain"
    exit 1
  fi
  echo "$domain" | docker exec --user root -i "$CONTAINER_NAME" bash -c "cat >> /etc/codex/allowed_domains.txt"
done

# 设置域名文件权限
docker exec --user root "$CONTAINER_NAME" bash -c "chmod 444 /etc/codex/allowed_domains.txt && chown root:root /etc/codex/allowed_domains.txt"

# 以 root 身份在容器内初始化防火墙
docker exec --user root "$CONTAINER_NAME" bash -c "/usr/local/bin/init_firewall.sh"

# 执行后移除防火墙脚本
docker exec --user root "$CONTAINER_NAME" bash -c "rm -f /usr/local/bin/init_firewall.sh"

# 在容器内执行命令，确保在工作目录下运行。
# 使用参数化 bash 命令安全处理命令与目录。

quoted_args=""
for arg in "$@"; do
  quoted_args+=" $(printf '%q' "$arg")"
done
docker exec -it "$CONTAINER_NAME" bash -c "cd \"/app$WORK_DIR\" && codex --full-auto ${quoted_args}"
