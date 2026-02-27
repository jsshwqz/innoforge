# Patent Hub 项目上下文文档

**创建日期**: 2026 年 2 月 27 日  
**目的**: 记录项目完整上下文，供未来参考

---

## 📋 项目概述

**Patent Hub** 是一个基于 Rust 的专利检索与分析系统，支持：
- 全球专利搜索（通过 SerpAPI + Google Patents）
- AI 智能分析（支持智谱 GLM、OpenAI 等）
- 专利对比
- 数据导出

**技术栈**: Rust + Axum + SQLite + 原生 HTML/JS

---

## 🎯 设计要求与实现状态

### 核心功能清单

| 功能 | 设计要求 | 实现状态 | 说明 |
|------|----------|----------|------|
| **在线专利搜索** | SerpAPI + Google Patents | ✅ 完成 | `/api/search/online` |
| **本地数据库搜索** | SQLite FTS5 | ✅ 完成 | `/api/search` |
| **搜索历史** | 自动保存最近 10 条 | ✅ 完成 | 前端 localStorage |
| **高级筛选** | 日期、国家 | ✅ 完成 | 前端 + 后端支持 |
| **排序功能** | 相关度/最新/最早 | ✅ 完成 | `sort_by` 参数 |
| **统计分析** | 申请人/国家/趋势图 | ✅ 完成 | `/api/search/stats` |
| **导出 Excel** | CSV 格式 | ✅ 完成 | `/api/search/export` |
| **AI 智能分析** | OpenAI 兼容接口 | ✅ 完成 | `/api/ai/chat` 等 |
| **专利对比** | AI 对比两个专利 | ✅ 完成 | `/api/ai/compare` |
| **相似推荐** | 基于关键词 | ✅ 完成 | `/api/patent/similar/:id` |
| **文件对比** | 上传 TXT 对比 | ✅ 完成 | `/api/upload/compare` |
| **设置管理** | SerpAPI/AI 配置 | ✅ 完成 | Settings API |
| **Timestamp** | CSS 缓存破坏 | ✅ 完成 | `style.css?v={{timestamp}}` |

---

## 🔧 关键技术决策

### 1. 环境变量管理

**问题**: `dotenvy::dotenv()` 不会覆盖已存在的环境变量

**解决方案**: 使用 `dotenv_override()` 强制覆盖

```rust
// src/main.rs
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load .env file and override any existing environment variables
    let _ = dotenvy::dotenv_override();
    tracing_subscriber::fmt::init();
    // ...
}
```

**原因**: Windows 系统环境变量可能被之前的进程设置，需要确保 `.env` 文件优先。

---

### 2. API 密钥脱敏

**安全要求**: API 密钥不能以明文暴露在浏览器 DevTools 或日志中

**实现**:
```rust
fn mask_api_key(key: &str) -> String {
    if key.is_empty() || key == "your-serpapi-key-here" {
        String::new()
    } else if key.len() <= 8 {
        "****".to_string()
    } else {
        format!("{}****{}", &key[..4], &key[key.len()-4..])
    }
}
```

**返回格式**:
```json
{
  "serpapi_key": "test****2345",
  "serpapi_key_configured": true,
  "ai_api_key": "d985****f7cd",
  "ai_api_key_configured": true
}
```

---

### 3. 输入验证

**SerpAPI Key 验证**:
- 长度：20-200 字符
- 字符：仅允许字母数字、连字符、下划线

**AI 配置验证**:
- Base URL: 必须是有效的 HTTP/HTTPS URL
- API Key: 长度 10-200 字符
- 模型名称：长度 2-100 字符，允许字母数字、连字符、下划线、点号

---

### 4. 文件锁机制

**问题**: 并发写入 `.env` 文件可能导致损坏

**解决方案**: 使用 `fs2` crate 添加独占锁

```rust
fn update_env_file(key: &str, value: &str) -> Result<(), String> {
    use fs2::FileExt;
    use std::fs::OpenOptions;
    use std::io::{Read, Write, Seek, SeekFrom};

    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(env_path)?;

    // 文件独占锁
    file.lock_exclusive()?;
    // 读取、修改、写入、解锁...
}
```

---

### 5. Timestamp 缓存破坏

**问题**: 浏览器缓存 CSS/JS 文件，更新后用户无法立即看到新版本

**解决方案**: 在 URL 中添加动态时间戳

```rust
// src/routes.rs
pub async fn search_page() -> Html<String> {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let html = include_str!("../templates/search.html")
        .replace("{{timestamp}}", &timestamp.to_string());
    Html(html)
}
```

```html
<!-- templates/search.html -->
<link rel="stylesheet" href="/static/style.css?v={{timestamp}}">
```

**效果**: CSS URL 变为 `style.css?v=1772202584`，浏览器视为新资源

---

### 6. 前端 API 端点修复

**问题**: 前端调用 `/api/search/local`，但后端没有这个端点

**解决方案**: 修改前端调用 `/api/search`

```javascript
// templates/search.html
const endpoint = mode === 'online' ? '/api/search/online' : '/api/search';
```

---

## 📁 关键文件结构

```
patent-hub-backup/
├── src/
│   ├── main.rs           # 应用入口，路由注册
│   ├── routes.rs         # API 端点实现
│   ├── db.rs             # 数据库操作
│   ├── ai.rs             # AI 客户端
│   └── patent.rs         # 数据模型
├── templates/
│   ├── search.html       # 搜索页面（含 timestamp）
│   ├── index.html        # 首页
│   ├── compare.html      # 对比页面
│   ├── ai.html           # AI 助手页面
│   ├── settings.html     # 设置页面
│   └── patent_detail.html # 专利详情页
├── static/
│   └── style.css         # 样式文件
├── .env                  # 环境变量配置
├── Cargo.toml            # 依赖配置
└── patent_hub.db         # SQLite 数据库
```

---

## 🔑 依赖项 (Cargo.toml)

```toml
[dependencies]
axum = { version = "0.6", features = ["multipart"] }
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
reqwest = { version = "0.11", features = ["json", "rustls-tls"] }
rusqlite = { version = "0.29", features = ["bundled"] }
tower-http = { version = "0.4", features = ["fs", "cors"] }
dotenvy = "0.15"
fs2 = "0.4"           # 文件锁
url = "2.5"           # URL 验证
# ... 其他依赖
```

---

## ⚙️ 配置说明

### .env 文件格式

```env
# SerpAPI Key（Google Patents 搜索）
SERPAPI_KEY=你的 serpapi 密钥

# AI 配置（智谱 GLM - 推荐国内用户）
AI_BASE_URL=https://open.bigmodel.cn/api/paas/v4
AI_API_KEY=你的智谱密钥
AI_MODEL=glm-4-flash

# 其他可选 AI 配置：
# OpenAI:
# AI_BASE_URL=https://api.openai.com/v1
# AI_API_KEY=sk-...
# AI_MODEL=gpt-4o

# DeepSeek:
# AI_BASE_URL=https://api.deepseek.com/v1
# AI_API_KEY=...
# AI_MODEL=deepseek-chat

# Ollama (本地):
# AI_BASE_URL=http://localhost:11434/v1
# AI_API_KEY=ollama
# AI_MODEL=qwen2.5:7b
```

---

## 🧪 测试验证

### 1. 编译测试
```bash
cd patent-hub-backup
cargo build --release
# 预期：编译成功，仅有少量警告
```

### 2. 启动测试
```bash
./target/release/patent-hub.exe
# 预期：服务器启动，监听 0.0.0.0:3000
```

### 3. API 测试
```powershell
# 测试设置 API（验证脱敏）
Invoke-RestMethod http://127.0.0.1:3000/api/settings

# 测试在线搜索（验证 SerpAPI）
$body = @{query='artificial intelligence';page=1;page_size=3} | ConvertTo-Json
Invoke-WebRequest -Uri 'http://127.0.0.1:3000/api/search/online' `
  -Method POST -ContentType 'application/json' -Body $body

# 预期：source="serpapi", total>0
```

### 4. Timestamp 测试
```powershell
$r = Invoke-WebRequest -Uri 'http://127.0.0.1:3000/search' -UseBasicParsing
if ($r.Content -match 'style\.css\?v=(\d{10})') {
    Write-Host "✅ Timestamp 正常：$($matches[1])"
} else {
    Write-Host "❌ Timestamp 未找到"
}
```

### 5. 输入验证测试
```powershell
# 测试非法 URL（应返回错误）
$body = @{base_url='invalid';api_key='short';model='x'} | ConvertTo-Json
Invoke-WebRequest -Uri 'http://127.0.0.1:3000/api/settings/ai' `
  -Method POST -ContentType 'application/json' -Body $body

# 预期：{"status":"error","message":"Invalid URL format..."}
```

---

## 🐛 已知问题与解决方案

### 问题 1: SerpAPI 调用返回本地结果

**症状**: `/api/search/online` 返回 `source: "local"` 而非 `source: "serpapi"`

**原因**: 
1. 系统环境变量 `SERPAPI_KEY` 被设置为测试值
2. `dotenvy::dotenv()` 不覆盖已有环境变量

**解决**: 使用 `dotenv_override()` 并重启服务器

---

### 问题 2: 前端调用不存在的 API 端点

**症状**: 本地搜索返回 404

**原因**: 前端调用 `/api/search/local`，但后端只有 `/api/search`

**解决**: 修改前端代码调用 `/api/search`

---

### 问题 3: Timestamp 功能无效

**症状**: CSS URL 没有时间戳参数

**原因**: 模板中没有 `{{timestamp}}` 占位符

**解决**: 在 `templates/search.html` 添加：
```html
<link rel="stylesheet" href="/static/style.css?v={{timestamp}}">
```

---

## 📊 API 端点清单

### 页面路由
| 端点 | 方法 | 说明 |
|------|------|------|
| `/` | GET | 首页 |
| `/search` | GET | 搜索页面 |
| `/compare` | GET | 对比页面 |
| `/ai` | GET | AI 助手页面 |
| `/settings` | GET | 设置页面 |
| `/patent/:id` | GET | 专利详情页 |

### 搜索 API
| 端点 | 方法 | 说明 |
|------|------|------|
| `/api/search` | POST | 本地搜索 |
| `/api/search/online` | POST | 在线搜索（SerpAPI） |
| `/api/search/stats` | POST | 统计信息 |
| `/api/search/export` | POST | 导出 CSV |

### AI API
| 端点 | 方法 | 说明 |
|------|------|------|
| `/api/ai/chat` | POST | AI 聊天 |
| `/api/ai/summarize` | POST | AI 摘要 |
| `/api/ai/compare` | POST | AI 对比 |
| `/api/search/analyze` | POST | AI 分析搜索结果 |

### 设置 API
| 端点 | 方法 | 说明 |
|------|------|------|
| `/api/settings` | GET | 获取配置（脱敏） |
| `/api/settings/serpapi` | POST | 保存 SerpAPI 密钥 |
| `/api/settings/ai` | POST | 保存 AI 配置 |

### 专利 API
| 端点 | 方法 | 说明 |
|------|------|------|
| `/api/patent/fetch` | POST | 获取专利详情 |
| `/api/patent/enrich/:id` | GET | 丰富专利信息 |
| `/api/patent/similar/:id` | GET | 相似推荐 |
| `/api/upload/compare` | POST | 上传文件对比 |

---

## 🚀 部署指南

### 1. 编译
```bash
cargo build --release
```

### 2. 配置
```bash
cp .env.example .env
# 编辑 .env 填入 API 密钥
```

### 3. 启动
```bash
./target/release/patent-hub.exe
```

### 4. 访问
- 本地：http://127.0.0.1:3000
- 手机：http://[本机 IP]:3000

---

## 📝 开发注意事项

### 1. 环境变量读取顺序
1. 系统环境变量（最高优先级）
2. `.env` 文件（使用 `dotenv_override()` 覆盖）
3. 默认值

### 2. 文件锁使用场景
- 写入 `.env` 文件时必须加锁
- 防止并发写入导致文件损坏

### 3. Timestamp 使用场景
- 静态资源 URL（CSS/JS）需要缓存破坏时
- 仅在 `search_page()` 中使用
- 模板中必须有 `{{timestamp}}` 占位符

### 4. API 密钥处理
- 永远不要以明文返回完整密钥
- 使用 `mask_api_key()` 脱敏
- 日志中不要打印完整密钥

---

## 🔗 相关文档

- `README.md` - 项目说明
- `SECURITY_FIXES.md` - 安全修复详情
- `VERIFICATION_REPORT.md` - 验证报告
- `TIMESTAMP_FIX_REQUIRED.md` - Timestamp 修复指南
- `docs/` - 用户文档目录

---

## 📅 重要日期

| 日期 | 事件 |
|------|------|
| 2026-02-24 | 项目初始版本 (v0.1.0) |
| 2026-02-25 | 安全修复、排序功能 |
| 2026-02-27 | 完整修复（SerpAPI、Timestamp、设置按钮） |

---

**文档版本**: 1.0  
**最后更新**: 2026 年 2 月 27 日  
**维护者**: Patent Hub Team  

---

## 📚 相关文档

- [DOCS_INDEX.md](DOCS_INDEX.md) - 完整文档导航
- [.kiro/agent.md](.kiro/agent.md) - AI 助手阅读指南
- [DOCUMENTATION_SYSTEM.md](DOCUMENTATION_SYSTEM.md) - 文档系统说明
- [FUTURE_PROJECTS.md](FUTURE_PROJECTS.md) - 未来项目规划（DocNav 独立项目）
