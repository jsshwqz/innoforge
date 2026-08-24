# M-A 施工规格书：多源检索框架（MA1/MA2/MA3/MA5 实现依据）

> 2026-08-15 规划会话编制。外部源事实经联网核实（来源见文末），标注 ⚠️ 的条目要求执行 agent 用冒烟用例实证后再依赖。
> 配套：prd-v1.md N1-N4 需求；types-migration-map.md 类型落位。

---

## 1. 统一契约（crates 内新模块，先于任何源实现）

```rust
// crates/search/src/model.rs（或 src/search/model.rs，随 T1.1 缩水版落位）
pub struct SearchQuery {
    pub keyword: String,          // 用户输入，原样保留
    pub country: Option<String>,  // "CN"/"US"/… 默认 Some("CN")
    pub language: Option<Lang>,   // Chinese/English/All，默认 Chinese
    pub assignee: Option<String>, // 申请人
    pub exact_assignee: bool,     // MA4：精确匹配开关
    pub date_from/date_to: Option<String>,
    pub limit: usize,
}

pub enum SourceKind { SerpApi, GooglePatentsXhr, EpoOps, LocalFts }

pub struct AttemptReport {          // MA5 诊断面板数据源
    pub source: SourceKind,
    pub status: AttemptStatus,      // Success | Failed(FailKind) | Skipped
    pub latency_ms: u64,
    pub hits: usize,
    pub error: Option<String>,      // 用户可读，禁止裸 err.to_string()
}

pub struct SearchOutcome {
    pub results: Vec<MergedPatent>, // 多源合并去重后（publication_number 为键）
    pub attempts: Vec<AttemptReport>,
}
```

**FailKind 分类（决定是否切下一源）**：Network(超时/DNS) → 切换；Quota(402/429/配额文案) → 标记冷却并切换；Auth(key 无效) → 冷却该源并切换；Parse(结构变化) → 记 bug 不切换直接报错。所有源调用必须带独立超时（建议 15s）+ 单次重试（指数退避，429 尊重 Retry-After）。

## 2. 源一：Google Patents 直抓（免费，无 Key）——推荐作为第一备用

- **XHR 端点**：`GET https://patents.google.com/xhr/query?url={urlencoded}`
  - `url=` 解码后即搜索框语法，如 `q=(固态电池)+(assignee=张三)&country=CN&language=CHINESE&sort=new`
  - 高级过滤作为 url 串的独立参数：`country=CN`、`language=CHINESE`、`assignee=`、`inventor=`、`before=/after=priority|publication:YYYYMMDD`
- **能力**：全球覆盖含 CN ✅、中文界面语言 ✅、无需 Key ✅
- **已知坑（⚠️实证项）**：
  1. 数据中心 IP 高频请求会吃 503——必须限速（建议 ≥2s 间隔+抖动）与退避；
  2. `after=` 服务端日期过滤在 XHR 路径上行为可能与网页不一致——日期过滤改为客户端二次过滤兜底；
  3. 返回 JSON 结构（cluster/results 字段）与网页 DOM 不同，需按实际响应建模。
- **合规边界**：仅个人本地工具自用频率；禁止包装成对外服务。

## 3. 源二：SerpAPI google_patents 引擎（付费主源，现役）

- 端点 `https://serpapi.com/search.json?engine=google_patents&q=...`；高级语法同样内嵌在 q 中（`(关键词)+(country=CN)` 等）
- 响应 `organic_results[]` 含 publication_number/title/snippet/assignee/inventor/pdf/country_status；详情走 `engine=google_patents_details`
- **改造点**：
  1. 迁入 trait 实现（从 routes/search.rs 抽出，逻辑保持）；
  2. q 构造统一由 SearchQuery 渲染（含 CN/language 注入——**修复"要中文给英文"的直接动作**）；
  3. 配额耗尽预警（余额接口已有 api_serpapi_balance，接入诊断面板）。

## 4. 源三：EPO OPS（免费注册，英文域权威）

- OAuth2 client_credentials：`POST https://ops.epo.org/3.2/auth/accesstoken`（Basic key:secret），token ≈20 分钟过期需缓存刷新
- 检索：`GET /3.2/rest-services/published-data/search?q=...&Range=1-25`，CQL 语法 `ti= ab= pa= ps= cn=CN`
- **定位**：英文/EU 域补充源；对中文检索贡献有限，排在链路第三位。仓内 routes/patent.rs 已有 fetch_epo 雏形可复用其鉴权代码
- 注册 Key 放设置页（新增两个配置位：EPO_KEY/EPO_SECRET，走 app_settings）

## 5. 源四（可选）：PatentsView（美国专利，免费注册）

- 新平台 `https://search.patentsview.org/api/v1/patent/` POST JSON 查询，需免费 API key
- 仅美国域；作为 US 深度查询备选，M-A 可只留接口位不实现（标 Skipped 即可）

## 6. 执行链策略（MA2 核心）

```
用户检索 → 并行发起 [SerpApi, GooglePatentsXhr]（各自 15s 超时）
         → 任一成功即开始返回；全部失败 → EPO OPS → 仍无 → 本地 FTS 兜底并明示"仅本地库"
合并去重键：publication_number 规范化（去空格/统一大小写）
```

- 不做严格串行瀑布（太慢）；并行+先到先得，AttemptReport 全量记录进诊断面板
- 每源独立熔断：连续 3 次 FailKind∈{Quota,Auth} 后冷却 10 分钟不再尝试

## 7. 本地库兜底与索引同步（MA4）

- 现状缺陷：FTS5 同步只在 save_patent 内手动 DELETE+重灌，绕过即失联
- 修法：所有检索结果入库统一走单一入口方法（内部同步 FTS）；新增回归测试"插入→立即可搜"；触发器方案列为评估项不强制
- **验收锚点（PRD）**：本人姓名精确搜索命中名下专利；已检索结果断网可复检

## 8. 冒烟清单（执行 agent 在合入前必须逐条实证并附证据到任务 Notes）

1. ⚠️ XHR 端点带 `country=CN&language=CHINESE` 搜中文关键词，确认响应含中文标题
2. ⚠️ XHR 端点 `assignee=<本人姓名>` 精确性验证（引号 vs 裸值对比）
3. SerpAPI q 内嵌 `(country=CN)` 后中文命中率对比
4. 断网模拟：禁用两在线源后本地 FTS 兜底路径可用
5. 503 场景退避生效（连续请求间隔 <1s 应触发限速保护而非打满错误）

## 参考来源

- [SerpAPI Google Patents Organic Results 文档](https://serpapi.com/google-patents-organic-results)（已抓取存 .firecrawl/）
- [Google Patents 隐藏 XHR 端点分析](https://dev.to/jdpg23/tracking-new-patent-filings-without-an-api-key-google-patents-has-a-hidden-xhr-endpoint-4ec7)（已抓取存 .firecrawl/）
- [PatentsView Search API](http://search.patentsview.org/docs/docs/Search%20API/SearchAPIReference/)
- [EPO OPS v3.2 官方文档 PDF](https://link.epo.org/web/ops_v3.2_documentation_-_version_1.3.18_en.pdf)
