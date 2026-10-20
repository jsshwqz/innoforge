#!/usr/bin/env bash
# setup_rust.sh — 华为临时容器（6h 重置后）一键装配 Rust 开发环境 + innoforge 基线
# 用法（容器内单行）：bash /root/sync/setup_rust.sh
# 全部走国内镜像 / codeload，避免容器里 git clone 与 GitHub 直连超时。
# v2 变更（2026-10-21，前两轮实测教训）：
#   1) 自愈 IPv6-only DNS 陷阱：新容器可能只返回 AAAA 且无 v6 出口，所有外网请求静默挂死
#   2) 基线改用 codeload 按 **具体 sha** 下 tar 包 + git init（容器里 git clone github.com 实测必挂）
#   3) 基线 sha 落盘 /root/sync/BASELINE，指挥方据此核对 diff 起点，不再靠猜
set -uo pipefail
export RUSTUP_HOME="$HOME/.rustup" CARGO_HOME="$HOME/.cargo"
export RUSTUP_DIST_SERVER="https://rsproxy.cn" RUSTUP_UPDATE_ROOT="https://rsproxy.cn/rustup"
export PATH="$CARGO_HOME/bin:$PATH"
LOG=/root/sync/rust_setup.log
mkdir -p /root/sync/inbox /root/workspace
t0=$(date +%s)
{
  echo "=== setup_rust v2 start $(date) ==="
  # 0) 强制 IPv4 优先（幂等）
  if ! grep -q 'precedence ::ffff:0:0/96 100' /etc/gai.conf 2>/dev/null; then
    echo 'precedence ::ffff:0:0/96 100' >> /etc/gai.conf
    echo "[0/4] gai.conf: forced IPv4 precedence"
  else
    echo "[0/4] gai.conf: IPv4 precedence already set"
  fi
  echo "--- connectivity probe ---"
  curl -4 -s -m 25 -o /dev/null -w "raw_github:%{http_code}\n" \
    https://raw.githubusercontent.com/jsshwqz/innoforge/main/README.md || echo "raw_github:FAIL"

  # 1) 装 rustup（已存在则跳过）
  if [ ! -x "$CARGO_HOME/bin/cargo" ]; then
    echo "[1/4] installing rustup via rsproxy ..."
    curl -4 -sSf --retry 3 --max-time 180 https://rsproxy.cn/rustup-init.sh -o /tmp/rustup-init.sh \
      && sh /tmp/rustup-init.sh -y --default-toolchain stable --profile minimal 2>&1 | tail -5
  else
    echo "[1/4] cargo already present, skip rustup"
  fi
  cargo --version 2>&1; rustc --version 2>&1

  # 2) cargo 走 rsproxy sparse 索引
  echo "[2/4] configuring cargo mirror ..."
  mkdir -p "$CARGO_HOME"
  cat > "$CARGO_HOME/config.toml" <<'CFG'
[source.crates-io]
replace-with = 'rsproxy-sparse'
[source.rsproxy-sparse]
registry = "sparse+https://rsproxy.cn/index/"
[registries.rsproxy]
index = "https://rsproxy.cn/crates.io-index"
[net]
git-fetch-with-cli = true
CFG

  # 3) 建基线：按 sha 下 codeload tar 包 + git init
  if [ -d /root/innoforge/.git ]; then
    echo "[3/4] repo exists, baseline=$(cat /root/sync/BASELINE 2>/dev/null) — skip"
  else
    echo "[3/4] building baseline from codeload (git clone unreliable in-container) ..."
    SHA=$(curl -4 -s -m 30 https://api.github.com/repos/jsshwqz/innoforge/commits/main \
            | grep -m1 '"sha"' | cut -d'"' -f4)
    if [ -z "$SHA" ]; then
      echo "[3/4] FATAL: cannot resolve main sha from api.github.com"; exit 1
    fi
    echo "    main sha = $SHA"
    curl -4 -sSL --retry 3 --max-time 600 \
      "https://codeload.github.com/jsshwqz/innoforge/tar.gz/$SHA" -o /root/sync/main.tgz
    ls -l /root/sync/main.tgz
    echo "    main.tgz md5 = $(md5sum /root/sync/main.tgz | cut -d' ' -f1)"
    mkdir -p /root/innoforge
    tar xzf /root/sync/main.tgz -C /root/innoforge --strip-components=1
    git -C /root/innoforge init -q
    git -C /root/innoforge config user.name jsshwqz
    git -C /root/innoforge config user.email jsshwqz@users.noreply.github.com
    # 注意：此处 add -A 只把「刚下载的 pristine main 快照」烤成根提交（基线本身），不是工作提交。
    # GLM 后续提交仍必须逐文件 git add，禁 git add -A。
    git -C /root/innoforge add -A
    git -C /root/innoforge commit -qm "baseline main $SHA (codeload tarball)"
    echo "$SHA" > /root/sync/BASELINE
    echo "    baseline root commit = $(git -C /root/innoforge rev-parse HEAD)"
  fi

  # 4) 依赖 + 编译计时（这才是每次重置的真实代价）
  echo "[4/4] cargo fetch + check (timed) ..."
  cd /root/innoforge || exit 1
  cargo fetch 2>&1 | tail -3
  start=$(date +%s)
  cargo check --all-targets 2>&1 | tail -8
  rc=$?
  end=$(date +%s)
  echo "=== DONE rc=$rc cold_check_secs=$((end-start)) total_setup_secs=$((end-t0)) $(date) ==="
} > "$LOG" 2>&1
echo "SETUP-EXIT $?" >> "$LOG"
