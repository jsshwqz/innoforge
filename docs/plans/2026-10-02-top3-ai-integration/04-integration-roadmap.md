# 四、与 InnoForge 现有架构的整合

## 4.1 Provider 来源扩展

现有 `AiClient` 仅支持 API Key 方式。扩展为支持多种来源：

```rust
// src/ai/client.rs 扩展

enum ProviderSource {
    /// API Key 方式（现有）
    ApiKey { base_url: String, api_key: String, model: String },
    /// OAuth Token 方式（现有 Google，扩展到其他）
    OAuth { provider: String, token: String, model: String },
    /// 订阅 Cookie 代理方式（新增）
    Subscription { provider: String, session: SubscriptionSession, model: String },
}

impl AiClient {
    async fn chat(&self, messages: &[Message]) -> Result<String> {
        // 1. 尝试主 Provider（可能是任意来源）
        // 2. 失败 → Failover 到备用 Provider（可能跨来源）
        // 3. 全部失败 → 返回错误
    }
}
```

## 4.2 Failover 跨来源切换

```
主 Provider: ChatGPT 订阅（Subscription）
  ↓ 失败（Cookie 过期）
备用 1: Claude API Key（ApiKey）
  ↓ 失败（余额不足）
备用 2: Gemini OAuth（OAuth）
  ↓ 成功
返回结果
```

## 4.3 设置页面改动

在现有 `/settings` 页面新增"御三家 AI"配置区，每家提供：
- API Key 方式：输入 Key + 选择模型 + 测试连接
- 订阅方式：登录按钮 + 状态显示 + 退出
- Failover 优先级配置

---

# 五、实施路线图

## Phase 1：API 方式预设（1 天）

```
改动：
  src/ai/providers.rs（新增）    ← 三家预设配置
  static/settings.html          ← 设置页面加三家快捷选择
  src/routes/ai.rs              ← 加 /api/ai/test-connection 端点

内容：
  - OpenAI 预设：base_url + 模型列表
  - Claude 预设：base_url（兼容端点）+ 模型列表
  - Gemini 预设：已有，补充 Gemini 2.5 模型
  - 设置页面三家一键选择
  - 连接测试功能
```

## Phase 2：订阅代理核心（5-7 天）

```
改动：
  src/ai/subscription/mod.rs（新增）      ← SubscriptionProvider trait
  src/ai/subscription/openai.rs（新增）   ← ChatGPT 代理
  src/ai/subscription/anthropic.rs（新增）← Claude.ai 代理
  src/db/                                  ← ai_subscription_sessions 表
  src/routes/ai.rs                         ← 订阅登录/状态/退出端点
```

## Phase 3：WebView 登录集成（3-5 天）

```
改动：
  前端：内嵌 WebView 登录窗口
  src/routes/ai.rs ← WebView 回调处理
  static/settings.html ← 订阅登录 UI
```

## Phase 4：Failover 整合（2-3 天）

```
改动：
  src/ai/client.rs ← 扩展 ProviderSource 枚举 + 跨来源 Failover
```

## 时间线

```
Week 1:     Phase 1 (API 预设) + Phase 2 开始
Week 2:     Phase 2 (订阅代理核心)
Week 3:     Phase 3 (WebView 集成)
Week 4:     Phase 4 (Failover 整合) + 测试
```
