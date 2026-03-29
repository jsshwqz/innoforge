# 贡献指南 / Contributing Guide

感谢你对 Patent Hub 的关注！

## 如何贡献

### 报告问题
- 在 [GitHub Issues](https://github.com/jsshwqz/patent-hub/issues) 或 [Gitee Issues](https://gitee.com/jsshwqz/patent-hub/issues) 提交
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

### 代码规范

- 使用 `cargo fmt` 格式化代码
- 使用 `cargo clippy` 检查代码质量
- 所有面向用户的文本：中文在前，英文在后
- 提交信息使用中文，格式：`feat/fix/refactor: 简要描述`

### 项目结构

```
src/           # Rust 核心代码（后端 + API）
mobile/        # Dioxus 移动端（Rust UI）
templates/     # HTML 页面模板
static/        # 静态资源（CSS、JS）
tests/         # 集成测试
docs/          # 文档
```

### 关联仓库

- [patent-hub-desktop](https://gitee.com/jsshwqz/patent-hub-desktop) -- Tauri 桌面端
- [patent-hub-ios](https://gitee.com/jsshwqz/patent-hub-ios) -- iOS 原生壳
- [patent-hub-harmony](https://gitee.com/jsshwqz/patent-hub-harmony) -- 鸿蒙原生壳

## 许可证

贡献的代码将采用 [MIT](LICENSE) 许可证。
