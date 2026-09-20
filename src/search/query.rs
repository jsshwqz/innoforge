//! 检索语句渲染 / Query rendering（MA1）
//!
//! spec §3 改造点 2 要求「q 构造统一由 [`SearchQuery`] 渲染」。本模块把原先散在
//! `routes/mod.rs::build_online_query` 与 `routes/search.rs::api_search_online` 里的
//! q 串 / 国内外判定逻辑集中到一处，**渲染规则逐字保持**：
//! 同一条用户输入产出的 `q` 串与 URL 参数必须与迁移前完全相同（由 `query_url_golden_*` 单测锁死）。

use crate::search::model::{Lang, SearchQuery};
use crate::types::search::SearchType;

/// 中国专利申请号形态：10~15 位数字（可含一个小数点校验位）。
/// 逐字迁自 `routes/search.rs` 的 `looks_like_cn_patent_number` 判定。
fn looks_like_cn_patent_number(query_trimmed: &str) -> bool {
    let digits_only: String = query_trimmed
        .chars()
        .filter(|c| c.is_ascii_digit())
        .collect();
    digits_only.len() >= 10
        && digits_only.len() <= 15
        && query_trimmed
            .chars()
            .all(|c| c.is_ascii_digit() || c == '.')
}

/// 是否含中日韩统一表意文字（旧 `api_search_online` 的内联判定）。
fn contains_cjk_char(s: &str) -> bool {
    s.chars()
        .any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c))
}

/// 搜索区域判定：用户明确选择 > 自动检测（迁自 `routes/search.rs`）。
///
/// 返回值收敛为 [`Lang`]：`Chinese` ⇔ 旧 `is_cn_query`，`English` ⇔ 旧 `is_intl_query`
/// （旧代码里两者在所有分支上互补，故一个枚举即可无损替代）。
/// `Lang::All` 目前只在 MA2 的显式「不限语种」入口使用。
pub fn resolve_lang(region: Option<&str>, country: Option<&str>, query_trimmed: &str) -> Lang {
    let auto_cn = matches!(country, Some("CN"))
        || query_trimmed.starts_with("CN")
        || query_trimmed.starts_with("ZL")
        || looks_like_cn_patent_number(query_trimmed)
        || contains_cjk_char(query_trimmed);
    match region {
        Some("cn") => Lang::Chinese, // 用户明确选国内
        Some("intl") => Lang::English, // 用户明确选国外
        _ => {
            if auto_cn {
                Lang::Chinese
            } else {
                Lang::English
            }
        }
    }
}

/// 由 [`SearchQuery`] 渲染在线引擎的 `q` 串（Google Patents / SerpAPI 共用同一套语法）。
///
/// 函数体逐字迁自 `src/routes/mod.rs::build_online_query`，仅把四个散装参数换成 SearchQuery；
/// 引号剥离、CN 申请号取核心位、`OR` 双写、`after:`/`before:` 追加顺序均未改动。
pub fn render_q(query: &SearchQuery) -> String {
    let q = query.keyword.trim().replace('"', "");
    let mut search_query = match query.search_type.as_ref() {
        Some(SearchType::Applicant) => format!("assignee:\"{}\"", q),
        Some(SearchType::Inventor) => format!("inventor:\"{}\"", q),
        Some(SearchType::PatentNumber) => {
            // For Chinese application numbers (e.g. "CN202420009882.7" or "202210835143.9"),
            // Google Patents indexes by PUBLICATION number, not application number.
            // CN application number format: YYYYMMNNNNNN.X (12 digits + check digit)
            let digits: String = q.chars().filter(|c| c.is_ascii_digit()).collect();
            let has_dot = q.contains('.');
            let is_cn_app = digits.len() >= 10
                && digits.len() <= 15
                && (q.chars().all(|c| c.is_ascii_digit() || c == '.')
                    || (q.starts_with("CN") && q.contains('.')));
            if is_cn_app {
                // If the original query has a dot (e.g. "202210835143.9"),
                // strip the check digit after dot → use 12-digit core number.
                // If no dot but 13 digits, the last digit is likely the check digit.
                let core = if has_dot {
                    // Take only digits before the dot position
                    let dot_pos = q.find('.').unwrap_or(q.len());
                    let pre_dot: String = q[..dot_pos]
                        .chars()
                        .filter(|c| c.is_ascii_digit())
                        .collect();
                    pre_dot
                } else if digits.len() == 13 {
                    // 13 digits without dot: last digit is check digit
                    digits[..12].to_string()
                } else {
                    digits
                };
                // 同时保留原始申请号与核心位数，提升 SerpAPI 在不同索引形态下的命中率
                format!("\"{}\" OR \"{}\"", q, core)
            } else {
                format!("\"{}\"", q)
            }
        }
        _ => q,
    };
    if let Some(from) = query.date_from.as_deref() {
        if !from.is_empty() {
            search_query.push_str(&format!(" after:{from}"));
        }
    }
    if let Some(to) = query.date_to.as_deref() {
        if !to.is_empty() {
            search_query.push_str(&format!(" before:{to}"));
        }
    }
    search_query
}

/// 旧位置签名（`routes/mod.rs::build_online_query`）的兼容包装：
/// 只做参数装箱，随后交给 [`render_q`]，确保两条入口渲染结果恒等。
pub fn build_online_query(
    query: &str,
    search_type: Option<&SearchType>,
    date_from: Option<&str>,
    date_to: Option<&str>,
) -> String {
    render_q(&SearchQuery {
        keyword: query.to_string(),
        country: None,
        language: None,
        assignee: None,
        exact_assignee: false,
        date_from: date_from.map(|s| s.to_string()),
        date_to: date_to.map(|s| s.to_string()),
        limit: 0,
        page: 1,
        sort_by: None,
        search_type: search_type.cloned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::model::Lang;

    /// 只填 keyword/search_type，其余走「全 None / 空」基线（不用 `SearchQuery::default()`，
    /// 因为默认值按 spec §1 是 CN + 中文，会干扰本模块的渲染断言）。
    fn blank() -> SearchQuery {
        SearchQuery {
            keyword: String::new(),
            country: None,
            language: None,
            assignee: None,
            exact_assignee: false,
            date_from: None,
            date_to: None,
            limit: 0,
            page: 1,
            sort_by: None,
            search_type: None,
        }
    }

    fn q(keyword: &str, search_type: Option<SearchType>) -> SearchQuery {
        SearchQuery {
            keyword: keyword.to_string(),
            search_type,
            ..blank()
        }
    }

    /// 兼容包装与 SearchQuery 渲染必须恒等（spec §3 改造点 2 的「行为保持」面）。
    #[test]
    fn legacy_wrapper_and_render_q_are_identical() {
        let cases: &[(&str, Option<SearchType>, Option<&str>, Option<&str>)] = &[
            ("Alice Zhang", Some(SearchType::Applicant), None, None),
            ("张三", Some(SearchType::Inventor), Some("20200101"), None),
            ("CN202420009882.7", Some(SearchType::PatentNumber), None, Some("20250101")),
            ("202210835143.9", Some(SearchType::PatentNumber), Some("20190101"), Some("20240101")),
            ("固态 电池", None, None, None),
            ("带\"引号\"的查询", Some(SearchType::Keyword), None, None),
        ];
        for (keyword, st, from, to) in cases {
            let legacy = build_online_query(keyword, st.as_ref(), *from, *to);
            let rendered = render_q(&q(
                keyword,
                st.clone(),
            ));
            assert_eq!(legacy, rendered, "keyword={keyword}");
        }
    }

    /// 旧代码的具体渲染结果（golden），证明迁移没顺手改语法。
    #[test]
    fn render_q_golden_cases() {
        assert_eq!(
            "assignee:\"Alice Zhang\"",
            render_q(&q("Alice Zhang", Some(SearchType::Applicant)))
        );
        assert_eq!(
            "inventor:\"张三\" after:20200101",
            render_q(&SearchQuery {
                keyword: "张三".to_string(),
                search_type: Some(SearchType::Inventor),
                date_from: Some("20200101".to_string()),
                ..q("", None)
            })
        );
        // 13 位无点申请号：末位校验位被剥掉，双写原始值与核心值
        assert_eq!(
            "\"2022108351439\" OR \"202210835143\"",
            render_q(&q("2022108351439", Some(SearchType::PatentNumber)))
        );
        assert_eq!(
            "\"CN202420009882.7\" OR \"202420009882\"",
            render_q(&q("CN202420009882.7", Some(SearchType::PatentNumber)))
        );
        assert_eq!("\"iPhone 15\"", render_q(&q("iPhone 15", Some(SearchType::Keyword))));
        assert_eq!("iPhone 15", render_q(&q("iPhone 15", None)));
    }

    /// 国内/国外判定与旧 `auto_cn`/`region` 分支等价。
    #[test]
    fn resolve_lang_matches_legacy_region_flags() {
        // 用户明确选择优先于自动检测
        assert_eq!(
            Lang::English,
            resolve_lang(Some("intl"), Some("CN"), "固态电池")
        );
        assert_eq!(Lang::Chinese, resolve_lang(Some("cn"), None, "hinge"));
        // 自动检测：country=CN / CN 前缀 / ZL 前缀 / 纯申请号 / 中文字符
        assert_eq!(Lang::Chinese, resolve_lang(None, Some("CN"), "hinge"));
        assert_eq!(Lang::Chinese, resolve_lang(None, None, "CN10A"));
        assert_eq!(Lang::Chinese, resolve_lang(None, None, "ZL2024xxx"));
        assert_eq!(Lang::Chinese, resolve_lang(None, None, "202210835143.9"));
        assert_eq!(Lang::Chinese, resolve_lang(None, None, "固态电池"));
        // 非中文且像专利号但位数不足 → 国外
        assert_eq!(Lang::English, resolve_lang(None, Some("US"), "1234567"));
        assert_eq!(Lang::English, resolve_lang(None, None, "dustproof hinge"));
    }
}
