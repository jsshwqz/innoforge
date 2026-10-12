# 三、技术实现细节

## 3.1 OpenAI ChatGPT 订阅代理

```rust
// src/ai/subscription/openai.rs

struct ChatGPTSubscription {
    session_token: String,    // __Secure-next-auth.session-token
    access_token: String,     // 从 /api/auth/session 获取
    user_email: String,
    plan_type: String,        // "plus" | "pro" | "free"
}

impl ChatGPTSubscription {
    fn from_cookies(cookies: &CookieJar) -> Result<Self> {
        let session_token = cookies.get("__Secure-next-auth.session-token")?;
        // ...
    }
    
    async fn fetch_access_token(&self) -> Result<String> {
        // GET https://chat.openai.com/api/auth/session
        // Cookie: __Secure-next-auth.session-token=xxx
        // → { "accessToken": "eyJxxx...", "user": {...} }
    }
    
    async fn chat(&self, messages: &[Message], model: &str) -> Result<String> {
        // POST https://chat.openai.com/backend-api/conversation
        // Authorization: Bearer {access_token}
        // Body: { "action": "next", "messages": [...], "model": "gpt-4o",
        //   "conversation_id": null, "parent_message_id": uuid, "stream": true }
        // → SSE 流式响应
    }
}
```

| ChatGPT 内部名 | 对应模型 | 说明 |
|----------------|----------|------|
| `auto` | 自动选择 | ChatGPT 自动选最佳模型 |
| `gpt-4o` | GPT-4o | 主力模型 |
| `gpt-4o-mini` | GPT-4o-mini | 快速模型 |
| `o3-mini` | o3-mini | 推理模型（Plus 可用） |
| `o3` | o3 | 强推理模型（Pro 可用） |

## 3.2 Anthropic Claude 订阅代理

```rust
// src/ai/subscription/anthropic.rs

struct ClaudeSubscription {
    session_key: String,      // sessionKey Cookie (sk-ant-sid01-xxx)
    org_id: String,           // 组织 ID
    user_uuid: String,
    plan_type: String,        // "pro" | "max" | "free"
}

impl ClaudeSubscription {
    fn from_cookies(cookies: &CookieJar) -> Result<Self> {
        let session_key = cookies.get("sessionKey")?;
        // ...
    }
    
    async fn fetch_org_id(&self) -> Result<String> {
        // GET https://claude.ai/api/organizations
        // Cookie: sessionKey=sk-ant-sid01-xxx
        // → [{ "uuid": "org-xxx", ... }]
    }
    
    async fn chat(&self, messages: &[Message], model: &str) -> Result<String> {
        // 1. POST .../chat_conversations → { "uuid": "conv-xxx" }
        // 2. POST .../chat_conversations/{conv_id}/completion
        //    Body: { "prompt": ..., "model": "claude-sonnet-4-6", ... }
        //    → SSE 流式响应
    }
}
```

| Claude.ai 内部名 | 对应模型 | 订阅要求 |
|-----------------|----------|----------|
| `claude-sonnet-4-6` | Claude Sonnet 4.6 | Pro+ |
| `claude-haiku-4-5` | Claude Haiku 4.5 | 免费 |
| `claude-opus-4` | Claude Opus 4 | Max |

## 3.3 Google Gemini —— 已有 OAuth 覆盖

Google One AI Premium 订阅用户通过 OAuth 登录后即可使用 Gemini，**InnoForge 已实现此功能**（`src/routes/auth.rs`）。

需要补充：
- OAuth Token 有效时自动将 Gemini 设为可用 Provider
- 设置页面显示 Gemini OAuth 登录状态
- 支持通过 OAuth Token 调用 Gemini 2.5 Pro（专家模型）

## 3.4 统一抽象层

```rust
// src/ai/subscription/mod.rs

#[async_trait]
trait SubscriptionProvider {
    fn id(&self) -> &str;           // "openai" | "anthropic" | "google"
    fn display_name(&self) -> &str;  // "ChatGPT" | "Claude" | "Gemini"
    fn from_cookies(cookies: &CookieJar) -> Result<Self> where Self: Sized;
    async fn check_login(&self) -> Result<LoginStatus>;
    async fn get_plan(&self) -> Result<PlanInfo>;
    async fn chat(&self, req: &ChatRequest) -> Result<ChatResponse>;
    async fn chat_stream(&self, req: &ChatRequest) -> Result<ChatStream>;
    async fn list_models(&self) -> Result<Vec<ModelInfo>>;
    async fn refresh(&mut self) -> Result<()>;
}

enum LoginStatus {
    LoggedIn { email: String, plan: String },
    LoggedOut,
    Expired,
}
```

## 3.5 数据库设计

```sql
CREATE TABLE ai_subscription_sessions (
    id INTEGER PRIMARY KEY,
    provider TEXT NOT NULL,          -- 'openai' | 'anthropic' | 'google'
    session_token TEXT NOT NULL,     -- 加密存储
    access_token TEXT,               -- 加密存储
    org_id TEXT,                     -- Claude 用
    user_email TEXT,
    plan_type TEXT,                  -- 'plus' | 'pro' | 'max' | 'free'
    expires_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    is_active INTEGER DEFAULT 1,
    UNIQUE(provider)
);

CREATE TABLE ai_subscription_usage (
    id INTEGER PRIMARY KEY,
    provider TEXT NOT NULL,
    model TEXT NOT NULL,
    input_tokens INTEGER,
    output_tokens INTEGER,
    success INTEGER NOT NULL,
    error_msg TEXT,
    created_at TEXT NOT NULL
);
```
