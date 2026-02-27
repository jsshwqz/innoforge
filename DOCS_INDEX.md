# 📚 Patent Hub 文档导航

**重要提示**: 这是你阅读本项目文档的**起点**！

---

## 🚀 快速开始（按顺序阅读）

### 第一次接触本项目 → 按此顺序阅读

| 顺序 | 文档 | 目的 | 阅读时间 |
|------|------|------|----------|
| **1** | [`PROJECT_CONTEXT.md`](PROJECT_CONTEXT.md) | **完整上下文** - 技术决策、设计要求、实现状态 | 15 分钟 |
| **2** | [`README.md`](README.md) | 项目介绍、快速上手 | 10 分钟 |
| **3** | [`docs/QUICK_START.md`](docs/QUICK_START.md) | 5 分钟上手指南 | 5 分钟 |
| **4** | [`docs/INSTALL.md`](docs/INSTALL.md) | 安装与配置详解 | 10 分钟 |

---

## 🤖 AI 助手入口

如果你是通过 AI 助手（Kiro、Qwen Code 等）访问本项目：

📄 **AI 助手会自动读取**: [`.kiro/agent.md`](.kiro/agent.md)

该文件包含：
- 首次接触项目的阅读顺序
- 根据问题类型选择文档的指南
- 文档优先级说明

---

## 📖 文档分类索引

### 🔰 核心文档（必读）

| 文档 | 说明 | 最后更新 |
|------|------|----------|
| [PROJECT_CONTEXT.md](PROJECT_CONTEXT.md) | ⭐ **项目完整上下文** - 技术决策、设计要求、API 清单 | 2026-02-27 |
| [README.md](README.md) | 项目介绍、功能特性、使用指南 | 2026-02-24 |
| [CHANGELOG.md](CHANGELOG.md) | 版本变更历史 | 2026-02-24 |

### 🛠️ 开发文档

| 文档 | 说明 | 最后更新 |
|------|------|----------|
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | 架构设计、模块说明 | 2026-02-24 |
| [docs/API.md](docs/API.md) | API 接口文档 | 2026-02-24 |
| [SECURITY_FIXES.md](SECURITY_FIXES.md) | 安全修复详情 | 2026-02-25 |
| [VERIFICATION_REPORT.md](VERIFICATION_REPORT.md) | 验证测试报告 | 2026-02-27 |

### 📦 修复报告（按日期排序）

| 文档 | 说明 | 日期 |
|------|------|------|
| [API_TEST_SUMMARY.md](API_TEST_SUMMARY.md) | API 测试摘要 | 2026-02-25 |
| [API_TEST_REPORT_TIMESTAMP.md](API_TEST_REPORT_TIMESTAMP.md) | Timestamp 功能测试 | 2026-02-25 |
| [TIMESTAMP_FIX_REQUIRED.md](TIMESTAMP_FIX_REQUIRED.md) | Timestamp 修复指南 | 2026-02-25 |
| [SORT_FEATURE_COMPLETE.md](SORT_FEATURE_COMPLETE.md) | 排序功能完成报告 | 2026-02-25 |
| [SORT_FEATURE_TEST_REPORT.md](SORT_FEATURE_TEST_REPORT.md) | 排序功能测试报告 | 2026-02-25 |
| [SETTINGS_API_SUMMARY.md](SETTINGS_API_SUMMARY.md) | Settings API 摘要 | 2026-02-25 |

### 📱 用户文档

| 文档 | 说明 | 最后更新 |
|------|------|----------|
| [docs/QUICK_START.md](docs/QUICK_START.md) | 快速开始指南 | 2026-02-24 |
| [docs/INSTALL.md](docs/INSTALL.md) | 安装指南 | 2026-02-24 |
| [docs/MOBILE_ACCESS.md](docs/MOBILE_ACCESS.md) | 移动设备访问 | 2026-02-24 |
| [docs/国内用户指南.md](docs/国内用户指南.md) | 国内用户使用指南 | 2026-02-24 |

### 🧪 测试文档

| 文档 | 说明 | 最后更新 |
|------|------|----------|
| [API_TEST_RESULTS.md](API_TEST_RESULTS.md) | API 测试结果 | 2026-02-25 |
| [API_TEST_REPORT.md](API_TEST_REPORT.md) | API 测试报告 | 2026-02-25 |

### 🔧 工具与脚本

| 文件 | 说明 |
|------|------|
| [test-api.ps1](test-api.ps1) | API 测试脚本 |
| [test-api-complete.ps1](test-api-complete.ps1) | 完整 API 测试套件 |
| [test-sort-feature.ps1](test-sort-feature.ps1) | 排序功能测试 |
| [test-settings-api.ps1](test-settings-api.ps1) | Settings API 测试 |
| [run-postman-tests.ps1](run-postman-tests.ps1) | Postman 测试运行器 |

---

## 🎯 按场景阅读

### 场景 1: 新接手项目
```
1. PROJECT_CONTEXT.md    ← 从这里开始！
2. README.md
3. docs/QUICK_START.md
4. docs/ARCHITECTURE.md
```

### 场景 2: 修复 Bug
```
1. PROJECT_CONTEXT.md    ← 先了解上下文
2. docs/API.md          ← 查看相关 API
3. VERIFICATION_REPORT.md ← 参考测试方法
```

### 场景 3: 添加新功能
```
1. PROJECT_CONTEXT.md    ← 了解现有设计
2. docs/ARCHITECTURE.md ← 理解架构
3. SECURITY_FIXES.md    ← 遵循安全规范
```

### 场景 4: 部署上线
```
1. docs/INSTALL.md
2. docs/QUICK_START.md
3. VERIFICATION_REPORT.md ← 验证清单
```

### 场景 5: 国内用户使用
```
1. docs/国内用户指南.md
2. README.md
3. docs/QUICK_START.md
```

---

## 📌 关键文档说明

### ⭐ PROJECT_CONTEXT.md
**为什么重要**: 
- 记录了所有技术决策和原因
- 包含完整的功能清单和实现状态
- 记录了已知问题和解决方案
- 未来维护者的第一站

**何时阅读**:
- ✅ 第一次接触项目
- ✅ 准备修改代码
- ✅ 需要理解设计意图

---

### 📖 README.md
**为什么重要**:
- 项目门面，快速了解项目
- 功能介绍和使用指南
- 包含常见问题解答

**何时阅读**:
- ✅ 初步了解项目
- ✅ 查找使用方法

---

### 🔒 SECURITY_FIXES.md
**为什么重要**:
- 记录了所有安全修复
- 包含输入验证规则
- 安全编码指南

**何时阅读**:
- ✅ 修改设置相关代码
- ✅ 添加新的 API 端点

---

### ✅ VERIFICATION_REPORT.md
**为什么重要**:
- 完整的测试验证报告
- 包含测试命令和预期结果
- 可用于回归测试

**何时阅读**:
- ✅ 修改代码后验证
- ✅ 准备发布新版本

---

## 🔄 文档更新指南

### 何时更新此索引
- 添加新文档时
- 删除旧文档时
- 文档重要性发生变化时

### 如何更新
1. 在对应分类中添加/删除文档链接
2. 更新"按场景阅读"指南（如需要）
3. 更新"关键文档说明"（如需要）

---

## 📞 找不到文档？

如果找不到需要的信息：

1. **先查** `PROJECT_CONTEXT.md` - 最完整的上下文
2. **再查** `docs/` 目录 - 用户文档
3. **搜索** 仓库中的 `.md` 文件
4. **查看** `CHANGELOG.md` - 版本历史

---

## 🎯 文档维护原则

1. **PROJECT_CONTEXT.md** 必须保持最新
2. 所有技术决策必须记录在案
3. 修复报告必须包含日期和测试结果
4. 删除旧文档前先更新此索引

---

**最后更新**: 2026 年 2 月 27 日  
**维护者**: Patent Hub Team  
**文档版本**: 1.0
