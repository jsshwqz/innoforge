//! Step 1: ParseInput - parse user input, extract keywords and technical domain
//! Type: CODE (pure code, no LLM call)

use crate::db::Database;
use crate::pipeline::context::{PipelineContext, QueryIntent};
use anyhow::Result;
use std::collections::HashSet;
use std::sync::Arc;

const ZH_STOPWORDS: &[&str] = &[
    "的",
    "了",
    "在",
    "是",
    "我",
    "有",
    "和",
    "就",
    "不",
    "人",
    "都",
    "一",
    "一个",
    "上",
    "也",
    "很",
    "到",
    "说",
    "要",
    "去",
    "你",
    "会",
    "着",
    "没有",
    "看",
    "好",
    "自己",
    "这",
    "他",
    "她",
    "它",
    "们",
    "那",
    "些",
    "可以",
    "什么",
    "用",
    "能",
    "如何",
    "通过",
    "进行",
    "以及",
    "或者",
    "但是",
    "因为",
    "所以",
    "如果",
    "而",
    "与",
    "将",
    "对",
    "把",
    "从",
    "被",
    "比",
    "为",
    "等",
    "该",
    "其",
    "中",
    "更",
    "之",
    "及",
    "目前",
    "哪些",
    "分别",
    "他们",
    "它们",
    "怎么",
    "请问",
    "方面",
    "类型",
    "区别",
    "联系",
    "问题",
    "面上",
    "别是",
    "么类",
    "前市",
    "优缺",
    "缺点",
    "优点",
    "什么",
    "有什么",
    "有哪些",
    "优缺点",
    "市面上",
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
        // 必须按 Unicode 字符数过滤（不能用 UTF-8 字节数 .len() >= 2，否则单个汉字 3 字节全部混入）
        if token.chars().count() >= 2 {
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

/// 确定性意图识别（零 LLM 延迟，v0.9.94）：
/// 区分“开放式行业/产品谱系调研问句”与“具体技术构想的创新性验证”。
pub fn detect_query_intent(title: &str, description: &str) -> QueryIntent {
    let combined = format!("{} {}", title.trim(), description.trim());
    let lower = combined.to_ascii_lowercase();

    // 若文本中已含明确的专利方案构造特征，优先走创新验证与布局路径
    const INVENTION_SIGNALS: &[&str] = &[
        "本发明",
        "本实用新型",
        "权利要求",
        "其特征在于",
        "本发明提出",
        "提出一种",
        "设计了一种",
        "一种用于",
        "一种基于",
        "通过将",
        "进而降低",
        "进而提高",
    ];
    let invention_hits = INVENTION_SIGNALS
        .iter()
        .filter(|s| combined.contains(**s))
        .count();

    const SURVEY_SIGNALS: &[&str] = &[
        "有哪些",
        "什么类型",
        "哪些类型",
        "分别是什么",
        "优缺点",
        "优劣对比",
        "市面上",
        "市场上",
        "市场现状",
        "行业现状",
        "竞品分析",
        "产品谱系",
        "主流方案",
        "主流路线",
        "技术路线有哪些",
        "发展趋势",
        "调研",
        "综述",
        "what types",
        "pros and cons",
        "market landscape",
        "state of the art",
        "survey of",
    ];
    let survey_hits = SURVEY_SIGNALS
        .iter()
        .filter(|s| combined.contains(**s) || lower.contains(**s))
        .count();

    if (survey_hits >= 1 && invention_hits == 0)
        || (survey_hits >= 2 && survey_hits > invention_hits)
    {
        QueryIntent::MarketLandscapeSurvey
    } else {
        QueryIntent::InventionValidation
    }
}

pub async fn execute(ctx: &mut PipelineContext, db: &Arc<Database>) -> Result<()> {
    let full_text = format!("{} {}", ctx.title, ctx.description);
    ctx.keywords = extract_keywords(&full_text, 20);
    ctx.technical_domain = infer_domain(&ctx.keywords);
    ctx.query_intent = detect_query_intent(&ctx.title, &ctx.description);

    ctx.research_state.current_hypothesis = match ctx.query_intent {
        QueryIntent::MarketLandscapeSurvey => format!(
            "梳理「{}」在{}领域的主流产品分类、技术路线优缺点与差异化研发切入点",
            ctx.title, ctx.technical_domain
        ),
        QueryIntent::InventionValidation => format!(
            "创意「{}」在{}领域可能具有新颖性",
            ctx.title, ctx.technical_domain
        ),
    };

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

    #[test]
    fn detect_query_intent_separates_market_survey_from_concrete_invention() {
        assert_eq!(
            detect_query_intent(
                "微压氧仓",
                "目前市面上微压氧仓有哪些，分别是什么类型，他们的优缺点有什么？"
            ),
            QueryIntent::MarketLandscapeSurvey
        );
        assert_eq!(
            detect_query_intent(
                "固态电解质路线调研",
                "What types of solid-state battery electrolytes exist and their pros and cons?"
            ),
            QueryIntent::MarketLandscapeSurvey
        );
        assert_eq!(
            detect_query_intent(
                "800V SiC双面微通道液冷母排",
                "本发明提出一种用于800V电驱的双面直接水冷叠层母排，其特征在于将铜排直接作为微通道冷板壁面，进而降低结壳热阻32%。"
            ),
            QueryIntent::InventionValidation
        );
    }
}
