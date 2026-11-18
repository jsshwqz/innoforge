//! 卡片 ⑦：系统诊断建议与数据包字段映射说明常量（`src/cards/settings_vault/prompts.rs`）

/// 首启四维健康自检诊断建议文案
pub const HEALTH_DIAGNOSTIC_GUIDE: &str = "\
InnoForge 四维环境健康自检说明：\
1. SQLite 存储内核：要求 WAL 并发模式与 Schema v26 就绪；\
2. 网络与同源鉴权：默认绑定 127.0.0.1 回环地址，非回环监听必须启用同源 HttpOnly Cookie / Bearer 令牌鉴权；\
3. 9 家 AI 服务商容灾链与看图模型：至少 1 家主力或本地 Ollama 端点可用，扫描件 PDF 三级回退依赖具备 supports_vision 的服务商；\
4. 四维检索与三级 OCR：OpenAlex / Crossref / arXiv / Semantic Scholar 免 Key 默认可用，SerpAPI / EPO 按需配置。";

/// 免 Python 原生数据包灌库字段映射说明
pub const PACK_FIELD_MAPPING_GUIDE: &str = "\
支持 JSON 数组、JSONL 行流、CSV 表格与 BibTeX (.bib) 四种原生格式直接流式灌库：\
- 文献标识：doc_id / id / patent_number / publication_number / doi / arxiv_id\n\
- 标题：title_zh / title / title_en\n\
- 作者/申请人：authors_or_assignees / authors / inventors / assignee\n\
- 日期与出处：pub_or_filing_date / date / year；venue_or_authority / journal / booktitle\n\
- 摘要与正文：abstract_text / abstract；claims_json / claims；fulltext_raw / fulltext\n\
- 资产角色与公开状态：asset_role (own_asset|competitor_wall|review_cited|general_ref)、filing_status (unfiled_secret|patent_filed|public_disclosed)";

/// 公开时序与资产角色策略提示
pub const FILING_STATUS_POLICY_NOTE: &str = "\
资产角色标注为 own_asset（我方已有成果）且公开状态为 unfiled_secret（未交底保密）时，\
成果锻造工坊将在导出学术论文或开源文档前触发“先专利申请日、后论文公开”时序冲突预警。";
