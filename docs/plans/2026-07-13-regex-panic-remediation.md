# 2026-07-13 正则初始化 panic 加固计划

## 背景

项目规约禁止生产路径使用 `unwrap()` 或 `expect()`。创意报告行内 Markdown、专利说明书 HTML 清理与法律状态页面清理仍将静态 `Regex::new` 直接接 `expect()`。尽管当前字面量有效，后续维护错误会把一次请求变成 panic；同时，收拢 HTML 转义职责时必须保持用户文本始终转义。

## 范围

仅修改 `src/routes/idea.rs` 与 `src/routes/patent.rs`。

- 不新增 crate、API 路由、数据库迁移或前端改动。
- 保持正常 Markdown、说明书清理和法律状态查询的既有成功语义。
- 保留正则缓存，初始化异常不得向用户暴露内部细节。

## 实施步骤

1. 将行内 Markdown 的三个正则改为缓存 `Result<Regex, regex::Error>`；失败时记录日志并返回已转义文本。
2. 将专利说明书 HTML 清理和 Sogou 解析的缓存正则改为受控 `Result` 分支，分别返回友好 JSON 或既有降级链可处理的错误。
3. 将 Markdown 的 HTML 转义集中到函数入口，并将调用点改为传递原始文本，确保既不双重转义也不输出原始 HTML。
4. 添加 Markdown 标签渲染与 `<script>` 转义回归测试。

## 验收与归档

- `cargo fmt --check`
- `cargo clippy -- -D warnings`
- `cargo test`
- 构建正式二进制，并验证 `/`、`/idea`、`/oa-response` 页面可访问。

## 完成记录 / Completion

- **状态 / Status**: ✅ 已完成 / Completed
- **代码提交 / Code commit**: `26e20b2` (`fix: 消除正则初始化 panic`)
- **结果 / Result**: 五处生产正则初始化均不再 panic；Markdown 在正常格式渲染与危险 HTML 输入下都保持安全；法律状态查询保留既有失败降级语义。
- **验证 / Verification**: `cargo fmt --check`、`cargo clippy -- -D warnings`、`cargo test`（299 passed, 1 ignored）、正式二进制构建和三页 HTTP 200 回归均通过。
