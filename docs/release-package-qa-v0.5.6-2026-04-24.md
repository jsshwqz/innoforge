# v0.5.6 发布包真实验收记录

日期：2026-04-24

## 验收对象

- Release：`https://github.com/jsshwqz/innoforge/releases/tag/v0.5.6`
- 资产：`innoforge-windows-x86_64.zip`
- 本地验收目录：`target/release-qa/v0.5.6/pkg2`
- 包内启动文件：`innoforge.exe`

## 发布包启动结论

- 默认 `0.0.0.0:3000` 在当前机器返回 `os error 10013`，不是进程占用，而是系统端口/地址权限限制。
- 设置 `INNOFORGE_PORT=3921` 后，发布包可正常启动并返回首页。
- 该问题已作为 v0.5.7 修复项处理：默认端口不可用时自动尝试备用端口；用户显式指定端口时仍严格失败。

## 页面与接口可见化验收

输出目录：`docs/visible-e2e-runs/release-v0.5.6-package-20260424-162425`

- API：20/20 通过。
- 页面：`/`、`/search`、`/idea`、`/compare`、`/settings`、`/ai`、`/patent/smoke-pat-001` 均可访问。
- 按钮：触发 45 个，失败 0 个，跳过 31 个。
- 截图：97 张。
- 控制台错误：0。

跳过项主要是不可见 tab、动态 modal 按钮或禁用态按钮，不计为运行失败。

## 定向验收

输出目录：`docs/visible-e2e-runs/release-v0.5.6-targeted-20260424-163013`

- 发布包可读取本机 `.env` 中的搜索与 AI 配置。
- 创意历史删除 API 验证通过：删除后列表不再出现，详情接口返回 not found。
- AI 深度输出验证通过：同一提示下返回约 870 个中文字符，包含研发可用性、专利风险、实验验证三个维度。
- 重复公开号入库验证：两个公开号变体入库后，数据库保留 1 条，说明入库去重有效。

## 发现的问题

- 混合搜索输入公开号时，v0.5.6 可能按 `Mixed` 路径处理，导致公开号型查询查不到本地记录。
- 在线搜索相关性门槛过宽，`mobile phone`、`Folder type celluar phone` 等泛手机结果可能混入具体技术查询。
- Google Patents free 回退分支未复用主 SerpAPI 分支的相关性过滤规则。

以上问题已进入 v0.5.7 修复。
