//! 数据结构定义 / Data Structures
//!
//! 专利、创意、搜索类型等核心数据结构。
//! Core data structures: Patent, Idea, SearchType, etc.

use serde::{Deserialize, Serialize};

/// 专利数据 / Patent data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Patent {
    #[serde(default = "gen_id")]
    pub id: String,
    pub patent_number: String,
    pub title: String,
    #[serde(default)]
    pub abstract_text: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub claims: String,
    #[serde(default)]
    pub applicant: String,
    #[serde(default)]
    pub inventor: String,
    #[serde(default)]
    pub filing_date: String,
    #[serde(default)]
    pub publication_date: String,
    pub grant_date: Option<String>,
    #[serde(default)]
    pub ipc_codes: String,
    #[serde(default)]
    pub cpc_codes: String,
    #[serde(default)]
    pub priority_date: String,
    #[serde(default)]
    pub country: String,
    #[serde(default)]
    pub kind_code: String,
    pub family_id: Option<String>,
    #[serde(default)]
    pub legal_status: String,
    #[serde(default)]
    pub citations: String,
    #[serde(default)]
    pub cited_by: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub raw_json: String,
    #[serde(default = "now_str")]
    pub created_at: String,
    /// JSON array of image URLs (patent drawings)
    #[serde(default)]
    pub images: String,
    /// PDF download URL
    #[serde(default)]
    pub pdf_url: String,
}

fn gen_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn now_str() -> String {
    chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

/// Canonical patent key for deduplication.
/// Keeps `country_prefix + main_digits` when present, strips spaces/dots/kind code.
pub fn canonical_patent_key(raw: &str) -> String {
    let upper = raw.trim().to_uppercase();
    if upper.is_empty() {
        return String::new();
    }
    let clean: String = upper
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    if clean.is_empty() {
        return String::new();
    }

    let bytes = clean.as_bytes();
    let mut i = 0usize;
    while i + 2 < bytes.len() {
        if bytes[i].is_ascii_alphabetic() && bytes[i + 1].is_ascii_alphabetic() {
            let mut j = i + 2;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            if j - (i + 2) >= 6 {
                return clean[i..j].to_string();
            }
        }
        i += 1;
    }

    let digits: String = clean.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() >= 8 {
        return digits;
    }
    clean
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum SearchType {
    Applicant,    // 按申请人搜索
    Inventor,     // 按发明人搜索
    PatentNumber, // 按专利号搜索
    Keyword,      // 关键词搜索（标题/摘要）
    Mixed,        // 混合搜索
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchRequest {
    pub query: String,
    #[serde(default = "d1")]
    pub page: usize,
    #[serde(default = "d20")]
    pub page_size: usize,
    pub country: Option<String>,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub search_type: Option<String>, // "applicant", "inventor", "keyword", "mixed"
    pub sort_by: Option<String>,     // "relevance", "new", "old"
    #[serde(default)]
    pub ipc: Option<String>, // IPC classification filter (prefix match)
    #[serde(default)]
    pub cpc: Option<String>, // CPC classification filter (prefix match)
    #[serde(default)]
    pub region: Option<String>, // "cn" (国内) | "intl" (国外) | None (auto)
}

fn d1() -> usize {
    1
}
fn d20() -> usize {
    20
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchResult {
    pub patents: Vec<PatentSummary>,
    pub total: usize,
    pub page: usize,
    pub page_size: usize,
    pub search_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub categories: Option<Vec<CategoryGroup>>,
    #[serde(default)]
    pub dedup_removed: usize,
}

/// Group of search results by category (applicant, country, etc.)
#[derive(Debug, Serialize, Deserialize)]
pub struct CategoryGroup {
    pub label: String,
    pub count: usize,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PatentSummary {
    pub id: String,
    pub patent_number: String,
    pub title: String,
    pub abstract_text: String,
    pub applicant: String,
    pub inventor: String,
    pub filing_date: String,
    pub country: String,
    #[serde(default)]
    pub relevance_score: Option<f64>,
    #[serde(default)]
    pub score_source: Option<String>, // 评分来源说明
}

#[cfg(test)]
mod tests {
    use super::canonical_patent_key;

    #[test]
    fn canonical_key_cn_with_spaces_and_kind() {
        assert_eq!(canonical_patent_key("CN 116401354 A"), "CN116401354");
    }

    #[test]
    fn canonical_key_us_with_kind_digit() {
        assert_eq!(canonical_patent_key("US1234567B2"), "US1234567");
    }

    #[test]
    fn canonical_key_google_patent_url_tail() {
        assert_eq!(
            canonical_patent_key("https://patents.google.com/patent/CN109876543A/zh"),
            "CN109876543"
        );
    }

    #[test]
    fn canonical_key_digits_fallback() {
        assert_eq!(canonical_patent_key("202310123456.7"), "2023101234567");
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AiChatRequest {
    pub message: String,
    pub patent_id: Option<String>,
    #[serde(default)]
    pub history: Vec<(String, String)>,
    #[serde(default)]
    pub web_search: bool,
    /// Base64-encoded images for multimodal vision models (e.g., Gemini)
    #[serde(default)]
    pub images: Vec<String>,
    /// Custom system prompt (overrides default role). Use preset key or full prompt text.
    /// Built-in presets: "patent_examiner" | "oa_expert" | "claim_analyst" | "inventor_brainstorm"
    #[serde(default)]
    pub system_prompt: Option<String>,
    /// Whether system_prompt is a preset key (true) or raw prompt text (false, default)
    #[serde(default)]
    pub preset_mode: bool,
}

/// Preset system prompts for AI chat roles
pub static SYSTEM_PROMPT_PRESETS: &[(&str, &str, &str)] = &[
    (
        "general",
        "通用助手 / General Assistant",
        "你是一位专业、严谨的研发助理，擅长回答技术问题，提供专利相关信息。请用简洁清晰的语言回答。",
    ),
    (
        "patent_examiner",
        "专利审查员视角 / Patent Examiner",
        "你是一位资深中国专利审查员（执业20年，曾任复审委员会成员）。你精通中国专利法及审查指南（2023修订版），对创造性审查（A22.3）尤为严格。你善于发现技术方案中的逻辑漏洞和论证不足，对对比文献公开内容的认定极其敏感。请用挑剔、专业的眼光审阅，明确指出事实认定不准确之处和法律推理的漏洞。语气直接、客观。",
    ),
    (
        "oa_expert",
        "专利答辩专家 / OA Response Expert",
        "你是一位资深中国专利代理师（执业20年+），精通中国专利法及审查指南。你擅长答复审查意见通知书，尤其是创造性驳回（A22.3）的答辩。你的分析必须精确到技术特征级别，每一处论断都必须引用原文。工作流程：先深度分析（逐特征对比），再写回复。关键原则：(1)精确到特征级 (2)引用原文 (3)区分事实与观点 (4)诚实分析——如果对比文献确实公开了某特征，必须承认 (5)必须分析组合动机。语气尊重审查员但立场坚定，不使用加粗、表情符号或过度格式化。",
    ),
    (
        "claim_analyst",
        "权利要求分析师 / Claims Analyst",
        "你是一位资深专利权利要求分析师。你擅长：(1)解析权利要求的保护范围，识别独立/从属关系，构建权利要求树 (2)逐特征分析技术要素，评估保护宽度 (3)识别潜在规避设计方向 (4)对比多组权利要求的结构异同。请用结构化方式输出，对每项独立权利要求单独分析，可辅以表格辅助说明。",
    ),
    (
        "inventor_brainstorm",
        "发明人头脑风暴 / Inventor Brainstorm",
        "你是一位经验丰富的研发创新导师，擅长与发明人进行头脑风暴。你善于：(1)从零散的技术想法中提炼核心发明构思 (2)引导发明人梳理技术问题的来龙去脉 (3)识别现有技术的盲点和改进空间 (4)帮助发明人用专利语言重新描述其技术方案。请用引导式提问帮助发明人深入思考，而非直接给出答案。语气鼓励、开放、建设性。",
    ),
];

impl AiChatRequest {
    /// Resolve the effective system prompt: preset key → full text, or use raw custom prompt
    pub fn effective_system_prompt(&self) -> Option<String> {
        match &self.system_prompt {
            Some(p) if self.preset_mode => {
                let preset = SYSTEM_PROMPT_PRESETS
                    .iter()
                    .find(|(k, _, _)| *k == p.as_str());
                preset.map(|(_, _, prompt)| prompt.to_string())
            }
            Some(p) => Some(p.clone()),
            None => None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AiResponse {
    pub content: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FetchPatentRequest {
    pub patent_number: String,
    pub source: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ImportRequest {
    pub patents: Vec<Patent>,
}

// ── Idea / Innovation validation ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Idea {
    pub id: String,
    pub title: String,
    pub description: String,
    pub input_type: String,
    pub status: String,
    pub analysis: String,
    pub web_results: String,
    pub patent_results: String,
    pub novelty_score: Option<f64>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub discussion_summary: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IdeaSubmitRequest {
    pub title: String,
    pub description: String,
    #[serde(default = "default_text")]
    pub input_type: String,
}

fn default_text() -> String {
    "text".to_string()
}

// ── Legal Status 法律状态 ────────────────────────────────────────────────────

/// 专利法律状态查询结果 / Patent legal status result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegalStatusResult {
    pub patent_number: String,
    /// "有效" | "无效" | "审查中" | "公开" | "驳回" | "撤回" | "未知"
    pub current_status: String,
    pub events: Vec<LegalEvent>,
    /// "google_patents" | "lens" | "cnipa_gazette" | "sogou"
    pub source: String,
    pub updated_at: String,
}

/// 单条法律状态事件 / A single legal status event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegalEvent {
    pub date: String,
    /// "公开" | "实审" | "授权" | "缴费" | "驳回" | "无效" | "转让" 等
    pub title: String,
    pub description: String,
}

// ── Feature Cards ────────────────────────────────────────────────────────────

/// A feature card linked to an idea, capturing a specific inventive feature.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureCard {
    pub id: String,
    pub idea_id: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub novelty_score: Option<f64>,
    pub created_at: String,
    // 5 维结构化字段
    #[serde(default)]
    pub technical_problem: String,
    #[serde(default)]
    pub core_structure: String,
    #[serde(default)]
    pub key_relations: String,
    #[serde(default)]
    pub process_steps: String,
    #[serde(default)]
    pub application_scenarios: String,
}

/// Request body for creating a new feature card.
#[derive(Debug, Deserialize)]
pub struct CreateFeatureCardRequest {
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub novelty_score: Option<f64>,
    #[serde(default)]
    pub technical_problem: String,
    #[serde(default)]
    pub core_structure: String,
    #[serde(default)]
    pub key_relations: String,
    #[serde(default)]
    pub process_steps: String,
    #[serde(default)]
    pub application_scenarios: String,
}

// ── 权利要求树 / Claim Tree ──────────────────────────────────────

/// 权利要求节点
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct ClaimNode {
    pub id: String,
    pub idea_id: String,
    pub claim_number: u32,
    pub claim_type: ClaimType,
    pub parent_claim_id: Option<String>,
    pub content: String,
    pub features: Vec<TechnicalFeature>,
    pub created_at: String,
}

/// 权利要求类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub enum ClaimType {
    Independent,
    Dependent,
}

/// 必要技术特征
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct TechnicalFeature {
    pub id: String,
    pub claim_id: String,
    pub description: String,
    pub novelty_flag: bool,
    pub evidence_ids: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IdeaSummary {
    pub id: String,
    pub title: String,
    pub status: String,
    pub novelty_score: Option<f64>,
    pub created_at: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub message_count: i32,
}
