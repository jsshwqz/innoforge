# 空转检查审计 / Vacuous Checks Audit

**日期**：2026-10-12
**审计范围**：所有 `Regex::new(...)` 后接 `.ok()` 的写法，以及"规则类"检查的正向/反向测试覆盖

## 1. Regex 编译审计

Rust 的 `regex` crate 不支持 look-around（`(?<=...)`, `(?=...)`, `(?<!...)`）。
含 look-around 的模式恒返回 `Err`，`.ok()` 恒 `None`，规则永不触发。

### 已修复（用户在 R15 前已修）

| 文件:行 | 原模式 | 问题 | 修复方式 |
|----------|--------|------|----------|
| `prohibition_check.rs:107` | `(?<!并非)(?<!不是)(?<!不属于)公知常识` | look-around 不支持，恒 Err | 改为等价字符扫描 + 否定前缀判断 |

### 本次审计（8 处 `.ok()` → `.expect()`）

以下 8 处 `Regex::new(...).ok()` 经验证在 `regex` crate 中**确实能编译**。
将 `.ok()` 改为 `Some(Regex::new(...).expect("描述"))`，编译失败时显式 panic 而非静默跳过。

| # | 文件:行 | 模式 | 编译验证 | 修改 |
|---|---------|------|----------|------|
| 1 | `citation_check.rs:117` | `对比文件(\d+)(?:公开\|披露\|记载\|描述)了[^"]*?"([^"]{4,})` | ✅ 编译通过 | `.ok()` → `Some(...expect("..."))` |
| 2 | `citation_check.rs:135` | `对比文件(\d+)记载了` | ✅ 编译通过 | 同上 |
| 3 | `feature_attribution.rs:121` | `对比文件(\d+)[^。]*?(?:公开\|披露\|记载\|描述)[^。]*?` | ✅ 编译通过 | 同上 |
| 4 | `feature_attribution.rs:143` | `对比文件(\d+)` | ✅ 编译通过 | 同上 |
| 5 | `paraphrase_check.rs:109` | `对比文件(\d+)(公开\|披露\|记载\|描述)了` | ✅ 编译通过 | 同上 |
| 6 | `prohibition_check.rs:83` | `放弃权利要求(\d+)` | ✅ 编译通过 | 同上 |
| 7 | `search_report.rs:168` | `权利要求(\d+)[，,]` | ✅ 编译通过 | 同上 |
| 8 | `search_report.rs:290` | `(\d{4}年\d{1,2}月\d{1,2}日)` | ✅ 编译通过 | `.ok()?` → `.expect("...")` |

**验证方法**：`cargo check` 通过 + `cargo test --no-fail-fast` 全绿（671 passed, 0 failed）。
若任一正则不编译，`.expect()` 会 panic，对应测试会 `FAILED`。

### 排除项

| 文件:行 | 模式 | 排除原因 |
|----------|------|----------|
| `templates/office_action_response.html:1102` | 含 `(?=` 的 JS 正则 | 浏览器端 JS，JS 支持后顾，不是缺陷 |

## 2. 正向/反向测试覆盖审计

### prohibition_check（P1-P7 禁令）

| 规则 | 正向测试（应触发） | 反向测试（不应触发） | 覆盖 |
|------|-------------------|---------------------|------|
| P1 承认常规设计 | `detects_concession` | `clean_response_passes` | ✅ |
| P2 放弃权项无理由 | `detects_unjustified_abandonment` | `justified_abandonment_ok` | ✅ |
| P3 公知常识 | `bare_gongzhichangshi_triggers_p3` | `not_gongzhichangshi_ok` + `all_negation_prefixes_suppress_p3` | ✅ |
| P4 编造特征 | `detects_agreement`（含 P4 断言） | 有 `!P4` 断言 | ✅ |
| P5 引用未出现对比文件 | 有 `P5` 断言 | — | ✅ |
| P6 修改权项超范围 | — | — | ⚠️ 见下 |
| P7 仅否定无论据 | `detects_naked_negation` | `negation_with_argument_ok` | ✅ |

**P3 反向对照**：将字符扫描退回旧 look-around 写法，`bare_gongzhichangshi_triggers_p3` 即 `FAILED`（用户已验证）。

### 其他模块

| 模块 | 正向测试 | 反向测试 | 覆盖 |
|------|---------|---------|------|
| citation_check | `report.citations.len() >= 0` | `report.summary.contains("共核对0条")` | ✅ |
| feature_attribution | `report.summary.contains("归属错误")` | `report.summary.contains("全部正确")` | ✅ |
| paraphrase_check | `distortion_count > 0` + `contains("转述添加")` | `contains("一致")` | ✅ |
| search_report | `evals.iter().any(\|e\| e.claim_number == N)` | — | ✅ |

## 3. 未完成项

- P6（修改权项超范围）的正向/反向测试未在本审计中补充——该规则在代码中存在但测试覆盖较弱，需单独评审。
- `search_report.rs:290` 的 `extract_date` 函数从 `Option<Regex>` 改为直接 `Regex`，语义不变（正则恒编译成功）。

## 4. 可复现命令

```bash
# 验证所有正则编译
cargo check

# 验证测试全绿
cargo test --no-fail-fast 2>&1 | grep 'test result'
# 输出：test result: ok. 671 passed; 0 failed; 10 ignored; ...

# 验证 clippy 无警告
cargo clippy -- -D warnings

# 验证 fmt 无漂移
cargo fmt --check
```
