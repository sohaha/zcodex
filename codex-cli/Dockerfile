FROM node:24-slim

ARG TZ
ENV TZ="$TZ"

# 安装基础开发工具、证书与 iptables/ipset，并清理 apt 缓存以缩小镜像体积
RUN apt-get update && apt-get install -y --no-install-recommends \
  aggregate \
  ca-certificates \
  curl \
  dnsutils \
  fzf \
  gh \
  git \
  gnupg2 \
  iproute2 \
  ipset \
  iptables \
  jq \
  less \
  man-db \
  procps \
  unzip \
  ripgrep \
  zsh \
  && rm -rf /var/lib/apt/lists/*

# 确保默认 node 用户可访问 /usr/local/share
RUN mkdir -p /usr/local/share/npm-global && \
  chown -R node:node /usr/local/share

ARG USERNAME=node

# 切换到非 root 用户
USER node

# 安装全局包
ENV NPM_CONFIG_PREFIX=/usr/local/share/npm-global
ENV PATH=$PATH:/usr/local/share/npm-global/bin

# 安装 codex
COPY dist/codex.tgz codex.tgz
RUN npm install -g codex.tgz \
  && npm cache clean --force \
  && rm -rf /usr/local/share/npm-global/lib/node_modules/codex-cli/node_modules/.cache \
  && rm -rf /usr/local/share/npm-global/lib/node_modules/codex-cli/tests \
  && rm -rf /usr/local/share/npm-global/lib/node_modules/codex-cli/docs

# 容器内默认已有足够隔离，因此允许 Codex CLI 在无额外沙箱的情况下运行。
ENV CODEX_UNSAFE_ALLOW_NO_SANDBOX=1

# 以 root 身份复制并设置防火墙脚本。
USER root
COPY scripts/init_firewall.sh /usr/local/bin/
RUN chmod 500 /usr/local/bin/init_firewall.sh

# 切回非 root 用户。
USER node
