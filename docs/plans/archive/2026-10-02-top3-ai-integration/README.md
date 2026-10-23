# 御三家 AI 接入规划

本目录包含 OpenAI + Anthropic Claude + Google Gemini 接入 InnoForge 的规划文档。

## 文档列表

- `2026-10-02-top3-ai-integration.md` — 御三家接入专项规划（API + 订阅两种方式）
- `2026-10-02-ai-integration-general.md` — AI 集成通用规划（含市场调研）

## 核心结论

1. **API 方式**：三家全部走现有 OpenAI 兼容架构，加预设即可，1 天搞定
2. **订阅方式**：借鉴 ChatGPT 套壳 + Cursor 的 WebView 登录模式
3. **Gemini** 订阅方式已通过 OAuth 实现，无需额外开发
4. **ChatGPT 和 Claude** 订阅方式需要新建订阅代理模块，约 2-3 周
