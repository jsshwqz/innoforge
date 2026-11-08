//! 查询改写与意图解析（看板队列 #2 P1，设计稿 `docs/plans/2026-11-08-agent-search-design.md`）。
//!
//! ## 职责边界（设计稿 §1 的落点）
//!
//! LLM 只做「编排与判断」，不做「取数」——本模块只负责**解析与校验** LLM 的改写输出，
//! 取数仍由现有五源确定性执行。prompt 常量在 `routes/search.rs`（AGENTS.md 2.6）。
//!
//! ## 诚实性纪律
//!
//! - **主查询恒为用户原词**：LLM 的「改写主查询」一律不采纳（改写偏离原意是最大风险，
//!   设计稿 §5），LLM 只贡献变体（≤3 条，作为「链 0 命中后的重试词」）。
//! - 解析失败 / JSON 破损 / 字段越界 → 返回 `None`，路由层记 Failed(Parse) 后走原行为
//!   ——LLM 全挂时行为与 v0.9.38 逐字一致（设计稿硬验收）。
//! - 意图映射失败 → 保守回 `Keyword`（不猜）。

use crate::search::model::SearchQuery;

/// 查询意图（LLM 判定，解析失败保守回 Keyword）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchIntent {
    Keyword,
    Applicant,
    Inventor,
    PatentNumber,
}

impl SearchIntent {
    /// LLM 输出的意图标签 → 枚举。未知标签保守回 Keyword。
    pub fn from_label(label: &str) -> Self {
        match label.trim() {
            "申请人" | "applicant" => SearchIntent::Applicant,
            "发明人" | "inventor" => SearchIntent::Inventor,
            "专利号" | "patent_number" | "patent number" => SearchIntent::PatentNumber,
            _ => SearchIntent::Keyword,
        }
    }
}

/// 查询改写结果。**没有「改写后的主查询」字段**——主查询恒为用户原词（见模块头）。
#[derive(Debug, Clone, PartialEq)]
pub struct QueryRewrite {
    /// 重试变体（≤3 条，去重、剔除与原词相同者、剔除空串）。顺序 = LLM 给出的优先序。
    pub variants: Vec<String>,
    pub intent: SearchIntent,
}

/// 从 LLM 输出解析改写结果。
///
/// 输入契约（system prompt 约定）：纯 JSON（允许 ``` 围栏），
/// `{"variants": ["...", "..."], "intent": "关键词|申请人|发明人|专利号"}`。
/// 任何解析/校验失败 → `None`（路由层记 Failed(Parse) 并走原行为）。
pub fn parse_rewrite_response(body: &str, original: &str) -> Option<QueryRewrite> {
    let trimmed = body.trim();
    // 剥 ``` 围栏（LLM 常见输出形态；与 response_pipeline::extract_json_array 同取向，
    // 但此处只需要一个 JSON 对象而非数组，且不引依赖——本地小解析）。
    let json_str = if let Some(start) = trimmed.find("```") {
        let after = &trimmed[start + 3..];
        let after = after.strip_prefix("json").unwrap_or(after);
        let obj_start = after.find('{')?;
        let obj_end = after.rfind('}')?;
        &after[obj_start..=obj_end]
    } else {
        let obj_start = trimmed.find('{')?;
        let obj_end = trimmed.rfind('}')?;
        &trimmed[obj_start..=obj_end]
    };
    let json: serde_json::Value = match serde_json::from_str(json_str) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("改写输出非合法 JSON: {} | body[:160]={:?}", e, {
                let s: String = body.chars().take(160).collect();
                s
            });
            return None;
        }
    };
    let original = original.trim();
    let mut variants = Vec::new();
    if let Some(arr) = json.get("variants").and_then(|v| v.as_array()) {
        for v in arr {
            let s = match v.as_str() {
                Some(s) => s.trim().to_string(),
                None => continue,
            };
            if s.is_empty() || s == original {
                continue;
            }
            if variants.contains(&s) {
                continue;
            }
            variants.push(s);
            if variants.len() >= 3 {
                break;
            }
        }
    }
    let intent = json
        .get("intent")
        .and_then(|v| v.as_str())
        .map(SearchIntent::from_label)
        .unwrap_or(SearchIntent::Keyword);
    Some(QueryRewrite { variants, intent })
}

/// 「链 0 命中后」的重试决策（纯函数，离线可测）。
///
/// 返回 `Some(重试查询)` = 用第一个变体重试一次；`None` = 不重试（走原降级链）。
/// 不重试的情形：
/// - 改写不可用（LLM 失败/解析失败 → `rewrite` 为 `None`）或没有任何可用变体；
/// - 意图为专利号——精确直查通道（SerpAPI Details）已覆盖，变体毫无意义；
/// - 原查询为空（空查询在路由层早已早退，此处兜底不猜）。
pub(crate) fn retry_query_for(
    original: &SearchQuery,
    rewrite: Option<&QueryRewrite>,
) -> Option<SearchQuery> {
    if original.keyword.trim().is_empty() {
        return None;
    }
    let rw = rewrite?;
    if rw.intent == SearchIntent::PatentNumber {
        return None;
    }
    let first = rw.variants.first()?;
    let mut retry = original.clone();
    retry.keyword = first.clone();
    Some(retry)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rewrite_json(body: &str) -> Option<QueryRewrite> {
        parse_rewrite_response(body, "流量调节")
    }

    #[test]
    fn parse_accepts_plain_and_fenced_json() {
        let plain = r#"{"variants":["阀门流量控制","流量调节阀"],"intent":"关键词"}"#;
        let rw = rewrite_json(plain).expect("裸 JSON 应可解析");
        assert_eq!(rw.variants, vec!["阀门流量控制", "流量调节阀"]);
        assert_eq!(rw.intent, SearchIntent::Keyword);

        let fenced = "```json\n{\"variants\":[\"电动调节阀\"],\"intent\":\"申请人\"}\n```";
        let rw = rewrite_json(fenced).expect("围栏 JSON 应可解析");
        assert_eq!(rw.variants, vec!["电动调节阀"]);
        assert_eq!(rw.intent, SearchIntent::Applicant);
    }

    #[test]
    fn parse_caps_three_dedupes_and_drops_original() {
        let body = r#"{"variants":["流量调节","流量调节","流量控制阀","","阀门","a","b","c","d"],"intent":"关键词"}"#;
        let rw = rewrite_json(body).expect("应可解析");
        assert_eq!(
            rw.variants,
            vec!["流量控制阀", "阀门", "a"],
            "去重、剔除空串与原词、上限 3 条"
        );
    }

    #[test]
    fn parse_failure_modes_return_none() {
        assert!(rewrite_json("这不是 JSON").is_none(), "非 JSON → None");
        assert!(
            rewrite_json("{\"variants\": 3}").is_some(),
            "variants 非数组：跳过后 variants 为空、意图保守 Keyword，仍返回结构（重试决策会因无变体而放弃）"
        );
        assert!(rewrite_json("").is_none(), "空输出 → None");
    }

    #[test]
    fn parse_intent_unknown_label_falls_back_to_keyword() {
        let rw = rewrite_json(r#"{"variants":["x"],"intent":"天顶星人技术"}"#).expect("应可解析");
        assert_eq!(rw.intent, SearchIntent::Keyword, "未知意图保守回 Keyword");
        assert_eq!(rw.variants, vec!["x"]);
    }

    #[test]
    fn parse_all_variants_filtered_out_still_yields_intent() {
        // LLM 只回了个与原词相同的「变体」→ variants 空，但意图仍有价值（路由跳 EPO）。
        let rw = rewrite_json(r#"{"variants":["流量调节"],"intent":"发明人"}"#).expect("应可解析");
        assert!(rw.variants.is_empty());
        assert_eq!(rw.intent, SearchIntent::Inventor);
    }

    #[test]
    fn retry_decision_matrix() {
        let original = SearchQuery {
            keyword: "流量调节".into(),
            ..Default::default()
        };
        // 无改写 → 不重试
        assert!(retry_query_for(&original, None).is_none());
        // 无变体 → 不重试
        let empty = QueryRewrite {
            variants: vec![],
            intent: SearchIntent::Keyword,
        };
        assert!(retry_query_for(&original, Some(&empty)).is_none());
        // 专利号意图 → 不重试（精确直查通道已覆盖）
        let pn = QueryRewrite {
            variants: vec!["CN114962757A".into()],
            intent: SearchIntent::PatentNumber,
        };
        assert!(retry_query_for(&original, Some(&pn)).is_none());
        // 正常：用第一个变体重试，其余字段（country/language/page…）逐字保留
        let rw = QueryRewrite {
            variants: vec!["阀门流量控制".into(), "流量调节阀".into()],
            intent: SearchIntent::Keyword,
        };
        let retry = retry_query_for(&original, Some(&rw)).expect("应重试");
        assert_eq!(retry.keyword, "阀门流量控制");
        assert_eq!(retry.language, original.language);
        assert_eq!(retry.country, original.country);
        assert_eq!(retry.page, original.page);
        // 空原词兜底 → 不重试
        let mut blank = original.clone();
        blank.keyword = "  ".into();
        assert!(retry_query_for(&blank, Some(&rw)).is_none());
    }
}
