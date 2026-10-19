#!/usr/bin/env bash
# setup_rust.sh — 在华为临时容器里自动装配 Rust 开发环境并克隆 innoforge
# 每次 6 小时重置后由看门狗重建分支自动执行。全部走国内镜像，避免直连慢/超时。
set -uo pipefail
export RUSTUP_HOME="$HOME/.rustup" CARGO_HOME="$HOME/.cargo"
export RUSTUP_DIST_SERVER="https://rsproxy.cn" RUSTUP_UPDATE_ROOT="https://rsproxy.cn/rustup"
LOG=/root/sync/rust_setup.log
t0=$(date +%s)
{
  echo "=== setup_rust start $(date) ==="
  # 1) 装 rustup（若已存在则跳过）
  if [ ! -x "$CARGO_HOME/bin/cargo" ]; then
    echo "[1/4] installing rustup via rsproxy ..."
    curl -sSf --retry 3 --max-time 120 https://rsproxy.cn/rustup-init.sh -o /tmp/rustup-init.sh \
      && sh /tmp/rustup-init.sh -y --default-toolchain stable --profile minimal 2>&1 | tail -5
  else
    echo "[1/4] cargo already present, skip rustup"
  fi
  export PATH="$CARGO_HOME/bin:$PATH"
  cargo --version 2>&1; rustc --version 2>&1

  # 2) 配 cargo 走 rsproxy sparse 索引（crates 拉取加速）
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

  # 3) 克隆 innoforge（源码真相源=GitHub，public 免认证读）
  echo "[3/4] cloning innoforge ..."
  if [ ! -d /root/innoforge/.git ]; then
    git clone --depth 1 https://github.com/jsshwqz/innoforge.git /root/innoforge 2>&1 | tail -3
  else
    echo "  repo exists, fetching ..."
    git -C /root/innoforge fetch --depth 1 origin 2>&1 | tail -2
  fi
  git -C /root/innoforge log --oneline -1 2>&1

  # 4) 依赖拉取 + cargo check 计时（这才是每次重置的真实代价）
  echo "[4/4] cargo fetch + check (timed) ..."
  cd /root/innoforge
  /usr/bin/time -v cargo fetch 2>&1 | grep -E "time|Elapsed|error" | tail -3 || cargo fetch 2>&1 | tail -3
  start=$(date +%s)
  cargo check --all-targets 2>&1 | tail -8
  end=$(date +%s)
  echo "=== DONE rc=$? cold_check_secs=$((end-start)) total_setup_secs=$((end-t0)) $(date) ==="
} > "$LOG" 2>&1
echo "SETUP-EXIT $?" >> "$LOG"
