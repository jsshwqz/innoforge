# 贡献指南 / Contributing Guide

感谢你对创研台 InnoForge 的关注！

## 如何贡献

### 报告问题
- 在 [GitHub Issues](https://github.com/jsshwqz/innoforge/issues) 提交
- 请包含：操作系统、Rust 版本、复现步骤、错误日志

### 提交代码

1. Fork 本仓库
2. 创建功能分支：`git checkout -b feature/your-feature`
3. 确保代码通过检查：
   ```bash
   cargo fmt --check
   cargo clippy
   cargo test
   ```
4. 提交更改并推送
5. 发起 Pull Request

### 发布门禁（强制）

- 发布前必须执行人工实测清单：`docs/release-gate-manual-e2e.md`
- 发布前可执行自动核验脚本：`scripts/release_verify.ps1`
- 若任一功能项或资产核验失败，不得宣布“发布成功”
- 发布后必须补复盘文档（问题、修复、复验结论）

### 代码规范

- 使用 `cargo fmt` 格式化代码
- 使用 `cargo clippy` 检查代码质量
- 所有面向用户的文本：中文在前，英文在后
- 提交信息使用中文，格式：`feat/fix/refactor: 简要描述`

### 开发者与 AI 协作规范

* **人类贡献者指南**：遵循本文件（`CONTRIBUTING.md`）的标准 GitHub 流程。
* **AI 代理开发规约**：本项目采用多 Agent / AI 协同工程化流程。所有参与编码的 AI Agent 必须在开工前读取并严格遵守 [`AGENTS.md`](AGENTS.md) 中的工程纪律、边界守卫与质量门禁。
* **文档中心**：完整架构与 API 契约文档参见 [`docs/README.md`](docs/README.md)。

### 项目结构

```
src/           # Rust 核心代码（后端 + API + 状态机流水线）
templates/     # 服务端渲染 HTML 页面模板
static/        # 前端静态资源（CSS、JS、双语翻译）
tests/         # 集成测试与守卫测试
scripts/       # 构建、代码生成与契约验证工具脚本
docs/          # 项目文档中心（详见 docs/README.md）
```

### 关联仓库

桌面端（Tauri）/ iOS / 鸿蒙壳均为规划中的独立仓库，目前尚未创建；本仓库通过 `src/lib.rs` 的 C ABI 导出供其内嵌。

## 许可证

贡献的代码将采用 [MIT](LICENSE) 许可证。
