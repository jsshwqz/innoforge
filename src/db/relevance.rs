// ── Relevance scoring functions ──────────────────────────────────────────────

/// Unified relevance scoring for a single field (applicant or inventor).
pub(crate) fn calculate_field_relevance(
    query: &str,
    field_value: &str,
    field_name: &str,
) -> (f64, String) {
    let q = query.trim().to_lowercase();
    let f = field_value.trim().to_lowercase();

    // Exact match
    if q == f || q.replace(' ', "") == f.replace(' ', "") {
        return (100.0, format!("{field_name} exact match"));
    }

    // Prefix match (for applicant)
    if field_name == "applicant" && f.starts_with(&q) {
        return (95.0, format!("{field_name} prefix match"));
    }

    // Contains match
    if f.contains(&q) {
        return (90.0, format!("{field_name} contains match"));
    }

    // Chinese character matching (for inventor)
    if field_name == "inventor" {
        let q_chars: Vec<char> = q.chars().filter(|c| *c > '\u{7F}').collect();
        let f_chars: Vec<char> = f.chars().filter(|c| *c > '\u{7F}').collect();
        if !q_chars.is_empty() && !f_chars.is_empty() {
            // Surname match
            if q_chars.first() == f_chars.first() && (q_chars.len() <= 2 || f_chars.len() <= 2) {
                return (85.0, "surname match".to_string());
            }
            if q_chars.iter().all(|qc| f_chars.contains(qc)) {
                return (80.0, format!("{field_name} name contains"));
            }
        }
    }

    // Word-level matching
    let q_words: Vec<&str> = q
        .split(|c: char| c.is_whitespace() || c == ',' || c == '.')
        .filter(|s| !s.is_empty())
        .collect();
    let f_words: Vec<&str> = f
        .split(|c: char| c.is_whitespace() || c == ',' || c == '.')
        .filter(|s| !s.is_empty())
        .collect();

    let mut matched_words = 0;
    for qw in &q_words {
        for fw in &f_words {
            if fw.contains(qw) || qw.contains(fw) {
                matched_words += 1;
                break;
            }
        }
    }

    if !q_words.is_empty() {
        let match_ratio = matched_words as f64 / q_words.len() as f64;
        if match_ratio > 0.0 {
            let score = 50.0 + (match_ratio * 40.0);
            return (
                score,
                format!("{field_name} word match ({:.0}%)", match_ratio * 100.0),
            );
        }
    }

    (30.0, format!("{field_name} fuzzy match"))
}

/// Calculate mixed search relevance score.
pub(crate) fn calculate_mixed_relevance(
    query: &str,
    applicant: &str,
    inventor: &str,
    title: &str,
) -> f64 {
    let q = query.trim().to_lowercase();

    let (applicant_score, _) = calculate_field_relevance(query, applicant, "applicant");
    if applicant_score >= 90.0 {
        return applicant_score;
    }

    let (inventor_score, _) = calculate_field_relevance(query, inventor, "inventor");
    if inventor_score >= 90.0 {
        return inventor_score;
    }

    // Title match — exact/prefix/contains
    let t = title.trim().to_lowercase();
    if t == q {
        return 95.0;
    }
    if t.starts_with(&q) {
        return 85.0;
    }
    if t.contains(&q) {
        return 75.0;
    }

    // Word-level matching for space-separated queries (English / mixed)
    let q_words: Vec<&str> = q.split_whitespace().filter(|w| w.len() > 1).collect();
    if !q_words.is_empty() {
        let matches = q_words.iter().filter(|w| t.contains(*w)).count();
        let ratio = matches as f64 / q_words.len() as f64;
        if ratio >= 0.5 {
            return 50.0 + ratio * 30.0;
        }
    }

    // Chinese bigram matching — "电池热管理" vs "锂电池的热管理系统"
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
        if t_chars.len() >= 2 {
            let t_bigrams: Vec<String> = t_chars.windows(2).map(|w| w.iter().collect()).collect();
            let matches = q_bigrams.iter().filter(|bg| t_bigrams.contains(bg)).count();
            let ratio = matches as f64 / q_bigrams.len() as f64;
            if ratio >= 0.3 {
                return 45.0 + ratio * 35.0;
            }
        }
    }

    applicant_score.max(inventor_score).max(40.0)
}

/// 形似人名判定——**仅用于 name_like 标注，不参与检索路由**。
///
/// # issue #72 — 字符数而非字节数
/// 旧实现用 `q.len()`（UTF-8 字节数）做边界：2 个汉字 = 6 字节恰好落进
/// `2..=6` 的姓名区间。现改为 `chars().count()`（字符数），3 字姓名「张三丰」
/// （9 字节）不再被误拒。
///
/// # issue #72 v2 — 不再当路由门
/// 旧实现把本函数当作 `detect_search_type` 的**排除门**：判姓名 ⇒ 只查 inventor ⇒
/// 技术词零命中。但技术词首字与常见姓高度重合（高温/金属/白色/向心 的 高/金/白/向
/// 均为常见姓），姓氏表无论怎么调都是按测试词拟合，违反引擎通用性。现本函数只表达
/// 「查询在形状上像人名」，供 0 命中时给用户「可尝试 search_type=inventor」的提示；
/// 路由一律走 Mixed，姓名召回由 Mixed 内含的 inventor 字段搜索保证。
pub(crate) fn is_likely_name(query: &str) -> bool {
    let q = query.trim();
    let char_count = q.chars().count();
    if !(2..=50).contains(&char_count) {
        return false;
    }
    is_chinese_name(q) || is_english_name(q)
}

/// 形似中文姓名——「短且纯中文」。
///
/// # issue #72 v2 — 不校验姓氏
/// 旧实现要求首字为常见姓氏以排除技术词，但技术词首字与常见姓高度重合
/// （高/金/白/向…），姓氏表是按测试词拟合，违反通用性。本函数现在只判**形状**
/// （2–4 字、纯 CJK），供 [`is_likely_name`] 做 name_like 标注；**不参与路由**，
/// 因此把「相变」「高温」判为 name_like 形状是无害的——路由一律 Mixed。
fn is_chinese_name(query: &str) -> bool {
    let q = query.trim();
    // 用字符数而非字节数做边界——2 个汉字 = 6 字节，会错误落入旧
    // `q.len() <= 6` 的姓名区间（issue #72）。
    let char_count = q.chars().count();
    if !(2..=4).contains(&char_count) {
        return false;
    }
    // 必须全是 CJK 汉字、无空格/数字/拉丁
    q.chars().all(|c| ('\u{4E00}'..='\u{9FFF}').contains(&c))
}

fn is_english_name(query: &str) -> bool {
    let words: Vec<&str> = query.split_whitespace().collect();
    if words.len() < 2 || words.len() > 5 {
        return false;
    }
    let capitalized_count = words
        .iter()
        .filter(|w| w.chars().next().map(|c| c.is_uppercase()).unwrap_or(false))
        .count();
    capitalized_count >= words.len().saturating_sub(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// # issue #72 v2 — is_likely_name 是形状 hint，不校验姓氏、不参与路由。
    /// 技术词（相变/高温/金属/白色/向心）首字虽为常见姓，判为 name_like 形状是无害的：
    /// 路由一律 Mixed，不会因此只查 inventor。
    #[test]
    fn name_like_hint_is_shape_based_not_surname_based() {
        // 技术词：2 字纯 CJK ⇒ name_like 形状为真（仅 hint，不路由）
        assert!(is_likely_name("相变"));
        assert!(is_likely_name("高温"));
        assert!(is_likely_name("金属"));
        assert!(is_likely_name("白色"));
        assert!(is_likely_name("向心"));
        // 真实姓名同样为真
        assert!(is_likely_name("张三"));
        assert!(is_likely_name("王明"));
        assert!(is_likely_name("张三丰"));
    }

    /// # issue #72 — 字符数而非字节数：3 字姓名（9 字节）旧实现会漏判
    #[test]
    fn three_char_chinese_name_passes_char_count_bound() {
        assert!(is_chinese_name("张三丰"));
    }
}
