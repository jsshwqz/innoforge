# 门禁速查卡（执行 agent 每任务收尾必跑）

> 全绿才算完成。顺序执行，fmt 失败先修格式再跑后续；clippy 失败不必跑 test。

## Windows PowerShell（本机开发环境）

```powershell
# 0) 磁盘水位（<5GB 先 cargo clean —— target 曾占 8.4GB 压垮 D 盘）
Get-PSDrive D

# 1) 格式
cargo fmt --check

# 2) 静态检查（零警告）
cargo clippy --all-targets -- -D warnings

# 3) 测试（当前基线 369 通过 0 失败 1 忽略；只增不减）
cargo test

# 4) 改动 templates/ 下任何 HTML 后必跑（编译前拦截"按钮在函数没了"）
node check_html_functions.mjs
#    若新增了页面级函数定义：先 node check_html_functions.mjs --refresh 更新基线，
#    并把基线变更与模板变更放同一提交、在提交说明中给出理由

# 5) 真实浏览器回归（54+ 项；需本机已装 Puppeteer 与 Chromium）
$env:PATH = "C:\Users\Administrator\AppData\Local\ms-playwright-go\1.57.0;C:\Users\Administrator\AppData\Roaming\npm;$env:PATH"
node e2e_test.mjs

# 6) JS 语法检查（i18n.js 是唯一有 ESLint 配置的文件）
npx eslint static/i18n.js
```

## Git Bash 形态（AGENTS.md 原文写法，等价）

```bash
export PATH="/c/Users/Administrator/AppData/Local/ms-playwright-go/1.57.0:/c/Users/Administrator/AppData/Roaming/npm:$PATH"
cd /d/test/patent-hub-backup && node e2e_test.mjs
```

## 远端关卡（T0.6 之后自动生效）

push 到 GitHub/Gitee 的 main 或 dev 触发双镜像 CI 三 job：`test`(fmt/clippy/test) → `lint`(HTML 扫描+ESLint) → `e2e`(Chromium)。**本地全绿 ≠ 远端绿**：对齐最新提交前不要引用任何历史 CI 结论。

## 提交规范

- 格式：`feat|fix|refactor|chore|docs: 中文简要描述`
- 一个任务一个 commit，完成后秒级提交；**禁止 `git add -A`**，只 add 本任务触碰的文件
- pre-commit 钩子（core.hooksPath=.githooks）自动跑 HTML 函数扫描——被拦即修，禁 --no-verify
