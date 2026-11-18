//! 卡片 ① 全域情报检索提示词（`src/cards/omni_search/prompts.rs`）
//!
//! 承载四维检索式 AI 扩写提示词与命中可解释理由生成提示词。

/// 四维检索式双语扩写系统提示词（供 `QueryPlanner::plan_with_optional_rewrite` 可选调用）
pub const OMNI_QUERY_REWRITE_SYSTEM_PROMPT: &str = r#"你是跨学科技术情报检索规划专家。请将用户输入的技术构想或关键词，改写为面向以下四个维度的精准双语检索式，并严格以 JSON 对象返回（不要输出多余解释或 Markdown 代码围栏）：
{
  "keywords": ["核心中文词", "core english term"],
  "expanded_terms": ["同义扩展词1", "synonym 2"],
  "patent_boolean_query": "(关键词A OR termA) AND (关键词B OR termB)",
  "scholar_query": "concise english academic search query",
  "code_query": "github technical keywords",
  "standard_query": "standard specification keywords"
}"#;

/// 生成检索式扩写的用户提示词载荷
pub fn build_query_rewrite_user_prompt(raw_query: &str, deterministic_boolean: &str) -> String {
    format!(
        "原始检索意图：{}\n确定性基线布尔式：{}\n请输出优化后的 JSON 检索规划。",
        raw_query.trim(),
        deterministic_boolean.trim()
    )
}

/// 生成命中理由摘要（纯确定性兜底，确保无 AI 时也能给出可解释推荐原因）
pub fn format_deterministic_match_reason(
    source_provider: &str,
    matched_terms: &[String],
    doi_verified: bool,
) -> String {
    let terms_str = if matched_terms.is_empty() {
        "全文相关性匹配".to_string()
    } else {
        matched_terms.join(" / ")
    };
    if doi_verified {
        format!(
            "来源 [{}] · 真实 DOI 已核验 · 命中特征：{}",
            source_provider, terms_str
        )
    } else {
        format!("来源 [{}] · 命中特征：{}", source_provider, terms_str)
    }
}
