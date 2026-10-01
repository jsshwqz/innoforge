# 六、风险与注意事项

## 6.1 合规风险（重要）

| 风险 | 说明 | 缓解 |
|------|------|------|
| OpenAI ToS | Cookie 代理调用 ChatGPT 后端 API 可能违反 ToS | 标注"实验性功能"，用户自行承担风险 |
| Anthropic ToS | 同上，Cookie 代理 claude.ai 可能违反 ToS | 同上 |
| 账号封禁 | 第三方检测异常流量可能封号 | 限制请求频率，模拟正常用户行为 |

> ⚠️ **建议**：订阅代理方式标注为"实验性功能 - 可能违反第三方服务条款"，
> 用户启用前需明确确认风险。API Key 方式无此风险，应作为推荐方式。

## 6.2 技术风险

| 风险 | 说明 | 缓解 |
|------|------|------|
| Cloudflare 防护 | ChatGPT/Claude 有 CF 防护 | WebView 方式由浏览器自动处理 |
| 接口变更 | 第三方随时改后端 API | 模块化适配层，可快速更新 |
| Cookie 过期 | Session Token 有效期有限 | 自动检测 + 提示重新登录 |
| SSE 解析 | 流式响应格式各家不同 | 统一 SSE 解析器 + 各家适配 |

## 6.3 优先级建议

```
推荐落地顺序：

1. ✅ API Key 方式（Phase 1）—— 无风险，立即可用，1 天搞定
   用户自己注册 OpenAI/Anthropic/Google API，填 Key 即可

2. ✅ Gemini OAuth（已有）—— 无需开发，已实现
   用户 gcloud 登录即可用 Gemini

3. ⚠️ ChatGPT 订阅代理（Phase 2-3）—— 有风险但用户需求大
   让 ChatGPT Plus 用户不用再买 API Key

4. ⚠️ Claude 订阅代理（Phase 2-3）—— 同上
   让 Claude Pro 用户不用再买 API Key

5. ✅ Failover 整合（Phase 4）—— 无风险
   三种来源统一调度
```

---

# 七、总结

| 家 | API 方式 | 订阅方式 | 开发量 |
|----|----------|----------|--------|
| **OpenAI** | ✅ 加预设即可（1天） | ⚠️ Cookie 代理（5天） | 中 |
| **Claude** | ✅ 加预设即可（1天，走兼容端点） | ⚠️ Cookie 代理（5天） | 中 |
| **Gemini** | ✅ 已有预设 | ✅ 已有 OAuth | 几乎为零 |

**核心结论**：
1. **API 方式**：三家全部走现有 OpenAI 兼容架构，加预设即可，1 天搞定
2. **订阅方式**：借鉴 ChatGPT 套壳 + Cursor 的 WebView 登录模式，核心是 Cookie 提取 + 后端 API 代理
3. **Gemini** 订阅方式已通过 OAuth 实现，无需额外开发
4. **ChatGPT 和 Claude** 订阅方式需要新建订阅代理模块，约 2-3 周
5. 订阅代理有合规风险，建议标注"实验性"，API Key 方式作为推荐
