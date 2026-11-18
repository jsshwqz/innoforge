//! 四维检索查询规划器（`query_planner.rs`）
//!
//! 负责将用户自然语言想法或技术关键词确定性地拆解并双语扩展为面向专利、学术论文、
//! 开源工程与行业标准四大圈层的结构化检索式；若挂载可选 AI 改写闭包，强制包裹
//! `tokio::time::timeout(Duration::from_secs(20), ...)`，超时或失败静默回退确定性检索式。

use std::collections::BTreeSet;
use std::future::Future;
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// 编排子步骤 AI 改写硬超时上限（严守 `#76`）
const AI_REWRITE_TIMEOUT: Duration = Duration::from_secs(20);

/// 常见中英文技术术语确定性双语扩展映射表
const BILINGUAL_TECH_DICT: &[(&str, &str)] = &[
    ("固态电池", "solid-state battery"),
    ("电解质", "electrolyte"),
    ("正极", "cathode"),
    ("负极", "anode"),
    ("硅碳", "silicon-carbon"),
    ("锂离子", "lithium-ion"),
    ("热管理", "thermal management"),
    ("相变材料", "phase change material"),
    ("人工智能", "artificial intelligence"),
    ("大语言模型", "large language model"),
    ("检索增强", "retrieval-augmented generation"),
    ("知识图谱", "knowledge graph"),
    ("微流控", "microfluidic"),
    ("光刻", "lithography"),
    ("碳化硅", "silicon carbide"),
    ("氮化镓", "gallium nitride"),
    ("逆变器", "inverter"),
    ("自动驾驶", "autonomous driving"),
    ("激光雷达", "LiDAR"),
    ("传感器", "sensor"),
    ("超声", "ultrasonic"),
    ("加密", "encryption"),
    ("零知识证明", "zero-knowledge proof"),
    ("联邦学习", "federated learning"),
    ("压缩", "compression"),
    ("降噪", "noise reduction"),
    ("battery", "电池"),
    ("electrolyte", "电解质"),
    ("cathode", "正极"),
    ("anode", "负极"),
    ("thermal", "热管理"),
    ("sensor", "传感器"),
    ("lidar", "激光雷达"),
    ("microfluidic", "微流控"),
    ("retrieval", "检索"),
    ("embedding", "向量嵌入"),
    ("transformer", "变换器"),
    ("encryption", "加密"),
    ("inverter", "逆变器"),
];

/// 四维全域检索查询规划结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryPlan {
    pub raw_query: String,
    pub keywords: Vec<String>,
    pub expanded_terms: Vec<String>,
    pub patent_boolean_query: String,
    pub scholar_query: String,
    pub code_query: String,
    pub standard_query: String,
    pub used_ai_rewrite: bool,
}

/// 四维查询规划器
pub struct QueryPlanner;

impl QueryPlanner {
    /// 纯确定性构建四维检索规划（零网络、零 LLM、毫秒级完成）
    pub fn plan_deterministic(raw_query: &str) -> QueryPlan {
        let trimmed = raw_query.trim();
        let keywords = extract_keywords(trimmed);
        let expanded_terms = expand_bilingual_terms(trimmed, &keywords);

        let patent_boolean_query = build_patent_boolean_query(&keywords, &expanded_terms);
        let scholar_query = build_scholar_query(trimmed, &keywords, &expanded_terms);
        let code_query = build_code_query(&keywords, &expanded_terms);
        let standard_query = build_standard_query(&keywords, &expanded_terms);

        QueryPlan {
            raw_query: trimmed.to_string(),
            keywords,
            expanded_terms,
            patent_boolean_query,
            scholar_query,
            code_query,
            standard_query,
            used_ai_rewrite: false,
        }
    }

    /// 带可选异步 AI 改写闭包的规划入口：强制 `20s` 超时，超时或报错时静默回退确定性检索式
    pub async fn plan_with_optional_rewrite<F, Fut>(
        raw_query: &str,
        rewriter: Option<F>,
    ) -> QueryPlan
    where
        F: FnOnce(QueryPlan) -> Fut,
        Fut: Future<Output = anyhow::Result<QueryPlan>>,
    {
        let fallback = Self::plan_deterministic(raw_query);
        let Some(rewrite_fn) = rewriter else {
            return fallback;
        };

        match tokio::time::timeout(AI_REWRITE_TIMEOUT, rewrite_fn(fallback.clone())).await {
            Ok(Ok(mut rewritten)) => {
                if rewritten.patent_boolean_query.trim().is_empty()
                    && rewritten.scholar_query.trim().is_empty()
                {
                    return fallback;
                }
                if rewritten.keywords.is_empty() {
                    rewritten.keywords = fallback.keywords;
                }
                if rewritten.raw_query.trim().is_empty() {
                    rewritten.raw_query = fallback.raw_query;
                }
                if rewritten.code_query.trim().is_empty() {
                    rewritten.code_query = fallback.code_query;
                }
                if rewritten.standard_query.trim().is_empty() {
                    rewritten.standard_query = fallback.standard_query;
                }
                rewritten.used_ai_rewrite = true;
                rewritten
            }
            Ok(Err(_)) | Err(_) => fallback,
        }
    }
}

fn extract_keywords(raw: &str) -> Vec<String> {
    if raw.is_empty() {
        return Vec::new();
    }
    let mut seen = BTreeSet::new();
    let mut result = Vec::new();

    // 先按空白与常见标点切分
    for token in raw.split(|c: char| {
        c.is_whitespace()
            || matches!(
                c,
                ',' | '，'
                    | ';'
                    | '；'
                    | '、'
                    | '|'
                    | '/'
                    | '('
                    | ')'
                    | '（'
                    | '）'
                    | '['
                    | ']'
                    | '"'
                    | '\''
                    | '。'
                    | '.'
            )
    }) {
        let cleaned = token.trim();
        if cleaned.chars().count() < 2 {
            continue;
        }
        let lower = cleaned.to_lowercase();
        if is_stopword(&lower) {
            continue;
        }
        if seen.insert(lower) {
            result.push(cleaned.to_string());
        }
    }

    // 额外匹配词典中的中文连续术语（应对无空格中文长句）
    let raw_lower = raw.to_lowercase();
    for &(src, _) in BILINGUAL_TECH_DICT {
        if raw_lower.contains(&src.to_lowercase()) && seen.insert(src.to_lowercase()) {
            result.push(src.to_string());
        }
    }

    if result.is_empty() && !raw.is_empty() {
        result.push(raw.to_string());
    }

    result
}

fn expand_bilingual_terms(raw: &str, keywords: &[String]) -> Vec<String> {
    let mut seen = BTreeSet::new();
    for kw in keywords {
        seen.insert(kw.to_lowercase());
    }

    let raw_lower = raw.to_lowercase();
    let mut expanded = Vec::new();
    for &(src, dst) in BILINGUAL_TECH_DICT {
        let src_lower = src.to_lowercase();
        let matched = raw_lower.contains(&src_lower)
            || keywords
                .iter()
                .any(|k| k.to_lowercase().contains(&src_lower));
        if matched && seen.insert(dst.to_lowercase()) {
            expanded.push(dst.to_string());
        }
    }
    expanded
}

fn build_patent_boolean_query(keywords: &[String], expanded: &[String]) -> String {
    if keywords.is_empty() && expanded.is_empty() {
        return String::new();
    }

    let mut groups: Vec<String> = Vec::new();
    for kw in keywords.iter().take(6) {
        let quoted = quote_if_spaced(kw);
        // 若存在对应双语扩展词，构造 (kw OR ext) 组
        let mut synonyms = vec![quoted];
        let kw_lower = kw.to_lowercase();
        for &(src, dst) in BILINGUAL_TECH_DICT {
            if kw_lower.contains(&src.to_lowercase()) {
                let ext = quote_if_spaced(dst);
                if !synonyms.contains(&ext) {
                    synonyms.push(ext);
                }
            }
        }
        if synonyms.len() > 1 {
            groups.push(format!("({})", synonyms.join(" OR ")));
        } else if let Some(first) = synonyms.into_iter().next() {
            groups.push(first);
        }
    }

    if groups.is_empty() {
        return expanded
            .iter()
            .take(5)
            .map(|t| quote_if_spaced(t))
            .collect::<Vec<_>>()
            .join(" OR ");
    }

    groups.join(" AND ")
}

fn build_scholar_query(raw: &str, keywords: &[String], expanded: &[String]) -> String {
    let mut parts: Vec<String> = Vec::new();
    // 学术检索优先使用英文词与核心关键词组合，提升 OpenAlex/Crossref/arXiv/S2 召回率
    for ext in expanded.iter().take(5) {
        if ext.is_ascii() {
            parts.push(ext.clone());
        }
    }
    for kw in keywords.iter().take(6) {
        if !parts.iter().any(|p| p.eq_ignore_ascii_case(kw)) {
            parts.push(kw.clone());
        }
    }
    if parts.is_empty() {
        raw.to_string()
    } else {
        parts.join(" ")
    }
}

fn build_code_query(keywords: &[String], expanded: &[String]) -> String {
    let mut ascii_terms: Vec<String> = Vec::new();
    for term in expanded.iter().chain(keywords.iter()) {
        if term.is_ascii() && !ascii_terms.iter().any(|t| t.eq_ignore_ascii_case(term)) {
            ascii_terms.push(term.clone());
        }
        if ascii_terms.len() >= 4 {
            break;
        }
    }
    if ascii_terms.is_empty() {
        keywords
            .iter()
            .take(3)
            .cloned()
            .collect::<Vec<_>>()
            .join(" ")
    } else {
        ascii_terms.join(" ")
    }
}

fn build_standard_query(keywords: &[String], expanded: &[String]) -> String {
    let mut terms: Vec<String> = Vec::new();
    for term in keywords.iter().take(4).chain(expanded.iter().take(3)) {
        if !terms.iter().any(|t| t.eq_ignore_ascii_case(term)) {
            terms.push(term.clone());
        }
    }
    terms.join(" ")
}

fn quote_if_spaced(term: &str) -> String {
    if term.contains(' ') {
        format!("\"{term}\"")
    } else {
        term.to_string()
    }
}

fn is_stopword(lower: &str) -> bool {
    matches!(
        lower,
        "the"
            | "and"
            | "for"
            | "with"
            | "from"
            | "that"
            | "this"
            | "into"
            | "based"
            | "using"
            | "一种"
            | "以及"
            | "基于"
            | "用于"
            | "方法"
            | "系统"
            | "装置"
    )
}
