#!/bin/bash
set -euo pipefail  # 出错、未定义变量、管道失败时退出
IFS=$'\n\t'       # 更严格的单词分割

# 从文件读取允许的域名
ALLOWED_DOMAINS_FILE="/etc/codex/allowed_domains.txt"
if [ -f "$ALLOWED_DOMAINS_FILE" ]; then
    ALLOWED_DOMAINS=()
    while IFS= read -r domain; do
        ALLOWED_DOMAINS+=("$domain")
    done < "$ALLOWED_DOMAINS_FILE"
    echo "使用文件中的域名：${ALLOWED_DOMAINS[*]}"
else
    # 回退到默认域名
    ALLOWED_DOMAINS=("api.openai.com")
    echo "未找到域名文件，使用默认值：${ALLOWED_DOMAINS[*]}"
fi

# 确保至少有一个域名
if [ ${#ALLOWED_DOMAINS[@]} -eq 0 ]; then
  echo "错误：未指定允许的域名"
    exit 1
fi

# 清空现有规则并删除现有 ipset
iptables -F
iptables -X
iptables -t nat -F
iptables -t nat -X
iptables -t mangle -F
iptables -t mangle -X
ipset destroy allowed-domains 2>/dev/null || true

# 在限制前先允许 DNS 与本地回环
# 允许出站 DNS
iptables -A OUTPUT -p udp --dport 53 -j ACCEPT
# 允许入站 DNS 响应
iptables -A INPUT -p udp --sport 53 -j ACCEPT
# 允许本地回环
iptables -A INPUT -i lo -j ACCEPT
iptables -A OUTPUT -o lo -j ACCEPT

# 创建支持 CIDR 的 ipset
ipset create allowed-domains hash:net

# 解析并添加其他允许的域名
for domain in "${ALLOWED_DOMAINS[@]}"; do
  echo "解析域名 $domain..."
    ips=$(dig +short A "$domain")
    if [ -z "$ips" ]; then
    echo "错误：解析 $domain 失败"
        exit 1
    fi

    while read -r ip; do
        if [[ ! "$ip" =~ ^[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}$ ]]; then
        echo "错误：DNS 返回的 IP 无效：$domain -> $ip"
            exit 1
        fi
    echo "为 $domain 添加 IP：$ip"
        ipset add allowed-domains "$ip"
    done < <(echo "$ips")
done

# 从默认路由获取宿主机 IP
HOST_IP=$(ip route | grep default | cut -d" " -f3)
if [ -z "$HOST_IP" ]; then
  echo "错误：无法检测宿主机 IP"
    exit 1
fi

HOST_NETWORK=$(echo "$HOST_IP" | sed "s/\.[0-9]*$/.0\/24/")
echo "检测到宿主机网段：$HOST_NETWORK"

# 设置剩余 iptables 规则
iptables -A INPUT -s "$HOST_NETWORK" -j ACCEPT
iptables -A OUTPUT -d "$HOST_NETWORK" -j ACCEPT

# 先将默认策略设为 DROP
iptables -P INPUT DROP
iptables -P FORWARD DROP
iptables -P OUTPUT DROP

# 先允许已建立的连接，保证已放行流量可继续
iptables -A INPUT -m state --state ESTABLISHED,RELATED -j ACCEPT
iptables -A OUTPUT -m state --state ESTABLISHED,RELATED -j ACCEPT

# 仅允许到允许域名的出站流量
iptables -A OUTPUT -m set --match-set allowed-domains dst -j ACCEPT

# 追加最终 REJECT 规则，确保快速返回错误
# TCP 发送 reset；UDP 发送 ICMP 端口不可达。
iptables -A INPUT -p tcp -j REJECT --reject-with tcp-reset
iptables -A INPUT -p udp -j REJECT --reject-with icmp-port-unreachable
iptables -A OUTPUT -p tcp -j REJECT --reject-with tcp-reset
iptables -A OUTPUT -p udp -j REJECT --reject-with icmp-port-unreachable
iptables -A FORWARD -p tcp -j REJECT --reject-with tcp-reset
iptables -A FORWARD -p udp -j REJECT --reject-with icmp-port-unreachable

echo "防火墙配置完成"
echo "正在验证防火墙规则..."
if curl --connect-timeout 5 https://example.com >/dev/null 2>&1; then
    echo "错误：防火墙验证失败 - 仍可访问 https://example.com"
    exit 1
else
    echo "防火墙验证通过 - 预期无法访问 https://example.com"
fi

# 始终验证 OpenAI API 可访问
if ! curl --connect-timeout 5 https://api.openai.com >/dev/null 2>&1; then
    echo "错误：防火墙验证失败 - 无法访问 https://api.openai.com"
    exit 1
else
    echo "防火墙验证通过 - 预期可访问 https://api.openai.com"
fi
