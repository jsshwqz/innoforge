//! Step 9: DetectContradictions — 检测现有技术间的矛盾信号
//!
//! 类型：CODE
//!
//! 矛盾 = 两个结果都与用户创意相关，但彼此之间差异很大
//! 这意味着该领域技术路线尚未收敛，存在创新空间

use crate::pipeline::context::{Contradiction, Evidence, PipelineContext};
use crate::pipeline::steps::text_util::jaccard;
use anyhow::Result;
use std::collections::HashSet;

/// 判断词元是否属于专利号、日期、抓取元数据或无意义中文虚词碎片
pub(crate) fn is_noise_diff_token(t: &str) -> bool {
    if t.chars().count() < 2 {
        return true;
    }
    let lower = t.to_ascii_lowercase();
    const NOISE_META: &[&str] = &[
        "abstract",
        "translated",
        "classifications",
        "machine-classified",
        "legal",
        "events",
        "date",
        "code",
        "title",
        "application",
        "filed",
        "granted",
        "publication",
        "priority",
        "patent",
        "patents",
        "claims",
        "description",
        "status",
        "active",
        "pending",
        "expired",
        "assignee",
        "inventor",
        "google",
        "http",
        "https",
        "www",
        "com",
        "一种",
        "本发",
        "发明",
        "明提",
        "提供",
        "器本",
        "种基",
        "基于",
        "用于",
        "包括",
        "所述",
        "设有",
        "及其",
        "方法",
        "系统",
        "装置",
        "设备",
        "以及",
        "具有",
        "通过",
        "种微",
        "种高",
        "的控",
        "舱的",
        "于迭",
        "公开",
        "涉及",
        "技术",
        "领域",
    ];
    if NOISE_META.contains(&lower.as_str()) {
        return true;
    }
    // 纯数字或日期（如 2024-08-05）
    if lower
        .chars()
        .all(|c| c.is_ascii_digit() || c == '-' || c == '/' || c == '.')
    {
        return true;
    }
    // 专利号 / 公开号（如 cn990001001a、us1234567b）
    let bytes = lower.as_bytes();
    if bytes.len() >= 7
        && bytes[0].is_ascii_alphabetic()
        && bytes[1].is_ascii_alphabetic()
        && bytes[2..].iter().filter(|b| b.is_ascii_digit()).count() >= 5
    {
        return true;
    }
    // 以常见中文虚词/碎片开头或结尾的 2 字滑窗碎片
    const EDGE_STOP_CHARS: &[char] = &[
        '的', '了', '在', '是', '与', '和', '及', '于', '种', '本', '所', '其', '该', '为', '以',
        '对', '将', '把', '或', '而', '之', '并', '且', '由', '从', '中',
    ];
    if t.chars().count() == 2 {
        let mut chs = t.chars();
        if let (Some(first), Some(last)) = (chs.next(), chs.next()) {
            if EDGE_STOP_CHARS.contains(&first) || EDGE_STOP_CHARS.contains(&last) {
                return true;
            }
        }
    }
    false
}

/// 提取两个结果之间的差异关键词
fn difference_keywords(a: &[String], b: &[String]) -> Vec<String> {
    let set_b: HashSet<&String> = b.iter().collect();
    let mut seen: HashSet<&str> = HashSet::new();
    a.iter()
        .filter(|t| !set_b.contains(t) && !is_noise_diff_token(t) && seen.insert(t.as_str()))
        .take(5)
        .cloned()
        .collect()
}

/// 执行 Step 9
pub async fn execute(ctx: &mut PipelineContext) -> Result<()> {
    let matches = &ctx.top_matches;
    if matches.len() < 2 {
        return Ok(());
    }

    let mut contradictions = Vec::new();

    // 两两比较 Top 匹配结果
    let check_count = matches.len().min(10); // 最多检查前 10 个
    for i in 0..check_count {
        for j in (i + 1)..check_count {
            let a = &matches[i];
            let b = &matches[j];

            // 两者都与用户创意有一定相关性（combined_score > 0.05）
            if a.combined_score < 0.05 || b.combined_score < 0.05 {
                continue;
            }

            // 计算两者之间的相似度
            let set_a: HashSet<&String> = a.tokens.iter().collect();
            let set_b: HashSet<&String> = b.tokens.iter().collect();
            let mutual_sim = jaccard(&set_a, &set_b);

            // 如果两者之间相似度很低（< 0.15），说明技术路线不同
            if mutual_sim < 0.15 {
                let a_unique = difference_keywords(&a.tokens, &b.tokens);
                let b_unique = difference_keywords(&b.tokens, &a.tokens);

                let signal_strength = 1.0 - mutual_sim; // 差异越大，信号越强

                let dimension = if !a_unique.is_empty() && !b_unique.is_empty() {
                    format!("{} vs {}", a_unique.join("/"), b_unique.join("/"))
                } else {
                    "技术路线差异".into()
                };

                let opportunity = format!(
                    "「{}」与「{}」解决类似问题但采用不同技术路线，\
                     两者的差异区间可能存在未被探索的创新空间",
                    truncate_title(&a.source_title),
                    truncate_title(&b.source_title),
                );

                contradictions.push(Contradiction {
                    source_a: a.source_title.clone(),
                    source_b: b.source_title.clone(),
                    dimension,
                    signal_strength,
                    opportunity,
                });
            }
        }
    }

    // 按信号强度排序，取 Top 5
    contradictions.sort_by(|a, b| {
        b.signal_strength
            .partial_cmp(&a.signal_strength)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    contradictions.truncate(5);

    // 为每个矛盾生成证据 / Generate evidence for each contradiction
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    for c in &contradictions {
        ctx.evidence_chain.push(Evidence {
            id: uuid::Uuid::new_v4().to_string(),
            idea_id: ctx.idea_id.clone(),
            claim: format!(
                "技术路线矛盾：{} vs {} ({})",
                c.source_a, c.source_b, c.dimension
            ),
            source_type: "contradiction".to_string(),
            source_id: format!("{}|{}", c.source_a, c.source_b),
            source_title: format!("{} ↔ {}", c.source_a, c.source_b),
            source_url: String::new(),
            claim_number: None,
            excerpt: c.opportunity.clone(),
            relation: "contradicts".to_string(),
            confidence: c.signal_strength,
            produced_by: "DetectContradictions".to_string(),
            created_at: now.clone(),
        });
    }

    ctx.contradictions = contradictions;

    // 更新研发状态机：为每个矛盾添加去重后的开放问题
    for c in &ctx.contradictions {
        let q = format!(
            "{}与{}在{}维度存在矛盾，需进一步验证",
            c.source_a, c.source_b, c.dimension
        );
        if !ctx.research_state.open_questions.contains(&q) {
            ctx.research_state.open_questions.push(q);
        }
    }

    Ok(())
}

fn truncate_title(title: &str) -> String {
    if title.chars().count() > 20 {
        format!("{}...", title.chars().take(20).collect::<String>())
    } else {
        title.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn difference_keywords_filters_metadata_dates_patent_numbers_and_single_cjk() {
        let a: Vec<String> = vec![
            "一",
            "一种",
            "种基",
            "cn990001001a",
            "abstract",
            "translated",
            "machine-classified",
            "2024-08-05",
            "气动自紧密封",
            "微通道阻尼",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        let b: Vec<String> = vec!["传统拉链".to_string()];
        let diff = difference_keywords(&a, &b);
        assert_eq!(diff, vec!["气动自紧密封", "微通道阻尼"]);
    }
}
