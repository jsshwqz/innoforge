//! 检索语句渲染 / Query rendering（MA1）
//!
//! spec §3 改造点 2 要求「q 构造统一由 [`SearchQuery`] 渲染」。本模块把原先散在
//! `routes/mod.rs::build_online_query` 与 `routes/search.rs::api_search_online` 里的
//! q 串 / 国内外判定逻辑集中到一处，**渲染规则逐字保持**：
//! 同一条用户输入产出的 `q` 串与 URL 参数必须与迁移前完全相同。
//!
//! 「行为保持」的证明方式：本文件末尾的 `test_support` 里保留了**迁移前的两份实现**
//! （只编译于 test cfg），`render_q_matches_pre_migration_implementation` 与
//! `resolve_lang_matches_pre_migration_region_flags` 对同一批输入与之逐用例比对。
//! 旧入口 `routes/mod.rs::build_online_query` 已随之删除，全仓只剩这一份实现。

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
    s.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c))
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
        Some("cn") => Lang::Chinese,   // 用户明确选国内
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

/// `2024-01-05` / `2024/1/5` / `20240105` → `"20240105"`；认不出来则 `None`。
///
/// MA2a 时它是 `providers/google_patents_xhr.rs` 的私有函数；MA2b 接入 EPO OPS 时
/// 该源同样要把用户侧日期串归一为上游格式（CQL 的 `pd="YYYYMMDD YYYYMMDD"`），
/// 故按 AGENTS.md 2.2 上提到本模块单一出处，**函数体逐字未改**。
///
/// 用户侧日期串形态不统一（前端传 `YYYY-MM-DD`，历史数据里见过 `YYYY/M/D`），
/// 而调用方的处理原则是**归一失败当作「无边界」**（见各源的日期过滤 / CQL 渲染），
/// 所以这里宁可返回 `None` 也不要瞎猜一个数。
pub fn normalize_date(s: &str) -> Option<String> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return None;
    }
    // 纯数字形态：至少要有 8 位才可能是完整年月日
    if trimmed.chars().all(|c| c.is_ascii_digit()) {
        let digits: String = trimmed.chars().collect();
        return if digits.len() >= 8 {
            Some(digits[..8].to_string())
        } else {
            None
        };
    }
    // 分隔符形态：年-月-日 / 年/月/日 / 年.月.日，月日可不补零
    let parts: Vec<&str> = trimmed.split(['-', '/', '.']).collect();
    if parts.len() != 3 {
        return None;
    }
    let year = parts[0].parse::<u32>().ok()?;
    let month = parts[1].parse::<u32>().ok()?;
    let day = parts[2].parse::<u32>().ok()?;
    if !(1000..=9999).contains(&year) || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some(format!("{year:04}{month:02}{day:02}"))
}

/// 迁移前的 q 串渲染实现，**逐字符复制**自 `src/routes/mod.rs::build_online_query`
/// （`git show b08f1c5:src/routes/mod.rs`，原 443-500 行）。
///
/// 只在 `cfg(test)` 下编译，专供「改造前后等价」的参照对比：
/// - [`crate::search::query::render_q`] 对同一批输入的产出必须与之完全相同；
/// - `providers::serpapi` 的上游 URL 也基于它构造参照值。
///
/// 生产代码不得调用本函数（旧入口 `routes/mod.rs::build_online_query` 已删除，
/// 避免出现两份可独立演化的实现）。
#[cfg(test)]
pub mod test_support {
    use crate::types::search::SearchType;

    pub fn legacy_build_online_query(
        query: &str,
        search_type: Option<&SearchType>,
        date_from: Option<&str>,
        date_to: Option<&str>,
    ) -> String {
        let q = query.trim().replace('"', "");
        let mut search_query = match search_type {
            Some(SearchType::Applicant) => format!("assignee:\"{}\"", q),
            Some(SearchType::Inventor) => format!("inventor:\"{}\"", q),
            Some(SearchType::PatentNumber) => {
                let digits: String = q.chars().filter(|c| c.is_ascii_digit()).collect();
                let has_dot = q.contains('.');
                let is_cn_app = digits.len() >= 10
                    && digits.len() <= 15
                    && (q.chars().all(|c| c.is_ascii_digit() || c == '.')
                        || (q.starts_with("CN") && q.contains('.')));
                if is_cn_app {
                    let core = if has_dot {
                        let dot_pos = q.find('.').unwrap_or(q.len());
                        let pre_dot: String = q[..dot_pos]
                            .chars()
                            .filter(|c| c.is_ascii_digit())
                            .collect();
                        pre_dot
                    } else if digits.len() == 13 {
                        digits[..12].to_string()
                    } else {
                        digits
                    };
                    format!("\"{}\" OR \"{}\"", q, core)
                } else {
                    format!("\"{}\"", q)
                }
            }
            _ => q,
        };
        if let Some(from) = date_from {
            if !from.is_empty() {
                search_query.push_str(&format!(" after:{from}"));
            }
        }
        if let Some(to) = date_to {
            if !to.is_empty() {
                search_query.push_str(&format!(" before:{to}"));
            }
        }
        search_query
    }

    /// 迁移前的国内/国外判定，**逐字符复制**自 `src/routes/search.rs::api_search_online`
    /// （`git show b08f1c5:src/routes/search.rs`，原 248-276 行）。
    /// 返回旧代码的 `(is_cn_query, is_intl_query)` 二元组。
    pub fn legacy_region_flags(
        region: Option<&str>,
        country: Option<&str>,
        query_trimmed: &str,
    ) -> (bool, bool) {
        let looks_like_cn_patent_number = {
            let digits_only: String = query_trimmed
                .chars()
                .filter(|c| c.is_ascii_digit())
                .collect();
            digits_only.len() >= 10
                && digits_only.len() <= 15
                && query_trimmed
                    .chars()
                    .all(|c| c.is_ascii_digit() || c == '.')
        };
        let auto_cn = matches!(country, Some("CN"))
            || query_trimmed.starts_with("CN")
            || query_trimmed.starts_with("ZL")
            || looks_like_cn_patent_number
            || query_trimmed
                .chars()
                .any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c));

        let is_cn_query = match region {
            Some("cn") => true,
            Some("intl") => false,
            _ => auto_cn,
        };
        let is_intl_query = match region {
            Some("intl") => true,
            Some("cn") => false,
            _ => !auto_cn,
        };
        (is_cn_query, is_intl_query)
    }
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

    /// 一条 q 渲染用例：`(关键词, 检索域, date_from, date_to)`
    type RenderCase = (
        &'static str,
        Option<SearchType>,
        Option<&'static str>,
        Option<&'static str>,
    );

    /// 与**迁移前实现**逐字符等价（spec §3 改造点 2 的「行为保持」证据）。
    /// 覆盖自 `routes/mod.rs` 测试迁来的两条（applicant 域、inventor + 日期区间）。
    #[test]
    fn render_q_matches_pre_migration_implementation() {
        use crate::search::query::test_support::legacy_build_online_query;
        let cases: &[RenderCase] = &[
            ("Alice Zhang", Some(SearchType::Applicant), None, None),
            (
                "Alice Zhang",
                Some(SearchType::Inventor),
                Some("2024-01-01"),
                Some("2024-12-31"),
            ),
            ("张三", Some(SearchType::Inventor), Some("20200101"), None),
            (
                "CN202420009882.7",
                Some(SearchType::PatentNumber),
                None,
                Some("20250101"),
            ),
            (
                "202210835143.9",
                Some(SearchType::PatentNumber),
                Some("20190101"),
                Some("20240101"),
            ),
            ("2022108351439", Some(SearchType::PatentNumber), None, None),
            ("20221083", Some(SearchType::PatentNumber), None, None),
            ("固态 电池", None, None, None),
            ("带\"引号\"的查询", Some(SearchType::Keyword), None, None),
            (
                "  首尾空格  ",
                Some(SearchType::Applicant),
                Some(""),
                Some(""),
            ),
            ("", None, None, None),
        ];
        for (keyword, st, from, to) in cases {
            let legacy = legacy_build_online_query(keyword, st.as_ref(), *from, *to);
            let rendered = render_q(&SearchQuery {
                keyword: (*keyword).to_string(),
                search_type: st.clone(),
                date_from: (*from).map(|s| s.to_string()),
                date_to: (*to).map(|s| s.to_string()),
                ..blank()
            });
            assert_eq!(legacy, rendered, "keyword={keyword}");
        }
    }

    /// 单条 golden（含日期区间形态），钉住可读的具体产出。
    #[test]
    fn render_q_golden_cases() {
        assert_eq!(
            "inventor:\"Alice Zhang\" after:2024-01-01 before:2024-12-31",
            render_q(&SearchQuery {
                keyword: "Alice Zhang".to_string(),
                search_type: Some(SearchType::Inventor),
                date_from: Some("2024-01-01".to_string()),
                date_to: Some("2024-12-31".to_string()),
                ..q("", None)
            })
        );
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
        // Keyword / Mixed / 未指定 三种取值都不加检索语法（旧 `_ => q` 分支），
        // 只保留 trim + 去引号；`"iPhone 15"` 这类带引号的写法会被剥掉而非转义。
        assert_eq!(
            "iPhone 15",
            render_q(&q("  iPhone 15  ", Some(SearchType::Keyword)))
        );
        assert_eq!(
            "iPhone 15",
            render_q(&q("iPhone 15", Some(SearchType::Mixed)))
        );
        assert_eq!(
            "带iPhone 15的查询",
            render_q(&q("带\"iPhone 15\"的查询", Some(SearchType::Keyword)))
        );
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

    /// 与**迁移前判定**逐用例等价：`Lang::Chinese` ⇔ 旧 `is_cn_query`，
    /// `Lang::English` ⇔ 旧 `is_intl_query`（旧代码里两者在所有分支互补，一个枚举即可无损替代）。
    #[test]
    fn resolve_lang_matches_pre_migration_region_flags() {
        use crate::search::query::test_support::legacy_region_flags;
        let regions = [None, Some("cn"), Some("intl"), Some("other")];
        let countries = [None, Some("CN"), Some("US"), Some("")];
        let queries = [
            "",
            "  ",
            "hinge",
            "dustproof hinge",
            "固态电池",
            "CN1098765A",
            "ZL202410123456.7",
            "202210835143.9",
            "202210835143",
            "1234567890",
            "1234567890123456789",
            "CN202420009882.7",
            "混合 mixed 关键词",
            "日本語の特許",
        ];
        for region in regions {
            for country in countries {
                for keyword in queries {
                    let (legacy_cn, legacy_intl) = legacy_region_flags(region, country, keyword);
                    match resolve_lang(region, country, keyword) {
                        Lang::Chinese => assert_eq!(
                            (true, false),
                            (legacy_cn, legacy_intl),
                            "region={region:?} country={country:?} q={keyword}"
                        ),
                        Lang::English => assert_eq!(
                            (false, true),
                            (legacy_cn, legacy_intl),
                            "region={region:?} country={country:?} q={keyword}"
                        ),
                        Lang::All => panic!("MA1 不产出 All"),
                    }
                }
            }
        }
    }
}
