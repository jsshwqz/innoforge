//! 在线检索相关性判定 / Online relevance scoring（MA1 抽出）
//!
//! 三个函数逐字迁自 `src/routes/search.rs`（原私有），供 SerpAPI provider 与后续 MA2 多源
//! 合并复用。判定阈值（45/62/40）、停用词表、bigram 权重全部未改，因此同一条 (query, 结果)
//! 的打分与放行结论与迁移前逐字一致。
//!
//! 注意：`src/pipeline/steps/search.rs::contains_cjk` 是 types-migration-map.md §4 第 3 项登记的
//! 另一份实现，但它**多覆盖了扩展 A 区 `U+3400..U+4DBF`**，与本函数并非同形；
//! 归并会改变流水线分支走向，属 T2.2 的裁决项，MA1 不动。

/// 检查字符串是否包含中文字符
pub fn contains_cjk(s: &str) -> bool {
    s.chars().any(|c| ('\u{4E00}'..='\u{9FFF}').contains(&c))
}

/// 计算文本相似度分数（用于在线搜索排序和过滤）
pub fn calculate_online_relevance(
    query: &str,
    title: &str,
    abstract_text: &str,
    applicant: &str,
    inventor: &str,
) -> f64 {
    let q = query.trim().to_lowercase();
    let t = title.trim().to_lowercase();
    let a = abstract_text.trim().to_lowercase();
    let app = applicant.trim().to_lowercase();
    let inv = inventor.trim().to_lowercase();

    let mut score = 30.0;

    // Title matching (most important, max +50)
    if t == q {
        score += 50.0;
    } else if t.contains(&q) {
        score += 35.0;
    } else {
        // Word-level matching in title
        let q_words: Vec<&str> = q.split_whitespace().filter(|w| w.len() > 1).collect();
        if !q_words.is_empty() {
            let matches = q_words.iter().filter(|w| t.contains(*w)).count();
            score += (matches as f64 / q_words.len() as f64) * 30.0;
        }
        // Chinese bigram matching
        let q_chars: Vec<char> = q
            .chars()
            .filter(|c| ('\u{4E00}'..='\u{9FFF}').contains(c))
            .collect();
        if q_chars.len() >= 2 {
            let q_bigrams: Vec<String> = q_chars.windows(2).map(|w| w.iter().collect()).collect();
            let t_chars: Vec<char> = t
                .chars()
                .filter(|c| ('\u{4E00}'..='\u{9FFF}').contains(c))
                .collect();
            let t_bigrams: Vec<String> = if t_chars.len() >= 2 {
                t_chars.windows(2).map(|w| w.iter().collect()).collect()
            } else {
                vec![]
            };
            if !q_bigrams.is_empty() && !t_bigrams.is_empty() {
                let matches = q_bigrams.iter().filter(|bg| t_bigrams.contains(bg)).count();
                score += (matches as f64 / q_bigrams.len() as f64) * 25.0;
            }
        } else if !q_chars.is_empty() {
            let matches = q_chars.iter().filter(|c| t.contains(**c)).count();
            score += (matches as f64 / q_chars.len() as f64) * 20.0;
        }
    }

    // Abstract matching (secondary, max +15)
    if a.contains(&q) {
        score += 15.0;
    } else {
        let q_words: Vec<&str> = q.split_whitespace().filter(|w| w.len() > 1).collect();
        if !q_words.is_empty() {
            let matches = q_words.iter().filter(|w| a.contains(*w)).count();
            score += (matches as f64 / q_words.len() as f64) * 10.0;
        }
        let q_chars: Vec<char> = q
            .chars()
            .filter(|c| ('\u{4E00}'..='\u{9FFF}').contains(c))
            .collect();
        if q_chars.len() >= 2 {
            let q_bigrams: Vec<String> = q_chars.windows(2).map(|w| w.iter().collect()).collect();
            let a_chars: Vec<char> = a
                .chars()
                .filter(|c| ('\u{4E00}'..='\u{9FFF}').contains(c))
                .collect();
            let a_bigrams: Vec<String> = if a_chars.len() >= 2 {
                a_chars.windows(2).map(|w| w.iter().collect()).collect()
            } else {
                vec![]
            };
            if !q_bigrams.is_empty() && !a_bigrams.is_empty() {
                let matches = q_bigrams.iter().filter(|bg| a_bigrams.contains(bg)).count();
                score += (matches as f64 / q_bigrams.len() as f64) * 8.0;
            }
        }
    }

    // Applicant matching (bonus, max +5)
    if app.contains(&q) {
        score += 5.0;
    }

    // Inventor matching (bonus, max +15) — 用于中文发明人姓名搜索
    if inv.contains(&q) || q.contains(&inv) {
        score += 15.0;
    }

    score.min(100.0)
}

/// 判断在线搜索结果的关联性，发明人姓名匹配时直接放行
pub fn is_online_result_relevant(
    query: &str,
    title: &str,
    abstract_text: &str,
    content_score: f64,
    is_cn_query: bool,
    inventor: &str,
) -> bool {
    let q = query.trim();
    if q.is_empty() {
        return false;
    }
    let t = title.to_lowercase();
    let a = abstract_text.to_lowercase();
    let ql = q.to_lowercase();

    // 发明人姓名直接匹配：查询词命中的发明人姓名，直接放行
    let inv_lower = inventor.to_lowercase();
    if !inv_lower.is_empty()
        && q.chars()
            .all(|c| c.is_ascii_alphabetic() || c.is_whitespace())
    {
        // 英文姓名：双向包含检查（查询包含发明人，或发明人包含查询）
        if inv_lower.contains(&ql) || ql.contains(&inv_lower) {
            return true;
        }
    }
    // 中文姓名/拼音：查询词中的每个字都出现在发明人字段中
    if !inv_lower.is_empty() {
        let q_clean: String = q.chars().filter(|c| !c.is_ascii_punctuation()).collect();
        if q_clean.chars().all(|c| {
            c.is_ascii_alphabetic() || c.is_ascii_digit() || ('\u{4E00}'..='\u{9FFF}').contains(&c)
        }) && q_clean.len() >= 2
        {
            let all_in_inventor = q_clean.chars().all(|c| inv_lower.contains(c));
            if all_in_inventor {
                return true;
            }
        }
    }

    // 直接匹配优先保留
    if t.contains(&ql) || a.contains(&ql) {
        return true;
    }

    // 中文查询：门槛更高，避免无关英文噪声
    if is_cn_query {
        if contains_cjk(title) || contains_cjk(abstract_text) {
            return content_score >= 45.0;
        }
        return content_score >= 62.0;
    }

    // 英文/国际查询：多词技术查询至少命中两个查询词
    let query_terms: Vec<&str> = ql
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| w.len() >= 3)
        .filter(|w| {
            !matches!(
                *w,
                "patent"
                    | "device"
                    | "method"
                    | "system"
                    | "apparatus"
                    | "mobile"
                    | "phone"
                    | "electronic"
            )
        })
        .collect();
    if query_terms.len() >= 2 {
        let haystack = format!("{t} {a}");
        let matched = query_terms
            .iter()
            .filter(|term| haystack.contains(**term))
            .count();
        let required_matches = if query_terms.len() <= 3 {
            query_terms.len()
        } else {
            (query_terms.len() * 2).div_ceil(3)
        };
        if matched < required_matches {
            return false;
        }
    }

    content_score >= 40.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intl_relevance_rejects_generic_phone_results_for_specific_hinge_query() {
        let query = "foldable phone hinge dustproof patent";
        assert!(!is_online_result_relevant(
            query,
            "mobile phone",
            "A mobile phone includes a touch screen and a camera.",
            42.0,
            false,
            "",
        ));
    }

    #[test]
    fn intl_relevance_rejects_results_missing_specific_constraint_terms() {
        let query = "foldable phone hinge dustproof patent";
        assert!(!is_online_result_relevant(
            query,
            "mobile phone",
            "A foldable mobile communication terminal has a biaxial hinge device.",
            60.0,
            false,
            "",
        ));
    }

    #[test]
    fn intl_relevance_keeps_specific_hinge_results() {
        let query = "foldable phone hinge dustproof patent";
        assert!(is_online_result_relevant(
            query,
            "Dustproof hinge for a foldable electronic device",
            "The hinge blocks dust ingress while the foldable phone opens and closes.",
            52.0,
            false,
            "",
        ));
    }

    /// 中文查询的阈值分档（45 / 62）是旧代码行为，锁死防止「迁移顺手调参」。
    /// 用例特意让标题不含完整查询词，否则会在「直接匹配优先保留」分支提前返回。
    #[test]
    fn cn_thresholds_are_preserved() {
        // 标题含中文 → 门槛 45
        assert!(is_online_result_relevant(
            "固态电池",
            "一种新型电池结构",
            "",
            45.0,
            true,
            "张三",
        ));
        assert!(!is_online_result_relevant(
            "固态电池",
            "一种新型电池结构",
            "",
            44.9,
            true,
            "张三",
        ));
        // 标题无中文 → 门槛抬到 62
        assert!(is_online_result_relevant(
            "固态电池",
            "Solid State Cell",
            "",
            62.0,
            true,
            "",
        ));
        assert!(!is_online_result_relevant(
            "固态电池",
            "Solid State Cell",
            "",
            61.9,
            true,
            "",
        ));
    }

    /// 发明人命中直接放行（PRD N2 的姓名精确检索锚点）。
    #[test]
    fn inventor_match_bypasses_score_gate() {
        assert!(is_online_result_relevant(
            "Zhang San",
            "unrelated title",
            "nothing here",
            5.0,
            false,
            "Zhang San",
        ));
        assert!(!is_online_result_relevant(
            "Zhang San",
            "unrelated title",
            "nothing here",
            5.0,
            false,
            "Li Si",
        ));
    }

    /// 打分上限 100 与基础分 30 未变。
    /// （基础分用例特意给非空 inventor：旧代码对空 inventor 会因 `q.contains("")` 恒真而加 15 分，
    /// 那是既有行为，不在 MA1 改动。）
    #[test]
    fn score_bounds_are_preserved() {
        assert_eq!(
            30.0,
            calculate_online_relevance("abc", "xyz", "", "", "zzz")
        );
        assert_eq!(
            100.0,
            calculate_online_relevance("固态电池", "固态电池", "固态电池", "固态电池", "固态电池")
        );
        // 空 inventor 的既有行为：`q.contains(&inv)` 对空串恒真 → +15
        assert_eq!(45.0, calculate_online_relevance("abc", "xyz", "", "", ""));
    }
}
