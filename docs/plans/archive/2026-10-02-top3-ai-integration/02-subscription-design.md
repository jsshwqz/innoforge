# 二、订阅方式（登录即用）—— 借鉴同类软件原理

## 2.1 三家订阅产品

| 家 | 订阅产品 | 价格 | 登录地址 | AI 端点 |
|----|----------|------|----------|---------|
| OpenAI | ChatGPT Plus / Pro | $20/月 / $200/月 | chat.openai.com | `chat.openai.com/backend-api/conversation` |
| Anthropic | Claude Pro / Max | $20/月 / $100-200/月 | claude.ai | `claude.ai/api/organizations/.../completion` |
| Google | Google One AI Premium | $20/月 | gemini.google.com | 已有 OAuth 覆盖 |

## 2.2 同类软件怎么做的

市场上"登录即用"的 AI 工具，核心原理就三种：

### 原理一：Cookie/Session 代理（ChatGPT 套壳类）

代表：ChatGPTNextWeb、pandora、各种 ChatGPT 镜像站

```
用户在 InnoForge 内登录 ChatGPT
  ↓
InnoForge 提取浏览器 Cookie（__Secure-next-auth.session-token）
  ↓
GET https://chat.openai.com/api/auth/session → 获取 accessToken
  ↓
POST https://chat.openai.com/backend-api/conversation
  Headers: Authorization: Bearer {accessToken}
  Body: { messages, model, ... }
  ↓
SSE 流式响应 → InnoForge 解析并返回给用户
```

**技术难点**：
- Cloudflare 防护（需要正确的 UA、cf_clearance Cookie）
- Arkose Token（FunCaptcha，部分请求需要）
- Device ID / 指纹（需要伪造或提取）
- Session Token 有效期约 30 天，需定期刷新

**Claude.ai 同理**：
```
登录 claude.ai → 提取 sessionKey Cookie（sk-ant-sid01-xxx）
  ↓
GET https://claude.ai/api/organizations → 获取 org_id
  ↓
POST https://claude.ai/api/organizations/{org_id}/chat_conversations → 建会话
  ↓
POST https://claude.ai/api/organizations/{org_id}/chat_conversations/{conv_id}/completion
  Body: { prompt, model, ... }
  ↓
SSE 流式响应
```

### 原理二：内嵌 WebView（Cursor / Monica / Sider 类）

代表：Cursor、Monica、Sider、各种浏览器 AI 扩展

```
InnoForge 内嵌 WebView 组件
  ↓
用户在 WebView 中直接打开 chat.openai.com / claude.ai
  ↓
用户在官方界面登录（InnoForge 不接触密码）
  ↓
登录成功后，InnoForge 通过 JS Bridge 提取会话信息
  ↓
后续 AI 请求通过提取的会话信息代理调用
```

**优点**：
- 用户在官方页面登录，InnoForge 不接触账号密码
- 登录过程走官方流程，不绕过任何安全验证
- Cloudflare、Arkose 等防护由 WebView 自动处理

**缺点**：
- 依赖 WebView 环境（Tauri/Electron）
- 首次登录需要用户在 WebView 中操作

### 原理三：标准 OAuth（GitHub Copilot / Cursor 官方集成）

代表：GitHub Copilot、Cursor（官方集成方式）

```
InnoForge 发起 OAuth 授权 → 用户在第三方页面授权
  → 回调带回 code → 换 Token → 用 Token 调 API
```

**问题**：OpenAI 和 Anthropic 的订阅产品（ChatGPT Plus / Claude Pro）**不提供 OAuth 授权接口**。OAuth 只用于 API 付费用户，不是订阅用户。所以这条路对订阅方式走不通。

## 2.3 推荐方案：内嵌 WebView + Cookie 提取（原理二 + 原理一混合）

**流程**：
1. 用户点击"登录 ChatGPT" → InnoForge 打开内嵌 WebView 到 `chat.openai.com`
2. 用户在 WebView 中正常登录（邮箱密码 / Google / Microsoft）
3. 登录成功后，InnoForge 通过 WebView Cookie API 提取：
   - `__Secure-next-auth.session-token`（OpenAI）
   - `sessionKey`（Claude）
4. 用提取的 Cookie 调用后端 API 获取 accessToken
5. 后续 AI 调用使用 accessToken + Cookie 代理
6. Cookie 过期时提示用户重新登录
