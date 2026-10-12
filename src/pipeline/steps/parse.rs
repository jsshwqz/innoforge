//! Step 1: ParseInput - parse user input, extract keywords and technical domain
//! Type: CODE (pure code, no LLM call)

use crate::db::Database;
use crate::pipeline::context::PipelineContext;
use anyhow::Result;
use std::collections::HashSet;
use std::sync::Arc;

const ZH_STOPWORDS: &[&str] = &[
    "的", "了", "在", "是", "我", "有", "和", "就", "不", "人", "都", "一", "一个", "上", "也",
    "很", "到", "说", "要", "去", "你", "会", "着", "没有", "看", "好", "自己", "这", "他", "她",
    "它", "们", "那", "些", "可以", "什么", "用", "能", "如何", "通过", "进行", "以及", "或者",
    "但是", "因为", "所以", "如果", "而", "与", "将", "对", "把", "从", "被", "比", "为", "等",
    "该", "其", "中", "更", "之", "及",
];

const EN_STOPWORDS: &[&str] = &[
    "the", "a", "an", "is", "are", "was", "were", "be", "been", "being", "have", "has", "had",
    "do", "does", "did", "will", "would", "could", "should", "may", "might", "can", "shall", "and",
    "or", "but", "if", "in", "on", "at", "to", "for", "of", "with", "by", "from", "as", "into",
    "about", "between", "through", "during", "before", "after", "above", "below", "this", "that",
    "these", "those", "it", "its", "not", "no", "nor", "so", "than", "too", "very", "just", "also",
];

pub fn tokenize(text: &str) -> Vec<String> {
    let stopwords: HashSet<&str> = ZH_STOPWORDS
        .iter()
        .chain(EN_STOPWORDS.iter())
        .copied()
        .collect();
    let mut tokens = Vec::new();
    let mut current_ascii = String::new();
    let chars: Vec<char> = text.chars().collect();

    for &ch in &chars {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            current_ascii.push(ch.to_ascii_lowercase());
        } else {
            if !current_ascii.is_empty() {
                if current_ascii.len() >= 2 && !stopwords.contains(current_ascii.as_str()) {
                    tokens.push(current_ascii.clone());
                }
                current_ascii.clear();
            }
        }
    }
    if !current_ascii.is_empty()
        && current_ascii.len() >= 2
        && !stopwords.contains(current_ascii.as_str())
    {
        tokens.push(current_ascii);
    }

    let chinese_chars: Vec<char> = chars
        .iter()
        .filter(|c| (**c as u32) >= 0x4E00 && (**c as u32) <= 0x9FFF)
        .copied()
        .collect();

    for window in chinese_chars.windows(2) {
        let bigram: String = window.iter().collect();
        if !stopwords.contains(bigram.as_str()) {
            tokens.push(bigram);
        }
    }

    for &ch in &chinese_chars {
        let s = ch.to_string();
        if !stopwords.contains(s.as_str()) {
            tokens.push(s);
        }
    }

    tokens
}

fn extract_keywords(text: &str, max_keywords: usize) -> Vec<String> {
    let tokens = tokenize(text);
    let mut freq: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for token in &tokens {
        if token.len() >= 2 {
            *freq.entry(token.clone()).or_default() += 1;
        }
    }
    let mut sorted: Vec<_> = freq.into_iter().collect();
    sorted.sort_by_key(|item| std::cmp::Reverse(item.1));
    sorted
        .into_iter()
        .take(max_keywords)
        .map(|(k, _)| k)
        .collect()
}

fn infer_domain(keywords: &[String]) -> String {
    let domain_indicators: &[(&[&str], &str)] = &[
        (
            &[
                "电池", "锂", "充电", "能量", "储能", "电极", "battery", "lithium", "energy",
            ],
            "新能源/电池技术",
        ),
        (
            &[
                "芯片",
                "半导体",
                "晶圆",
                "集成电路",
                "chip",
                "semiconductor",
                "wafer",
            ],
            "半导体/集成电路",
        ),
        (
            &[
                "算法",
                "模型",
                "神经网络",
                "深度学习",
                "ai",
                "machine",
                "learning",
                "neural",
            ],
            "人工智能/机器学习",
        ),
        (
            &[
                "药物", "蛋白", "基因", "细胞", "抗体", "drug", "protein", "gene", "cell",
            ],
            "生物医药",
        ),
        (
            &[
                "机器人",
                "传感器",
                "控制",
                "自动",
                "robot",
                "sensor",
                "control",
                "autonomous",
            ],
            "机器人/自动化",
        ),
        (
            &[
                "通信", "信号", "频率", "天线", "5g", "6g", "wireless", "antenna",
            ],
            "通信技术",
        ),
        (
            &[
                "材料", "合金", "涂层", "纳米", "material", "alloy", "coating", "nano",
            ],
            "新材料",
        ),
        (
            &[
                "发动机",
                "飞行",
                "航空",
                "推进",
                "engine",
                "flight",
                "aviation",
                "propulsion",
            ],
            "航空航天",
        ),
        (
            &[
                "阀门",
                "流量",
                "管道",
                "液压",
                "valve",
                "flow",
                "pipe",
                "hydraulic",
            ],
            "流体控制/阀门",
        ),
    ];
    let keywords_lower: Vec<String> = keywords.iter().map(|k| k.to_lowercase()).collect();
    let mut best_domain = "通用技术";
    let mut best_score = 0;
    for (indicators, domain) in domain_indicators {
        let score = indicators
            .iter()
            .filter(|ind| keywords_lower.iter().any(|k| k.contains(*ind)))
            .count();
        if score > best_score {
            best_score = score;
            best_domain = domain;
        }
    }
    best_domain.to_string()
}

/// Inject relevant memory entries into the pipeline context.
/// Load previous memory for this idea and enrich keywords/hypothesis.
fn inject_memory(ctx: &mut PipelineContext, db: &Arc<Database>) {
    let entries = match db.get_memory_entries(&ctx.idea_id) {
        Ok(e) => e,
        Err(e) => {
            tracing::debug!("Failed to load memory: {}", e);
            return;
        }
    };
    if entries.is_empty() {
        return;
    }
    tracing::info!("Injecting {} memory entries into pipeline", entries.len());

    // Collect domain concepts to supplement keywords
    let concepts: Vec<String> = entries
        .iter()
        .filter(|e| e.concept_type == "domain_concept" && e.confidence >= 0.5)
        .map(|e| e.concept_name.clone())
        .take(10)
        .collect();

    // Add memory-derived keywords if we don't already have them
    for concept in &concepts {
        if !ctx.keywords.contains(concept) {
            ctx.keywords.push(concept.clone());
        }
    }

    // Add context note about memory
    let memory_note = format!(
        "\n\n[记忆注入] 从既往 pipeline 运行中加载了 {} 条记忆条目。\n相关概念: {}\n",
        entries.len(),
        concepts.join(", ")
    );
    ctx.description.push_str(&memory_note);
}

pub async fn execute(ctx: &mut PipelineContext, db: &Arc<Database>) -> Result<()> {
    let full_text = format!("{} {}", ctx.title, ctx.description);
    ctx.keywords = extract_keywords(&full_text, 20);
    ctx.technical_domain = infer_domain(&ctx.keywords);

    ctx.research_state.current_hypothesis = format!(
        "创意「{}」在{}领域可能具有新颖性",
        ctx.title, ctx.technical_domain
    );

    if ctx.keywords.len() < 2 {
        anyhow::bail!("输入内容太短，无法提取有效关键词（至少需要 2 个关键词）");
    }

    // Inject previous memory to enrich context
    inject_memory(ctx, db);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_tokenize_chinese() {
        let tokens = tokenize("流量矢量调节阀门");
        assert!(!tokens.is_empty());
        assert!(tokens.contains(&"流量".to_string()));
    }
    #[test]
    fn test_tokenize_english() {
        let tokens = tokenize("battery management system for electric vehicles");
        assert!(tokens.contains(&"battery".to_string()));
        assert!(tokens.contains(&"management".to_string()));
        assert!(!tokens.contains(&"for".to_string()));
    }
    #[test]
    fn test_extract_keywords() {
        let keywords = extract_keywords("用于抑制大范围火灾的耐火飞行器无人机灭火系统", 10);
        assert!(!keywords.is_empty());
    }
    #[test]
    fn test_infer_domain() {
        let kw = vec!["阀门".into(), "流量".into(), "调节".into()];
        assert_eq!(infer_domain(&kw), "流体控制/阀门");
    }
}
