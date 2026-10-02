# 一、三家现状覆盖分析

## 1.1 逐家覆盖情况

| 维度 | OpenAI | Anthropic Claude | Google Gemini |
|------|--------|------------------|---------------|
| **API 方式** | ✅ 原生 OpenAI 格式，已支持 | ⚠️ 需确认兼容端点 | ✅ OpenAI 兼容端点，已支持 |
| **OAuth 登录** | ❌ 未实现 | ❌ 未实现 | ✅ 已实现（gcloud CLI） |
| **订阅/Cookie 代理** | ❌ 未实现 | ❌ 未实现 | ⚠️ OAuth 基本覆盖 |
| **在 .env.example 中** | ❌ 未预设 | ❌ 未预设 | ✅ 已在 FALLBACK 中预设 |

## 1.2 三家 API 格式对比（2026 年现状）

### OpenAI —— 原生 OpenAI 格式（InnoForge 已支持）

```
端点：POST https://api.openai.com/v1/chat/completions
认证：Authorization: Bearer sk-xxx
格式：标准 OpenAI chat completions
模型：gpt-4o, gpt-4o-mini, gpt-4.1, o3, o3-mini
专家模型：o3, o3-mini（推理模型，慢但强）
```

InnoForge 的 `src/ai/client.rs` 本就是 OpenAI 兼容客户端，**直接可用，只需加预设**。

### Anthropic Claude —— 2026 年已推出 OpenAI 兼容端点

```
# 原生 Anthropic 格式（不同于 OpenAI）
端点：POST https://api.anthropic.com/v1/messages
认证：x-api-key: sk-ant-xxx + anthropic-version: 2023-06-01
格式：Anthropic Messages API（与 OpenAI 有差异）

# OpenAI 兼容端点（2025 年后推出，关键！）
端点：POST https://api.anthropic.com/v1/chat/completions
认证：Authorization: Bearer sk-ant-xxx
格式：OpenAI chat completions 兼容
模型：claude-sonnet-4-6, claude-haiku-4-5, claude-opus-4
专家模型：claude-opus-4 或 claude-sonnet-4-6
```

**关键发现**：Anthropic 官方发布说明确认已推出 OpenAI 兼容 API 端点：
> "We've launched an OpenAI-compatible API endpoint, allowing you to test Claude models
> by changing just your API key, base URL, and model name in existing OpenAI integrations."

这意味着 **InnoForge 可以直接通过现有架构接入 Claude，无需写格式适配器**。

### Google Gemini —— OpenAI 兼容端点 + OAuth 已实现

```
# OpenAI 兼容端点
端点：POST https://generativelanguage.googleapis.com/v1beta/openai/chat/completions
认证：Authorization: Bearer AIzaSyxxx（API Key）
格式：OpenAI chat completions 兼容
模型：gemini-2.0-flash, gemini-2.5-pro, gemini-2.5-flash
专家模型：gemini-2.5-pro（带 thinking 的推理模型）

# OAuth 方式（InnoForge 已实现）
gcloud auth application-default login → OAuth Token → 调 Gemini API
```

## 1.3 结论：API 方式三家全部可走现有架构

| 家 | base_url | 需要改动 |
|----|----------|----------|
| OpenAI | `https://api.openai.com/v1` | 仅加预设 |
| Claude | `https://api.anthropic.com/v1`（兼容端点） | 仅加预设 |
| Gemini | `https://generativelanguage.googleapis.com/v1beta/openai/` | 已有预设 |

**API 方式的开发量几乎为零**——在设置页面加三个预设选项即可。

真正需要开发的是**订阅方式（登录即用）**。
