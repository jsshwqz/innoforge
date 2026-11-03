## 变更描述 / Description

请简要说明本次 PR 的变更动机、涉及范围及主要改动：
Briefly describe the motivation, scope, and key changes of this PR:

- 

## 变更类型 / Type of Change

- [ ] 🐛 Bug 修复 / Bug fix
- [ ] ✨ 新功能 / New feature
- [ ] ♻️ 代码重构 / Refactoring (无功能变化 / No functional change)
- [ ] 📝 文档更新 / Documentation update
- [ ] 🧪 测试用例 / Test cases
- [ ] ⚡ 性能优化 / Performance improvement
- [ ] 🔧 构建或辅助工具变更 / Build or tooling changes

## 关联 Issue / Related Issues

Closes #
Fixes #

## 自查清单 / Verification Checklist

提交前请确认已完成以下检查：
Please ensure the following checks have passed before submitting:

- [ ] **格式与代码规范**：已运行并通过 `cargo fmt --check`
- [ ] **静态检查**：已运行并通过 `cargo clippy -- -D warnings`（零警告）
- [ ] **自动化测试**：已运行并通过 `cargo test` 全量测试
- [ ] **前端完整性**：若涉及 `templates/`，已运行 `node check_html_functions.mjs`
- [ ] **文档履约**：若改动涉及逻辑/架构，已在 `docs/plans/STATUS.md` 或 `docs/errors.md` 中记录
- [ ] **隐私与数据安全**：已核验代码和提交记录中**不包含**任何未脱敏的真实个人姓名、真实在审案号或私有密钥
