#!/usr/bin/env bash
# M2-E 回归脚本（2026-10-24）
# 用途：对 r33/m2-top3 构建的可执行做一次端到端 OA 链路回归并产出可复核指标。
#
# 端点说明（如实记录）：任务书写的 /api/ai/oa-analysis 路由不存在；
# 本脚本使用真实的 SSE 分析端点 /api/ai/office-action-response/stream（与前端同一路径）。
#
# 夹具说明：data/presubmit_simulated_fail.txt 由 tests/test_presubmit_r7.rs 用
# 合成（SIMULATED_FAIL）输入生成，不含任何案件数据；缺失时本脚本先跑该测试生成，
# 绝不硬编码案件号/姓名。
#
# 幂等性：启动前清理本脚本上次留下的 server（pidfile）；run-* 按<b>时间戳归档</b>不清空
# （保留历史供复核），输出文件均写到当次 run 目录，重跑互不覆盖。
# 端口被外部进程占用时明确报错退出。
#
# AI 通道：优先 env（AI_BASE_URL/AI_API_KEY），否则读本机 8080 网关配置的 key
# （只读取，不打印、不写入仓库/日志）。AI 不可用时脚本照常产出报告（分数为 0
# 并标注 AI_UNAVAILABLE），不伪装成功。
#
# 产出（/tmp/m2-regression/）：
#   output.txt         — SSE 原始响应完整落盘（任务书指定路径）
#   report.md          — 汇总：wall_time / fact_check_score / anchor_rate / quote_count / error_events
#   run-<ts>/sse_analysis.txt   — 原始 SSE 流（同 output.txt，按次归档）
#   run-<ts>/analysis_decoded.md— 解码后分析文本
#   run-<ts>/metrics.json       — 机器可读指标（含 wall_time_s、error_events）
#   server.log         — 服务日志

set -u

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT_BASE="${M2_REG_DIR:-/tmp/m2-regression}"
PORT="${M2_REG_PORT:-3000}"
TS="$(date +%Y%m%d-%H%M%S)"
RUN="$OUT_BASE/run-$TS"
LOG="$OUT_BASE/server.log"

mkdir -p "$RUN"

# ── 0. 幂等清理：上一轮的 server（本脚本 pidfile 管理的） ──────────────
PIDFILE="$OUT_BASE/server.pid"
if [ -f "$PIDFILE" ]; then
  OLD_PID="$(cat "$PIDFILE" 2>/dev/null || true)"
  if [ -n "${OLD_PID:-}" ] && kill -0 "$OLD_PID" 2>/dev/null; then
    echo "[cleanup] 停止上轮 server pid=$OLD_PID"
    kill "$OLD_PID" 2>/dev/null || true
    sleep 1
  fi
  rm -f "$PIDFILE"
fi

# ── 1. 构建 release ───────────────────────────────────────────────────
echo "[build] cargo build --release（增量，幂等）"
( cd "$ROOT" && cargo build --release --bin innoforge-server ) > "$RUN/build.log" 2>&1
if [ $? -ne 0 ]; then
  echo "[build] 失败，日志：$RUN/build.log"; tail -20 "$RUN/build.log"; exit 1
fi
BIN="$ROOT/target/release/innoforge-server"
[ -x "$BIN" ] || BIN="$ROOT/target/release/innoforge-server.exe"
if [ ! -x "$BIN" ]; then
  echo "[build] 未找到可执行文件"; exit 1
fi
echo "[build] OK: $BIN"

# ── 2. 夹具（存在则复用；缺失则用既有测试生成——合成数据） ─────────────
# test_presubmit_r7 以包根为 CWD 写 data/uploads/（gitignored 工作目录）
FIXTURE="$ROOT/data/uploads/presubmit_simulated_fail.txt"
if [ ! -s "$FIXTURE" ]; then
  echo "[fixture] 缺失，用 tests/test_presubmit_r7.rs 生成（合成输入）"
  ( cd "$ROOT" && cargo test --test test_presubmit_r7 ) > "$RUN/fixture_gen.log" 2>&1
  if [ ! -s "$FIXTURE" ]; then
    echo "[fixture] 生成失败，日志：$RUN/fixture_gen.log"; tail -10 "$RUN/fixture_gen.log"; exit 1
  fi
fi
cp "$FIXTURE" "$RUN/fixture_input.txt"
echo "[fixture] OK（$(wc -c < "$FIXTURE") bytes）"

# ── 3. AI 通道（不打印 key） ──────────────────────────────────────────
AI_BASE_URL="${AI_BASE_URL:-http://127.0.0.1:8080/v1}"
AI_MODEL="${AI_MODEL:-claude-opus-4-6-thinking}"
if [ -z "${AI_API_KEY:-}" ] && [ -f "$HOME/.antigravity_tools/gui_config.json" ]; then
  AI_API_KEY="$(node -e "try{const c=require(process.env.HOME+'/.antigravity_tools/gui_config.json');process.stdout.write((c.proxy&&c.proxy.api_key)||'')}catch(e){}" 2>/dev/null || true)"
fi
AI_MODE="ok"
if [ -z "$AI_API_KEY" ]; then
  AI_MODE="AI_UNAVAILABLE（未提供 key，指标将按失败链路如实记录）"
fi
echo "[ai] base=$AI_BASE_URL model=$AI_MODEL mode=$AI_MODE"

# ── 4. 启动 server（隔离 home + 指定超时） ────────────────────────────
HOME_DIR="$OUT_BASE/home"
rm -rf "$HOME_DIR"; mkdir -p "$HOME_DIR"
: > "$LOG"

# 注意：CWD 切到隔离 home——仓库根的 .env 会被 dotenvy::dotenv_override() 无条件
# 覆盖进程 env（实测：失效的 DeepSeek key 盖掉了网关配置）；换 CWD 后 .env 不参与，
# AI 配置真正来自上方 env/网关。
(
  cd "$HOME_DIR" || exit 1
  # exec：子 shell 直接替换为 server 进程，$! 记录的才是可 kill 的真实 PID
  exec env INNOFORGE_HOME="$HOME_DIR" \
    INNOFORGE_PORT="$PORT" \
    INNOFORGE_NO_OPEN=1 \
    INNOFORGE_AI_HTTP_TIMEOUT_SECS=300 \
    INNOFORGE_AI_GLOBAL_TIMEOUT_SECS=300 \
    AI_BASE_URL="$AI_BASE_URL" \
    AI_MODEL="$AI_MODEL" \
    AI_MODEL_EXPERT="${AI_MODEL_EXPERT:-$AI_MODEL}" \
    AI_API_KEY="$AI_API_KEY" \
    "$BIN" >> "$LOG" 2>&1
) &
SERVER_PID=$!
echo "$SERVER_PID" > "$PIDFILE"
trap 'kill "$SERVER_PID" 2>/dev/null || true' EXIT

echo "[server] 等待就绪 port=$PORT pid=$SERVER_PID"
READY=0
for _ in $(seq 1 60); do
  CODE="$(curl -s -o /dev/null -w "%{http_code}" --max-time 3 "http://127.0.0.1:$PORT/api/version" || true)"
  [ "$CODE" = "200" ] && READY=1 && break
  if ! kill -0 "$SERVER_PID" 2>/dev/null; then
    echo "[server] 进程退出，日志尾部："; tail -20 "$LOG"; exit 1
  fi
  sleep 1
done
if [ "$READY" != "1" ]; then echo "[server] 60s 未就绪"; tail -20 "$LOG"; exit 1; fi
curl -s "http://127.0.0.1:$PORT/api/version" > "$RUN/version.json"
echo "[server] OK $(head -c 160 "$RUN/version.json")"

# ── 5. 提交创意（夹具作为描述） ────────────────────────────────────────
node - "$PORT" "$RUN" <<'NODE'
const fs = require('fs');
const [port, run] = process.argv.slice(2);
const fixture = fs.readFileSync(`${run}/fixture_input.txt`, 'utf8');
const body = {
  title: `M2 regression ${new Date().toISOString()}`,
  description: fixture,
  input_type: 'text',
};
fs.writeFileSync(`${run}/submit_body.json`, JSON.stringify(body));
NODE
SUBMIT_METRICS="$(curl -s -o "$RUN/submit_resp.json" -w "%{http_code} %{time_total}" \
  -X POST -H "Content-Type: application/json" \
  -d @"$RUN/submit_body.json" \
  "http://127.0.0.1:$PORT/api/idea/submit")"
echo "[submit] http/time: $SUBMIT_METRICS"
IDEA_ID="$(node -e "try{const j=require(process.argv[1]);process.stdout.write(j.id||j.idea_id||'')}catch(e){}" "$RUN/submit_resp.json")"
echo "[submit] idea_id=${IDEA_ID:-<none>}"

# ── 6. OA 分析（真实 SSE 端点，捕获到文件） ───────────────────────────
node - "$RUN" <<'NODE'
const fs = require('fs');
const [run] = process.argv.slice(2);
const fixture = fs.readFileSync(`${run}/fixture_input.txt`, 'utf8');
const body = {
  my_patent: {
    title: 'M2 regression synthetic',
    content: fixture,
  },
  office_action: { content: fixture },
  references: [],
  oa_type: 'first_exam',
  depth: 'standard',
};
fs.writeFileSync(`${run}/analysis_body.json`, JSON.stringify(body));
NODE
SSE_OUT="$RUN/sse_analysis.txt"
STREAM_METRICS="$(curl -sN -o "$SSE_OUT" -w "%{http_code} %{time_total}" --max-time 600 \
  -X POST -H "Content-Type: application/json" \
  -d @"$RUN/analysis_body.json" \
  "http://127.0.0.1:$PORT/api/ai/office-action-response/stream")"
echo "[stream] http/time: $STREAM_METRICS"
# 任务书指定路径：完整 SSE 响应落盘 /tmp/m2-regression/output.txt
cp "$SSE_OUT" "$OUT_BASE/output.txt"

# ── 7. 解码 + fact_check 分 + anchor_rate + quote_count ───────────────
node - "$SSE_OUT" "$RUN" "$AI_MODE" "$STREAM_METRICS" <<'NODE' | tee "$RUN/metrics_stdout.txt"
const fs = require('fs');
const [ssePath, run, aiMode, streamMetrics] = process.argv.slice(2);
const raw = fs.readFileSync(ssePath, 'utf8');
let text = '', hasError = false, errorMsg = '', errorEvents = 0;
for (const line of raw.split('\n')) {
  if (line.startsWith('event:')) { if (line.slice(6).trim() === 'error') { hasError = true; errorEvents++; } continue; }
  if (!line.startsWith('data:')) continue;
  const d = line.slice(5).replace(/^ /, '');
  if (d === '[DONE]') continue;
  if (d.startsWith('"')) { try { text += JSON.parse(d); } catch { text += d; } continue; }
  if (d.startsWith('[ERROR]')) { hasError = true; errorMsg = d; continue; }
}
fs.writeFileSync(`${run}/analysis_decoded.md`, text);

// fact_check 分：分析流内嵌「AI 事实核查」段（M2-B 保证空文本显式 0 分）
const m = text.match(/可信度评分：(\d+)\/100/);
const factScore = m ? Number(m[1]) : (text.trim() ? null : 0);

// anchor_rate：事实句（含断言模式）中带锚点/专利号形态的比例；quote_count：中文引号对数
const sents = (text.match(/[^。！？\n]+[。！？]?/g) || []).map(s => s.trim()).filter(s => s.length > 7);
const factPat = /公开了|记载了|披露了|区别在于|特征在于|相比|属于|包括|实现/;
const anchorPat = /第\s*\[?0?\d{1,4}\]?\s*段|权利要求\s*\d+|\[00\d{2}\]|\bD\d\b|对比文件/;
const factSents = sents.filter(s => factPat.test(s));
const anchored = factSents.filter(s => anchorPat.test(s)).length;
const quoteCount = (text.match(/“[^”]{1,120}”/g) || []).length;

const wt = Number((streamMetrics || '').trim().split(/\s+/)[1]);
const metrics = {
  ai_mode: aiMode,
  wall_time_s: Number.isFinite(wt) && wt > 0 ? +wt.toFixed(2) : null,
  stream_error: hasError ? errorMsg || true : false,
  error_events: errorEvents,
  decoded_chars: text.length,
  newlines: (text.match(/\n/g) || []).length,
  fact_check_score: factScore,
  fact_sents: factSents.length,
  anchored_sents: anchored,
  anchor_rate: factSents.length ? +(anchored / factSents.length).toFixed(2) : null,
  quote_count: quoteCount,
};
fs.writeFileSync(`${run}/metrics.json`, JSON.stringify(metrics, null, 2));
console.log(JSON.stringify(metrics, null, 2));
NODE

# ── 8. 汇总报告 ───────────────────────────────────────────────────────
SUBMIT_HTTP="${SUBMIT_METRICS%% *}"
SUBMIT_TIME="${SUBMIT_METRICS#* }"
STREAM_HTTP="${STREAM_METRICS%% *}"
cat > "$OUT_BASE/report.md" <<EOF
# M2-E 回归报告（$TS）

- 分支/构建：$(git -C "$ROOT" rev-parse --abbrev-ref HEAD) @ $(git -C "$ROOT" rev-parse --short HEAD)（release）
- 端口：$PORT（隔离 home：$HOME_DIR）
- AI 通道：$AI_BASE_URL / $AI_MODEL — $AI_MODE
- 提交 /api/idea/submit：HTTP $SUBMIT_HTTP，${SUBMIT_TIME}s（idea_id=${IDEA_ID:-无}）
- 分析流 /api/ai/office-action-response/stream：HTTP $STREAM_HTTP
- 五项指标（详见 $RUN/metrics.json）：
  - wall_time（分析流墙钟）：$(echo "$STREAM_METRICS" | awk '{print $2}')s
  - fact_check_score：$(node -e "try{const j=require(process.argv[1]);console.log(j.fact_check_score)}catch(e){console.log('N/A')}" "$RUN/metrics.json")
  - anchor_rate：$(node -e "try{const j=require(process.argv[1]);console.log(j.anchor_rate)}catch(e){console.log('N/A')}" "$RUN/metrics.json")
  - quote_count：$(node -e "try{const j=require(process.argv[1]);console.log(j.quote_count)}catch(e){console.log('N/A')}" "$RUN/metrics.json")
  - error_events：$(node -e "try{const j=require(process.argv[1]);console.log(j.error_events)}catch(e){console.log('N/A')}" "$RUN/metrics.json")
- 产物：$OUT_BASE/output.txt（完整 SSE 响应）、$RUN/analysis_decoded.md、服务日志 $LOG
EOF
echo "[done] 报告：$OUT_BASE/report.md"
tail -40 "$RUN/metrics_stdout.txt" 2>/dev/null || true
