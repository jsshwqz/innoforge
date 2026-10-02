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

/// Detect if input is likely a person's name.
pub(crate) fn is_likely_name(query: &str) -> bool {
    let q = query.trim();
    if q.is_empty() || q.len() < 2 || q.len() > 50 {
        return false;
    }
    is_chinese_name(q) || is_english_name(q)
}

fn is_chinese_name(query: &str) -> bool {
    let q = query.trim();
    if q.len() < 2 || q.len() > 6 {
        return false;
    }
    let chinese_chars = q.chars().filter(|c| *c > '\u{7F}').count();
    let total_chars = q.chars().count();
    total_chars > 0 && (chinese_chars as f64 / total_chars as f64) >= 0.8 && !q.contains(' ')
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
