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

// ─────────────────────────────────────────────────────────────────────────────
// P2：LLM 重排解析（设计稿 [C] 段）
// ─────────────────────────────────────────────────────────────────────────────

/// 单条重排判定：1 起序号、0-100 分、一句话理由（≤200 字截断）。
#[derive(Debug, Clone, PartialEq)]
pub struct RerankVerdict {
    pub index: usize,
    pub score: u8,
    pub reason: String,
}

/// 从 LLM 输出解析重排判定。
///
/// 输入契约（system prompt 约定）：纯 JSON 数组（允许 ``` 围栏），
/// `[{"n": 1, "score": 85, "why": "一句话理由"}, ...]`。
///
/// **整段弃用纪律（设计稿拍板）**：任何序号越界（<1 或 >候选数）或非整数 → 返回 `None`
/// ——LLM 数错行数说明它没在核对候选清单，其分数不可信，宁可回退混合分。
/// 重复序号保留首个；漏标的候选视为「未判定」，调用方保持其原相对顺序。
/// 分数越界 clamp 到 0-100；理由截断 200 字（展示用途，2.5 规约）。
pub fn parse_rerank_response(body: &str, candidate_count: usize) -> Option<Vec<RerankVerdict>> {
    if candidate_count == 0 {
        return None;
    }
    let trimmed = body.trim();
    let json_str = if let Some(start) = trimmed.find("```") {
        let after = &trimmed[start + 3..];
        let after = after.strip_prefix("json").unwrap_or(after);
        let arr_start = after.find('[')?;
        let arr_end = after.rfind(']')?;
        &after[arr_start..=arr_end]
    } else {
        let arr_start = trimmed.find('[')?;
        let arr_end = trimmed.rfind(']')?;
        &trimmed[arr_start..=arr_end]
    };
    let json: serde_json::Value = match serde_json::from_str(json_str) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("重排输出非合法 JSON: {} | body[:160]={:?}", e, {
                let s: String = body.chars().take(160).collect();
                s
            });
            return None;
        }
    };
    let arr = json.as_array()?;
    let mut verdicts = Vec::new();
    for item in arr {
        let n = match item.get("n").and_then(|v| v.as_u64()) {
            Some(n) if n >= 1 && n <= candidate_count as u64 => n as usize,
            _ => {
                tracing::warn!("重输出现越界/非法序号，整段弃用");
                return None;
            }
        };
        let score_raw = item.get("score")?;
        let score_f = match score_raw.as_f64() {
            Some(f) => f,
            None => {
                tracing::warn!("重排出现非数值分数，整段弃用");
                return None;
            }
        };
        let score = score_f.round().clamp(0.0, 100.0) as u8;
        let why = item
            .get("why")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let why = if why.chars().count() > 200 {
            let mut s: String = why.chars().take(200).collect();
            s.push('…');
            s
        } else {
            why
        };
        if verdicts.iter().any(|v: &RerankVerdict| v.index == n) {
            continue;
        }
        verdicts.push(RerankVerdict {
            index: n,
            score,
            reason: why,
        });
    }
    if verdicts.is_empty() {
        return None;
    }
    Some(verdicts)
}

/// 构建重排候选块（外部内容隔离，设计稿拍板项）。
///
/// - ≤20 条（超出部分不参与重排，保持原序）；
/// - 每条独立 `<candidate N>…</candidate>` 标签包裹，N = 1 起序号；
/// - 标题 ≤100 字、摘要 ≤200 字截断（chars() 字符级，2.7 规约禁裸字节切片）；
/// - 网页命中（\`is_web\`）标注域名——给 LLM 的上下文里如实写明这是网页不是专利著录。
pub fn build_rerank_candidates(titles: &[String], abstracts: &[String], is_web: bool) -> String {
    const MAX_CANDIDATES: usize = 20;
    let mut out = String::new();
    for (i, (title, abstr)) in titles
        .iter()
        .zip(abstracts.iter())
        .take(MAX_CANDIDATES)
        .enumerate()
    {
        let mut t: String = title.chars().take(100).collect();
        if title.chars().count() > 100 {
            t.push('…');
        }
        let mut a: String = abstr.chars().take(200).collect();
        if abstr.chars().count() > 200 {
            a.push('…');
        }
        out.push_str(&format!(
            "<candidate {}>
",
            i + 1
        ));
        if is_web {
            out.push_str(
                "（来源：网页搜索命中，非专利著录）
",
            );
        }
        out.push_str(&format!(
            "标题：{}
摘要：{}
</candidate>
",
            t, a
        ));
    }
    out
}

#[cfg(test)]
mod rerank_tests {
    use super::*;

    #[test]
    fn parse_rerank_accepts_plain_and_fenced_arrays() {
        let plain = r#"[{"n":1,"score":85,"why":"标题与查询直接对应"},{"n":2,"score":40,"why":"仅摘要提及"}]"#;
        let v = parse_rerank_response(plain, 2).expect("裸数组应可解析");
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].index, 1);
        assert_eq!(v[0].score, 85);
        assert_eq!(v[1].reason, "仅摘要提及");

        let fenced = "```json
[{\"n\":1,\"score\":70,\"why\":\"对应\"}]
```";
        let v = parse_rerank_response(fenced, 1).expect("围栏应可解析");
        assert_eq!(v[0].score, 70);
    }

    #[test]
    fn parse_rerank_any_out_of_range_index_discards_whole_output() {
        let body = r#"[{"n":1,"score":85,"why":"ok"},{"n":3,"score":10,"why":"越界"}]"#;
        assert!(
            parse_rerank_response(body, 2).is_none(),
            "混入未出现序号 → 整段弃用（设计稿纪律）"
        );
        let zero = r#"[{"n":0,"score":85,"why":"越界"}]"#;
        assert!(parse_rerank_response(zero, 2).is_none());
        let frac = r#"[{"n":1.5,"score":85,"why":"非整数"}]"#;
        assert!(
            parse_rerank_response(frac, 2).is_none(),
            "非整数序号 → 弃用"
        );
    }

    #[test]
    fn parse_rerank_clamps_score_and_truncates_reason() {
        let body = format!(r#"[{{"n":1,"score":142,"why":"{}"}}]"#, "长".repeat(260));
        let v = parse_rerank_response(&body, 1).expect("应可解析");
        assert_eq!(v[0].score, 100, "越界分数 clamp 到 100");
        let chars = v[0].reason.chars().count();
        assert!(chars <= 201, "理由截断 200+省略号，实际 {chars}");
    }

    #[test]
    fn parse_rerank_keeps_first_of_duplicate_index() {
        let body = r#"[{"n":1,"score":10,"why":"first"},{"n":1,"score":99,"why":"dup"}]"#;
        let v = parse_rerank_response(body, 1).expect("应可解析");
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].score, 10, "重复序号保留首个");
    }

    #[test]
    fn parse_rerank_empty_and_zero_candidates() {
        assert!(
            parse_rerank_response("[]", 5).is_none(),
            "空数组 → None（无判定可用）"
        );
        assert!(parse_rerank_response(r#"[{"n":1,"score":5,"why":"x"}]"#, 0).is_none());
        assert!(parse_rerank_response("垃圾", 5).is_none());
    }

    #[test]
    fn build_rerank_candidates_tags_and_truncates() {
        let titles = vec!["短标题".to_string()];
        let abstracts = vec!["长".repeat(300)];
        let block = build_rerank_candidates(&titles, &abstracts, false);
        assert!(block.contains("<candidate 1>"));
        assert!(block.contains("</candidate>"));
        assert!(block.contains("标题：短标题"));
        let abs_line_len = block
            .lines()
            .find(|l| l.starts_with("摘要："))
            .map(|l| l.chars().count())
            .unwrap_or(0);
        assert!(
            abs_line_len <= 200 + "摘要：".len() + 1,
            "摘要截断 200 字+省略号，实际 {abs_line_len}"
        );
        // 网页命中标注
        let web_block = build_rerank_candidates(&titles, &abstracts, true);
        assert!(web_block.contains("非专利著录"));
        // 超 20 条截断
        let many: Vec<String> = (0..25).map(|i| format!("标题{i}")).collect();
        let many_abs: Vec<String> = (0..25).map(|_| "摘要".to_string()).collect();
        let big = build_rerank_candidates(&many, &many_abs, false);
        assert!(big.contains("<candidate 20>"));
        assert!(!big.contains("<candidate 21>"), "上限 20 条");
    }
}
