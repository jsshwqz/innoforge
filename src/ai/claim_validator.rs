//! 权利要求树结构化校验引擎（形式语法 + 专利法第26条第4款实施例支撑）
//!
//! 1. 确定性语法规则检查：
//!    - 从属权利要求引用编号合法性（禁止自引用/前向引用/引用不存在的权项）
//!    - 禁止多项从属权利要求引用另一多项从属权利要求（实施细则第25条第2款）
//!    - 前序与正文特征“所述XX”是否在父权项或同权项前文中已出现（缺少引用基础告警）
//!    - 独立权利要求必要技术特征完整性初筛
//! 2. 专利法第26条第4款说明书实施例支撑核对：
//!    - 结合 `v25` `chunks_fts` 检索说明书实施例分块，确定性核对权利要求中的数值范围与功能限定
//!    - 可选调用 `ai_client_expert` 做深层语义核对（离线/未配置 Key 时平滑降级为纯确定性引擎）

use crate::ai::{truncate_for_ai, AiClient};
use crate::db::Database;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

/// 单条权利要求输入结构
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ClaimInput {
    #[serde(default)]
    pub id: Option<String>,
    pub claim_number: u32,
    #[serde(default)]
    pub claim_type: String,
    #[serde(default)]
    pub parent_claim_id: Option<String>,
    #[serde(default)]
    pub depends_on: Vec<u32>,
    pub content: String,
}

/// 告警严重级别
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ClaimIssueSeverity {
    Fatal,
    Warning,
    Info,
}

/// 单条权利要求缺陷告警
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClaimValidationIssue {
    pub claim_number: u32,
    pub rule_id: String,
    pub category: String,
    pub severity: ClaimIssueSeverity,
    pub title: String,
    pub message: String,
    pub snippet: String,
    pub suggestion: String,
    #[serde(default)]
    pub evidence_chunks: Vec<String>,
}

/// 权利要求树体检报告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaimValidationReport {
    pub status: String,
    pub idea_id: String,
    pub passed: bool,
    pub total_claims: usize,
    pub fatal_count: usize,
    pub warning_count: usize,
    pub ai_checked: bool,
    pub fts_hits_count: usize,
    pub issues: Vec<ClaimValidationIssue>,
    pub summary: String,
}

#[derive(Debug, Clone)]
struct NumericConstraint {
    raw: String,
    numbers: Vec<String>,
    unit: String,
    start_char: usize,
    end_char: usize,
}

#[derive(Debug, Clone)]
struct ParsedClaim<'a> {
    input: &'a ClaimInput,
    refs: Vec<u32>,
    is_dependent: bool,
    is_multiple_dependent: bool,
}

/// 仅执行确定性规则校验（供单元测试与离线模式复用）
pub fn validate_claims_deterministic(
    idea_id: &str,
    claims: &[ClaimInput],
    spec_chunks: &[String],
) -> ClaimValidationReport {
    validate_claims_internal(idea_id, claims, spec_chunks, 0)
}

/// 结合 `Database`（`v25` `chunks_fts`）与可选 `ai_client_expert` 执行完整权利要求体检
pub async fn validate_claims(
    idea_id: &str,
    claims: &[ClaimInput],
    extra_spec_chunks: &[String],
    db: Option<&Database>,
    ai_expert: Option<&AiClient>,
) -> ClaimValidationReport {
    let mut spec_chunks: Vec<String> = extra_spec_chunks
        .iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    let mut fts_hits_count = 0usize;
    let mut matched_chunk_ids: Vec<String> = Vec::new();

    if let Some(database) = db {
        let mut query_parts: Vec<String> = claims
            .iter()
            .map(|c| c.content.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if let Ok(Some(idea)) = database.get_idea(idea_id) {
            if !idea.title.trim().is_empty() {
                query_parts.push(idea.title);
            }
        }
        let combined_query = query_parts.join(" ");
        if !combined_query.trim().is_empty() {
            match database.search_embodiment_chunks_lexical(&combined_query, 16) {
                Ok(hits) => {
                    fts_hits_count = hits.len();
                    for hit in hits {
                        matched_chunk_ids.push(format!(
                            "{}({})",
                            hit.chunk_id,
                            if hit.section_tag.is_empty() {
                                "chunk"
                            } else {
                                &hit.section_tag
                            }
                        ));
                        if !hit.content.trim().is_empty() {
                            spec_chunks.push(hit.content);
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("claim_validator: chunks_fts 检索失败，降级继续: {}", e);
                }
            }
        }
    }

    let mut report = validate_claims_internal(idea_id, claims, &spec_chunks, fts_hits_count);

    // 附带命中的 chunks_fts 来源标签（如有数值/功能警告，便于追溯已查核的分块）
    if !matched_chunk_ids.is_empty() {
        let top_refs: Vec<String> = matched_chunk_ids.into_iter().take(5).collect();
        for issue in &mut report.issues {
            if issue.category == "article_26_4" && issue.evidence_chunks.is_empty() {
                issue.evidence_chunks = top_refs.clone();
            }
        }
    }

    // 若配置了专家模型且存在待核对文本，执行深度第26条第4款语义核对
    if let Some(client) = ai_expert {
        if !claims.is_empty() && !spec_chunks.is_empty() {
            if let Some(ai_issues) = run_ai_article_26_4_review(client, claims, &spec_chunks).await
            {
                report.ai_checked = true;
                for issue in ai_issues {
                    let exists = report.issues.iter().any(|existing| {
                        existing.claim_number == issue.claim_number
                            && existing.rule_id == issue.rule_id
                            && (existing.snippet == issue.snippet || existing.title == issue.title)
                    });
                    if !exists {
                        report.issues.push(issue);
                    }
                }
                recompute_report_counts(&mut report);
            }
        }
    }

    report
}

fn validate_claims_internal(
    idea_id: &str,
    claims: &[ClaimInput],
    spec_chunks: &[String],
    fts_hits_count: usize,
) -> ClaimValidationReport {
    if claims.is_empty() {
        let issues = vec![ClaimValidationIssue {
            claim_number: 0,
            rule_id: "SYNTAX_EMPTY_CLAIMS".to_string(),
            category: "syntax".to_string(),
            severity: ClaimIssueSeverity::Warning,
            title: "尚未生成或添加权利要求".to_string(),
            message: "当前项目权利要求树为空，请先通过流水线生成或手动新增独立权利要求后再体检。"
                .to_string(),
            snippet: String::new(),
            suggestion: "点击「+ 新增权利要求」添加第1项独立权利要求。".to_string(),
            evidence_chunks: Vec::new(),
        }];
        let mut report = ClaimValidationReport {
            status: "ok".to_string(),
            idea_id: idea_id.to_string(),
            passed: false,
            total_claims: 0,
            fatal_count: 0,
            warning_count: 1,
            ai_checked: false,
            fts_hits_count,
            issues,
            summary: String::new(),
        };
        recompute_report_counts(&mut report);
        return report;
    }

    // 按 claim_number 升序组织，同时保留 id -> claim_number 映射
    let mut sorted_claims: Vec<&ClaimInput> = claims.iter().collect();
    sorted_claims.sort_by_key(|c| c.claim_number);

    let mut id_to_num: HashMap<String, u32> = HashMap::new();
    let mut valid_nums: BTreeSet<u32> = BTreeSet::new();
    let mut duplicate_nums: HashSet<u32> = HashSet::new();
    for c in &sorted_claims {
        if !valid_nums.insert(c.claim_number) {
            duplicate_nums.insert(c.claim_number);
        }
        if let Some(id) = c.id.as_ref() {
            let trimmed = id.trim();
            if !trimmed.is_empty() {
                id_to_num.insert(trimmed.to_string(), c.claim_number);
            }
        }
    }

    let mut parsed_claims: Vec<ParsedClaim<'_>> = Vec::with_capacity(sorted_claims.len());
    let mut multi_dep_map: HashMap<u32, bool> = HashMap::new();
    let mut deps_map: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    let mut content_map: HashMap<u32, &str> = HashMap::new();

    for c in &sorted_claims {
        let (refs, is_dep, is_multi) = parse_claim_dependencies(c, &id_to_num, &valid_nums);
        multi_dep_map.insert(c.claim_number, is_multi);
        deps_map.insert(c.claim_number, refs.clone());
        content_map.entry(c.claim_number).or_insert(&c.content);
        parsed_claims.push(ParsedClaim {
            input: c,
            refs,
            is_dependent: is_dep,
            is_multiple_dependent: is_multi,
        });
    }

    let mut issues: Vec<ClaimValidationIssue> = Vec::new();

    // 重复权号校验
    for dup in &duplicate_nums {
        issues.push(ClaimValidationIssue {
            claim_number: *dup,
            rule_id: "SYNTAX_DUPLICATE_CLAIM_NUMBER".to_string(),
            category: "syntax".to_string(),
            severity: ClaimIssueSeverity::Fatal,
            title: "权利要求编号重复".to_string(),
            message: format!(
                "存在多条编号为 {} 的权利要求，违反权利要求顺序唯一性。",
                dup
            ),
            snippet: format!("权利要求{}", dup),
            suggestion: "请调整权利要求序号，确保从 1 开始连续且不重复。".to_string(),
            evidence_chunks: Vec::new(),
        });
    }

    // 是否至少存在一条独立权利要求
    let has_independent = parsed_claims.iter().any(|p| !p.is_dependent);
    if !has_independent {
        issues.push(ClaimValidationIssue {
            claim_number: parsed_claims
                .first()
                .map(|p| p.input.claim_number)
                .unwrap_or(1),
            rule_id: "SYNTAX_NO_INDEPENDENT_CLAIM".to_string(),
            category: "syntax".to_string(),
            severity: ClaimIssueSeverity::Fatal,
            title: "缺少独立权利要求".to_string(),
            message: "权利要求树中未检出独立权利要求，所有权利要求均表现为从属引用形态。"
                .to_string(),
            snippet: String::new(),
            suggestion: "至少设置第 1 项为独立权利要求，并完整记载解决技术问题的必要技术特征。"
                .to_string(),
            evidence_chunks: Vec::new(),
        });
    }

    // 1. 确定性语法与引用合法性校验
    for p in &parsed_claims {
        let num = p.input.claim_number;
        let trimmed = p.input.content.trim();
        if trimmed.is_empty() {
            issues.push(ClaimValidationIssue {
                claim_number: num,
                rule_id: "SYNTAX_EMPTY_CONTENT".to_string(),
                category: "syntax".to_string(),
                severity: ClaimIssueSeverity::Fatal,
                title: "权利要求内容为空".to_string(),
                message: format!("权利要求 {} 正文为空。", num),
                snippet: String::new(),
                suggestion: "请补充该权利要求的技术特征限定内容，或删除空节点。".to_string(),
                evidence_chunks: Vec::new(),
            });
            continue;
        }

        // 1a. 引用编号合法性（禁止自引用/前向引用/不存在的权项）
        for &ref_num in &p.refs {
            if ref_num == 0 || !valid_nums.contains(&ref_num) {
                issues.push(ClaimValidationIssue {
                    claim_number: num,
                    rule_id: "SYNTAX_INVALID_REF".to_string(),
                    category: "syntax".to_string(),
                    severity: ClaimIssueSeverity::Fatal,
                    title: "引用不存在的权利要求编号".to_string(),
                    message: format!(
                        "权利要求 {} 引用了编号为 {} 的权利要求，但当前权利要求树中不存在该编号。",
                        num, ref_num
                    ),
                    snippet: format!("权利要求{}", ref_num),
                    suggestion: "修改引用编号为实际存在的在先权利要求编号。".to_string(),
                    evidence_chunks: Vec::new(),
                });
            } else if ref_num >= num {
                issues.push(ClaimValidationIssue {
                    claim_number: num,
                    rule_id: "SYNTAX_INVALID_REF".to_string(),
                    category: "syntax".to_string(),
                    severity: ClaimIssueSeverity::Fatal,
                    title: "禁止自引用或向后引用".to_string(),
                    message: format!(
                        "权利要求 {} 引用了权利要求 {}（自身或后续权项），不符合从属权利要求引用在先权项的规范。",
                        num, ref_num
                    ),
                    snippet: format!("权利要求{}", ref_num),
                    suggestion: format!("仅可引用编号小于 {} 的在先权利要求。", num),
                    evidence_chunks: Vec::new(),
                });
            }
        }

        // 显式标明从属却无任何有效引用对象
        if p.input.claim_type.eq_ignore_ascii_case("dependent") && p.refs.is_empty() {
            issues.push(ClaimValidationIssue {
                claim_number: num,
                rule_id: "SYNTAX_MISSING_PARENT".to_string(),
                category: "syntax".to_string(),
                severity: ClaimIssueSeverity::Warning,
                title: "从属权利要求缺少引用对象".to_string(),
                message: format!(
                    "权利要求 {} 类型为从属权利要求，但正文中未写明“根据权利要求X...”且未绑定父节点。",
                    num
                ),
                snippet: preview_chars(trimmed, 40),
                suggestion: "在正文前序部分写明“根据权利要求1所述的……”或选择父权利要求节点。"
                    .to_string(),
                evidence_chunks: Vec::new(),
            });
        }

        // 1b. 多项从属引用多项从属（实施细则第25条第2款）
        if p.is_multiple_dependent {
            for &parent_num in &p.refs {
                if parent_num < num && multi_dep_map.get(&parent_num).copied().unwrap_or(false) {
                    issues.push(ClaimValidationIssue {
                        claim_number: num,
                        rule_id: "SYNTAX_MULTI_DEP_CHAIN".to_string(),
                        category: "syntax".to_string(),
                        severity: ClaimIssueSeverity::Fatal,
                        title: "多项从属权利要求禁止引用多项从属权利要求".to_string(),
                        message: format!(
                            "权利要求 {} 为多项从属权利要求（引用 {:?}），其中权利要求 {} 本身也是多项从属权利要求，违反多项从属权利要求不得作为另一项多项从属权利要求基础的规定。",
                            num, p.refs, parent_num
                        ),
                        snippet: extract_ref_clause_snippet(trimmed),
                        suggestion: format!(
                            "将权利要求 {} 改为仅引用独立权利要求或单项从属权利要求，或拆分为独立的多项引用句式。",
                            num
                        ),
                        evidence_chunks: Vec::new(),
                    });
                }
            }
        }

        // 1c. “所述XX”引用基础校验
        let ancestor_text = collect_ancestor_text(num, &deps_map, &content_map, &sorted_claims);
        let suoshu_terms = extract_suoshu_terms(trimmed);
        let mut reported_terms: HashSet<String> = HashSet::new();
        for (char_pos, term) in suoshu_terms {
            if reported_terms.contains(&term) {
                continue;
            }
            let preceding_in_claim: String = trimmed.chars().take(char_pos).collect();
            let full_antecedent = format!("{}\n{}", ancestor_text, preceding_in_claim);
            if !has_antecedent_basis(&term, &full_antecedent) {
                reported_terms.insert(term.clone());
                issues.push(ClaimValidationIssue {
                    claim_number: num,
                    rule_id: "SYNTAX_ANTECEDENT_BASIS".to_string(),
                    category: "antecedent_basis".to_string(),
                    severity: ClaimIssueSeverity::Fatal,
                    title: "“所述”特征缺少引用基础".to_string(),
                    message: format!(
                        "权利要求 {} 中的“所述{}”在其引用链父权利要求及本权利要求前文中均未出现，存在缺少引用基础的不清楚缺陷（专利法第26条第4款）。",
                        num, term
                    ),
                    snippet: format!("所述{}", term),
                    suggestion: format!(
                        "请在前序或父权利要求中先首次引入“{}”，或将“所述{}”改为不定冠词首次限定。",
                        term, term
                    ),
                    evidence_chunks: Vec::new(),
                });
            }
        }

        // 1d. 必要技术特征完整性初筛
        if !p.is_dependent {
            let has_structural_marker = [
                "包括",
                "包含",
                "具有",
                "设有",
                "由",
                "特征",
                "步骤",
                "comprising",
                "including",
                "having",
            ]
            .iter()
            .any(|kw| trimmed.contains(kw));
            if trimmed.chars().count() < 12 || !has_structural_marker {
                issues.push(ClaimValidationIssue {
                    claim_number: num,
                    rule_id: "ART26_4_MISSING_ESSENTIAL_FEATURE".to_string(),
                    category: "essential_features".to_string(),
                    severity: ClaimIssueSeverity::Warning,
                    title: "独立权利要求可能缺少必要技术特征".to_string(),
                    message: format!(
                        "独立权利要求 {} 表述过短或缺乏组成/步骤连接词，可能未完整记载解决技术问题的必要技术特征。",
                        num
                    ),
                    snippet: preview_chars(trimmed, 48),
                    suggestion: "补充“包括……、其特征在于……”结构，明确核心部件/步骤及连接配合关系。"
                        .to_string(),
                    evidence_chunks: Vec::new(),
                });
            }
        }
    }

    // 2. 专利法第26条第4款说明书实施例支撑核对（确定性通道）
    let normalized_corpus = normalize_for_match(&spec_chunks.join("\n"));
    for p in &parsed_claims {
        let num = p.input.claim_number;
        let trimmed = p.input.content.trim();
        if trimmed.is_empty() {
            continue;
        }

        // 2a. 数值范围/参数支撑核对
        let constraints = extract_numeric_constraints(trimmed);
        let mut seen_numeric: HashSet<String> = HashSet::new();
        for nc in constraints {
            if !seen_numeric.insert(nc.raw.clone()) {
                continue;
            }
            if !numeric_supported_in_corpus(&nc, &normalized_corpus) {
                let reason = if normalized_corpus.is_empty() {
                    "当前未检索到说明书或 chunks_fts 实施例正文，无法证实该数值参数具有实施例支撑"
                        .to_string()
                } else {
                    format!(
                        "说明书及 chunks_fts 实施例分块中未检出数值限定的对应支撑（缺少边界值或参数 {}）",
                        nc.numbers.join(" / ")
                    )
                };
                issues.push(ClaimValidationIssue {
                    claim_number: num,
                    rule_id: "ART26_4_NUMERIC_UNSUPPORTED".to_string(),
                    category: "article_26_4".to_string(),
                    severity: ClaimIssueSeverity::Warning,
                    title: "数值范围/参数缺乏说明书实施例支撑（第26条第4款）".to_string(),
                    message: format!("权利要求 {} 中的数值限定“{}”：{}。", num, nc.raw, reason),
                    snippet: nc.raw,
                    suggestion: "在说明书具体实施方式中补充对应端点值及中间点实施例数据，或将范围收窄至说明书已记载的区间。"
                        .to_string(),
                    evidence_chunks: Vec::new(),
                });
            }
        }

        // 2b. 功能性限定核对（仅在存在说明书语料时做低重合度筛查，避免无语料时误报）
        if !normalized_corpus.is_empty() {
            for func_phrase in extract_functional_limitations(trimmed) {
                if !functional_supported_in_corpus(&func_phrase, &normalized_corpus) {
                    issues.push(ClaimValidationIssue {
                        claim_number: num,
                        rule_id: "ART26_4_FUNCTIONAL_UNSUPPORTED".to_string(),
                        category: "article_26_4".to_string(),
                        severity: ClaimIssueSeverity::Warning,
                        title: "功能性限定在说明书中支撑较弱（第26条第4款）".to_string(),
                        message: format!(
                            "权利要求 {} 的功能性描述“{}”在已检索的说明书实施例分块中词汇重合度极低，可能被认定为得不到说明书支持。",
                            num, func_phrase
                        ),
                        snippet: func_phrase,
                        suggestion: "建议在权利要求中补充实现该功能的具体结构或算法步骤，并在说明书中增加相应实施例。"
                            .to_string(),
                        evidence_chunks: Vec::new(),
                    });
                }
            }
        }
    }

    let mut report = ClaimValidationReport {
        status: "ok".to_string(),
        idea_id: idea_id.to_string(),
        passed: issues.is_empty(),
        total_claims: sorted_claims.len(),
        fatal_count: 0,
        warning_count: 0,
        ai_checked: false,
        fts_hits_count,
        issues,
        summary: String::new(),
    };
    recompute_report_counts(&mut report);
    report
}

fn recompute_report_counts(report: &mut ClaimValidationReport) {
    let fatal_count = report
        .issues
        .iter()
        .filter(|i| i.severity == ClaimIssueSeverity::Fatal)
        .count();
    let warning_count = report
        .issues
        .iter()
        .filter(|i| i.severity == ClaimIssueSeverity::Warning)
        .count();
    report.fatal_count = fatal_count;
    report.warning_count = warning_count;
    report.passed = fatal_count == 0 && warning_count == 0;
    report.summary = if report.issues.is_empty() {
        format!(
            "已完成 {} 条权利要求形式与第26条第4款体检，未发现缺陷（检索到 {} 条实施例分块）。",
            report.total_claims, report.fts_hits_count
        )
    } else {
        format!(
            "共检出 {} 项问题（{} 项致命错误、{} 项预警；共校验 {} 条权利要求，关联 {} 条实施例分块{}）。",
            report.issues.len(),
            fatal_count,
            warning_count,
            report.total_claims,
            report.fts_hits_count,
            if report.ai_checked { "，含专家模型复核" } else { "" }
        )
    };
}

/// 解析单条权利要求的引用编号与多项从属属性
fn parse_claim_dependencies(
    c: &ClaimInput,
    id_to_num: &HashMap<String, u32>,
    valid_nums: &BTreeSet<u32>,
) -> (Vec<u32>, bool, bool) {
    let mut refs: BTreeSet<u32> = BTreeSet::new();
    for &n in &c.depends_on {
        if n > 0 {
            refs.insert(n);
        }
    }
    if let Some(pid) = c.parent_claim_id.as_ref() {
        if let Some(&n) = id_to_num.get(pid.trim()) {
            refs.insert(n);
        }
    }

    let text = c.content.trim();
    let mut multi_phrase = false;

    // 匹配“上述任一项权利要求 / 前述任一项权利要求”
    if let Ok(re_any_prev) =
        Regex::new(r"(?:根据|如)(?:上述|前述|以上)(?:任一项|任意一项|之一)(?:所述)?(?:的)?权利要求")
    {
        if re_any_prev.is_match(text) {
            multi_phrase = true;
            for &vn in valid_nums {
                if vn < c.claim_number {
                    refs.insert(vn);
                }
            }
        }
    }

    // 匹配“根据权利要求1 / 根据权利要求1或2 / 根据权利要求1-3 / 权利要求1至3任一项”
    if let Ok(re_cn) =
        Regex::new(r"(?:根据|如)?(?:权利要求|权)\s*([0-9][0-9\s、，,和与或至到~～\-－]*)")
    {
        for cap in re_cn.captures_iter(text) {
            if let Some(m) = cap.get(1) {
                let expr = m.as_str();
                if expr.contains('或')
                    || expr.contains('、')
                    || expr.contains('至')
                    || expr.contains('到')
                    || expr.contains('-')
                    || expr.contains('~')
                    || expr.contains('～')
                    || expr.contains('－')
                {
                    multi_phrase = true;
                }
                for n in parse_number_expr(expr) {
                    refs.insert(n);
                }
            }
        }
    }

    // 匹配英文 “according to claim 1 / claims 1 or 2 / claims 1-3”
    if let Ok(re_en) = Regex::new(r"(?i)\bclaims?\s+([0-9]+(?:\s*(?:,|or|and|to|-|~)\s*[0-9]+)*)") {
        for cap in re_en.captures_iter(text) {
            if let Some(m) = cap.get(1) {
                let expr = m.as_str();
                let lower = expr.to_ascii_lowercase();
                if lower.contains("or") || lower.contains('-') || lower.contains("to") {
                    multi_phrase = true;
                }
                for n in parse_number_expr(expr) {
                    refs.insert(n);
                }
            }
        }
    }

    if text.contains("任一项")
        || text.contains("至少一项")
        || text.to_ascii_lowercase().contains("any one of claims")
        || text.to_ascii_lowercase().contains("any preceding claim")
    {
        multi_phrase = true;
    }

    let ref_vec: Vec<u32> = refs.into_iter().collect();
    let is_dependent = c.claim_type.eq_ignore_ascii_case("dependent")
        || !ref_vec.is_empty()
        || text.starts_with("根据权利要求")
        || text.starts_with("如权利要求");
    let is_multiple_dependent =
        is_dependent && (ref_vec.len() >= 2 || (multi_phrase && !ref_vec.is_empty()));

    (ref_vec, is_dependent, is_multiple_dependent)
}

fn parse_number_expr(expr: &str) -> Vec<u32> {
    let mut out = Vec::new();
    let normalized = expr
        .replace("or", ",")
        .replace("and", ",")
        .replace("to", "-")
        .replace(['、', '，', '和', '与', '或', ' '], ",")
        .replace(['至', '到', '~', '～', '－'], "-");

    for part in normalized.split(',') {
        let p = part.trim();
        if p.is_empty() {
            continue;
        }
        if let Some((a_str, b_str)) = p.split_once('-') {
            if let (Ok(a), Ok(b)) = (a_str.trim().parse::<u32>(), b_str.trim().parse::<u32>()) {
                if a > 0 && b >= a && b - a <= 50 {
                    for v in a..=b {
                        out.push(v);
                    }
                } else {
                    if a > 0 {
                        out.push(a);
                    }
                    if b > 0 {
                        out.push(b);
                    }
                }
                continue;
            }
        }
        if let Ok(n) = p.parse::<u32>() {
            if n > 0 {
                out.push(n);
            }
        }
    }
    out
}

/// 递归收集引用链上全部祖先权利要求全文（无引用编号时回退到更小编号的权利要求）
fn collect_ancestor_text(
    claim_number: u32,
    deps_map: &BTreeMap<u32, Vec<u32>>,
    content_map: &HashMap<u32, &str>,
    sorted_claims: &[&ClaimInput],
) -> String {
    let mut visited: BTreeSet<u32> = BTreeSet::new();
    let mut stack: Vec<u32> = deps_map
        .get(&claim_number)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|&n| n < claim_number)
        .collect();

    while let Some(curr) = stack.pop() {
        if !visited.insert(curr) {
            continue;
        }
        if let Some(parents) = deps_map.get(&curr) {
            for &p in parents {
                if p < curr && !visited.contains(&p) {
                    stack.push(p);
                }
            }
        }
    }

    if visited.is_empty() {
        // 如果从属权利要求未写明具体父编号，检查紧邻在先权项避免误报，否则仅返回空串
        if let Some(curr_claim) = sorted_claims
            .iter()
            .find(|c| c.claim_number == claim_number)
        {
            if curr_claim.claim_type.eq_ignore_ascii_case("dependent") {
                return sorted_claims
                    .iter()
                    .filter(|c| c.claim_number < claim_number)
                    .map(|c| c.content.as_str())
                    .collect::<Vec<_>>()
                    .join("\n");
            }
        }
        return String::new();
    }

    visited
        .into_iter()
        .filter_map(|n| content_map.get(&n).copied())
        .collect::<Vec<_>>()
        .join("\n")
}

/// 提取权利要求中的“所述XX”特征短语（返回 `(字符下标, 特征名)`），全程使用 `Vec<char>` 确保 UTF-8 绝对安全
fn extract_suoshu_terms(content: &str) -> Vec<(usize, String)> {
    const MULTI_STOP_TOKENS: &[&str] = &[
        "其特征在于",
        "包括",
        "包含",
        "具有",
        "设有",
        "设置",
        "安装",
        "连接",
        "耦合",
        "固定",
        "位于",
        "通过",
        "用于",
        "能够",
        "可以",
        "配置",
        "适配",
        "形成",
        "产生",
        "输出",
        "输入",
        "采集",
        "检测",
        "控制",
        "驱动",
        "支撑",
        "限制",
        "之间",
        "内部",
        "外部",
        "表面",
        "顶部",
        "底部",
        "两端",
        "一端",
        "分别",
        "进一步",
        "其中",
        "并且",
        "而且",
        "以及",
        "或者",
        "所述",
        "对应",
        "相应",
        "相连",
        "相接",
        "接触",
        "贴合",
        "贯穿",
        "延伸",
        "朝向",
        "沿着",
        "基于",
        "根据",
        "按照",
        "处于",
        "保持",
        "使得",
        "从而",
        "进而",
        "并",
    ];
    const SINGLE_STOP_CHARS: &[char] = &[
        '的', '地', '得', '与', '和', '及', '或', '并', '中', '内', '外', '上', '下', '向', '对',
        '将', '被', '由', '为', '是', '在', '以', '至', '到', '从', '于', '给', '把',
    ];

    let chars: Vec<char> = content.chars().collect();
    let mut results = Vec::new();
    let mut i = 0usize;

    while i + 1 < chars.len() {
        if chars[i] == '所' && chars[i + 1] == '述' {
            // 跳过 “如前所述 / 如上所述 / 前述 / 上述”
            if i >= 1 && (chars[i - 1] == '前' || chars[i - 1] == '上') {
                i += 2;
                continue;
            }
            let mut start = i + 2;
            if start < chars.len() && chars[start] == '的' {
                start += 1;
            }

            let mut term = String::new();
            let mut j = start;
            while j < chars.len() && term.chars().count() < 16 {
                let ch = chars[j];
                if is_clause_delimiter(ch) {
                    break;
                }
                if !term.is_empty() {
                    if SINGLE_STOP_CHARS.contains(&ch) && term.chars().count() >= 2 {
                        break;
                    }
                    let rest: String = chars.iter().skip(j).take(8).collect();
                    if MULTI_STOP_TOKENS.iter().any(|tok| rest.starts_with(tok)) {
                        break;
                    }
                }
                term.push(ch);
                j += 1;
            }

            let trimmed_term = term.trim();
            if trimmed_term.chars().count() >= 2 {
                results.push((i, trimmed_term.to_string()));
            }
            i = start;
        } else {
            i += 1;
        }
    }

    results
}

fn is_clause_delimiter(c: char) -> bool {
    c.is_whitespace()
        || matches!(
            c,
            '，' | '。'
                | '、'
                | '；'
                | '：'
                | '（'
                | '）'
                | '《'
                | '》'
                | '“'
                | '”'
                | ','
                | '.'
                | ';'
                | ':'
                | '('
                | ')'
                | '['
                | ']'
                | '{'
                | '}'
                | '/'
        )
}

fn has_antecedent_basis(term: &str, antecedent_corpus: &str) -> bool {
    let norm_term = normalize_for_match(term);
    let norm_corpus = normalize_for_match(antecedent_corpus);
    if norm_term.is_empty() || norm_corpus.is_empty() {
        return false;
    }
    norm_corpus.contains(&norm_term)
}

fn normalize_for_match(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

fn extract_numeric_constraints(content: &str) -> Vec<NumericConstraint> {
    let mut constraints = Vec::new();
    let chars: Vec<char> = content.chars().collect();

    // 先匹配范围表达式（如 150-200℃、0.5~2.0mm、10%至30%）
    if let Ok(re_range) = Regex::new(
        r"(\d+(?:\.\d+)?)\s*%?\s*(?:[-~～至到－])\s*(\d+(?:\.\d+)?)\s*(%|℃|°C|°c|K|mm|cm|m|μm|nm|MPa|kPa|Pa|mV|V|mA|A|kW|W|MHz|kHz|Hz|ms|min|s|h|rpm|kg|g|mL|L|Nm|N|Ω|dB|倍|个|次|层|级|路|组)?",
    ) {
        for m in re_range.captures_iter(content) {
            let Some(full) = m.get(0) else { continue };
            let Some(lo) = m.get(1) else { continue };
            let Some(hi) = m.get(2) else { continue };
            // 排除 "权利要求1-3" / "权1-3" 这类权项编号范围
            let prefix_chars: String = chars_before_byte(&chars, content, full.start(), 6);
            if prefix_chars.contains("权利要求") || prefix_chars.ends_with('权') {
                continue;
            }
            let unit = m.get(3).map(|u| u.as_str().to_string()).unwrap_or_default();
            constraints.push(NumericConstraint {
                raw: full.as_str().trim().to_string(),
                numbers: vec![lo.as_str().to_string(), hi.as_str().to_string()],
                unit,
                start_char: byte_to_char_idx(content, full.start()),
                end_char: byte_to_char_idx(content, full.end()),
            });
        }
    }

    // 再匹配带工程单位的单值或比较词限定（如 大于80%、100℃、50MPa）
    if let Ok(re_unit) = Regex::new(
        r"(?:(?:大于等于|小于等于|大于|小于|高于|低于|不超过|不少于|至少|至多|>=|<=|>|<|约为?|为)\s*)?(\d+(?:\.\d+)?)\s*(%|℃|°C|°c|mm|cm|μm|nm|MPa|kPa|Pa|mV|V|mA|A|kW|W|MHz|kHz|Hz|ms|rpm|kg|mL|Nm|Ω|dB)",
    ) {
        for m in re_unit.captures_iter(content) {
            let Some(full) = m.get(0) else { continue };
            let Some(val) = m.get(1) else { continue };
            let start_c = byte_to_char_idx(content, full.start());
            let end_c = byte_to_char_idx(content, full.end());
            // 避免与已识别的范围重叠
            let overlaps = constraints
                .iter()
                .any(|c| start_c < c.end_char && end_c > c.start_char);
            if overlaps {
                continue;
            }
            let unit = m.get(2).map(|u| u.as_str().to_string()).unwrap_or_default();
            constraints.push(NumericConstraint {
                raw: full.as_str().trim().to_string(),
                numbers: vec![val.as_str().to_string()],
                unit,
                start_char: start_c,
                end_char: end_c,
            });
        }
    }

    constraints
}

fn chars_before_byte(chars: &[char], full: &str, byte_idx: usize, count: usize) -> String {
    let cidx = byte_to_char_idx(full, byte_idx);
    let start = cidx.saturating_sub(count);
    chars.iter().skip(start).take(cidx - start).collect()
}

fn byte_to_char_idx(s: &str, byte_idx: usize) -> usize {
    let mut count = 0usize;
    for (b, _) in s.char_indices() {
        if b >= byte_idx {
            break;
        }
        count += 1;
    }
    count
}

fn numeric_supported_in_corpus(nc: &NumericConstraint, normalized_corpus: &str) -> bool {
    if normalized_corpus.is_empty() || nc.numbers.is_empty() {
        return false;
    }
    let norm_raw = normalize_for_match(&nc.raw).replace("°c", "℃");
    let norm_corpus = normalized_corpus.replace("°c", "℃");
    if norm_corpus.contains(&norm_raw) {
        return true;
    }
    // 统一范围连接符后检查（如 20-60℃ 与 20~60℃ / 20至60℃ 互认）
    if nc.numbers.len() == 2 {
        let lo = &nc.numbers[0];
        let hi = &nc.numbers[1];
        let unit = nc.unit.to_ascii_lowercase().replace("°c", "℃");
        let unit_ok = unit.is_empty() || norm_corpus.contains(&unit);
        return unit_ok
            && contains_standalone_number(&norm_corpus, lo)
            && contains_standalone_number(&norm_corpus, hi);
    }
    let val = &nc.numbers[0];
    let unit = nc.unit.to_ascii_lowercase().replace("°c", "℃");
    let unit_ok = unit.is_empty() || norm_corpus.contains(&unit);
    unit_ok && contains_standalone_number(&norm_corpus, val)
}

fn contains_standalone_number(haystack: &str, num: &str) -> bool {
    if num.is_empty() {
        return false;
    }
    let bytes = haystack.as_bytes();
    let mut start = 0usize;
    while let Some(rel) = haystack.get(start..).and_then(|sub| sub.find(num)) {
        let abs = start + rel;
        let end = abs + num.len();
        let left_ok = abs == 0
            || !bytes
                .get(abs - 1)
                .map(|b| b.is_ascii_digit() || *b == b'.')
                .unwrap_or(false);
        let right_ok = end >= bytes.len()
            || !bytes
                .get(end)
                .map(|b| b.is_ascii_digit() || *b == b'.')
                .unwrap_or(false);
        if left_ok && right_ok {
            return true;
        }
        start = abs + 1;
    }
    false
}

fn extract_functional_limitations(content: &str) -> Vec<String> {
    let mut out = Vec::new();
    let markers = ["用于", "配置为", "能够", "以实现", "使得", "从而"];
    for clause in content.split(is_clause_delimiter) {
        let trimmed = clause.trim();
        for m in &markers {
            if let Some(pos) = trimmed.find(m) {
                if let Some(sub) = trimmed.get(pos..) {
                    if sub.chars().count() >= 6 {
                        out.push(sub.to_string());
                        break;
                    }
                }
            }
        }
    }
    out
}

fn functional_supported_in_corpus(phrase: &str, normalized_corpus: &str) -> bool {
    let stripped = phrase
        .trim_start_matches("用于")
        .trim_start_matches("配置为")
        .trim_start_matches("能够")
        .trim_start_matches("以实现")
        .trim_start_matches("使得")
        .trim_start_matches("从而");
    let chars: Vec<char> = stripped
        .chars()
        .filter(|c| !c.is_whitespace() && !c.is_ascii_punctuation())
        .collect();
    if chars.len() < 3 {
        return true;
    }
    let mut total_bigrams = 0usize;
    let mut hits = 0usize;
    for w in chars.windows(2) {
        let bg: String = w.iter().collect();
        total_bigrams += 1;
        if normalized_corpus.contains(&bg.to_ascii_lowercase()) {
            hits += 1;
        }
    }
    if total_bigrams == 0 {
        return true;
    }
    (hits as f64) / (total_bigrams as f64) >= 0.25
}

fn extract_ref_clause_snippet(content: &str) -> String {
    for clause in content.split(|c| matches!(c, '，' | '。' | ',' | '.')) {
        if clause.contains("权利要求") || clause.to_ascii_lowercase().contains("claim") {
            return preview_chars(clause.trim(), 48);
        }
    }
    preview_chars(content, 48)
}

fn preview_chars(s: &str, max_chars: usize) -> String {
    let mut out: String = s.chars().take(max_chars).collect();
    if s.chars().count() > max_chars {
        out.push('…');
    }
    out
}

/// 可选专家模型复核第26条第4款实施例支撑（失败或输出非 JSON 时平滑降级返回 None）
async fn run_ai_article_26_4_review(
    ai: &AiClient,
    claims: &[ClaimInput],
    spec_chunks: &[String],
) -> Option<Vec<ClaimValidationIssue>> {
    let claims_text: String = claims
        .iter()
        .take(20)
        .map(|c| format!("{}. {}", c.claim_number, c.content.trim()))
        .collect::<Vec<_>>()
        .join("\n");
    let spec_excerpt = truncate_for_ai(&spec_chunks.join("\n---\n"), 3600);

    let prompt = format!(
        "你是中国国家知识产权局资深实质审查员。请仅依据下方 <user_input> 中提供的权利要求与说明书/chunks_fts实施例片段，\
         核查是否存在违反《专利法》第26条第4款（得不到说明书支持，尤其是无实施例支撑的数值范围、过度概括的功能性限定、缺少必要技术特征）的问题。\n\
         严格返回合法 JSON，不要包含 markdown 代码块或额外解释：\n\
         {{\"issues\":[{{\"claim_number\":1,\"rule_id\":\"ART26_4_NUMERIC_UNSUPPORTED\",\"severity\":\"warning\",\"title\":\"...\",\"message\":\"...\",\"snippet\":\"...\",\"suggestion\":\"...\"}}]}}\n\
         若无新增第26条第4款缺陷，返回 {{\"issues\":[]}}。\n\n\
         <user_input>\n[权利要求书]\n{}\n\n[说明书与检索实施例分块]\n{}\n</user_input>",
        truncate_for_ai(&claims_text, 2400),
        spec_excerpt,
    );

    let resp = match ai.chat(&prompt, None).await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(
                "claim_validator AI 第26条第4款复核失败，降级为确定性结果: {}",
                e
            );
            return None;
        }
    };

    let cleaned = resp
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    let parsed: serde_json::Value = match serde_json::from_str(cleaned) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("claim_validator AI 输出 JSON 解析失败，降级: {}", e);
            return None;
        }
    };

    let arr = parsed.get("issues")?.as_array()?;
    let mut issues = Vec::new();
    for item in arr.iter().take(20) {
        let claim_number = item
            .get("claim_number")
            .and_then(|v| v.as_u64())
            .unwrap_or(1) as u32;
        let rule_id = item
            .get("rule_id")
            .and_then(|v| v.as_str())
            .unwrap_or("ART26_4_FUNCTIONAL_UNSUPPORTED")
            .to_string();
        let severity = match item.get("severity").and_then(|v| v.as_str()) {
            Some("fatal") => ClaimIssueSeverity::Fatal,
            Some("info") => ClaimIssueSeverity::Info,
            _ => ClaimIssueSeverity::Warning,
        };
        let title = item
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("第26条第4款支撑风险")
            .to_string();
        let message = item
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if message.trim().is_empty() {
            continue;
        }
        let snippet = item
            .get("snippet")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let suggestion = item
            .get("suggestion")
            .and_then(|v| v.as_str())
            .unwrap_or("请在说明书中补充对应实施例或限缩保护范围。")
            .to_string();

        issues.push(ClaimValidationIssue {
            claim_number,
            rule_id,
            category: "article_26_4".to_string(),
            severity,
            title,
            message,
            snippet,
            suggestion,
            evidence_chunks: Vec::new(),
        });
    }

    Some(issues)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::rag::PatentChunk;
    use crate::db::Database;
    use crate::patent::Patent;

    fn make_claim(num: u32, claim_type: &str, content: &str) -> ClaimInput {
        ClaimInput {
            id: Some(format!("c-{}", num)),
            claim_number: num,
            claim_type: claim_type.to_string(),
            parent_claim_id: None,
            depends_on: Vec::new(),
            content: content.to_string(),
        }
    }

    #[test]
    fn clean_claim_tree_with_supported_ranges_passes() {
        let claims = vec![
            make_claim(
                1,
                "independent",
                "一种热管理电池箱，包括壳体、设置在壳体内的电芯模组以及导热板，其特征在于，所述导热板与电芯模组之间填充导热硅胶，工作温度范围为20-60℃。",
            ),
            make_claim(
                2,
                "dependent",
                "根据权利要求1所述的热管理电池箱，其特征在于，所述电芯模组外侧设有散热鳍片。",
            ),
        ];
        let spec = vec![
            "具体实施方式：在实施例中，热管理电池箱包括壳体、电芯模组和导热板，导热板与电芯模组之间填充导热硅胶，将工作温度控制在20~60℃范围内；电芯模组外侧设置有散热鳍片以增强换热。".to_string(),
        ];

        let report = validate_claims_deterministic("idea-clean", &claims, &spec);
        assert!(
            report.passed,
            "合法权利要求树应通过校验: {:?}",
            report.issues
        );
        assert_eq!(report.fatal_count, 0);
        assert_eq!(report.warning_count, 0);
    }

    #[test]
    fn detects_missing_antecedent_basis() {
        let claims = vec![
            make_claim(
                1,
                "independent",
                "一种减震支架，包括底座和固定连接于底座上的支撑臂。",
            ),
            make_claim(
                2,
                "dependent",
                "根据权利要求1所述的减震支架，其特征在于，所述阻尼弹簧套设于所述支撑臂外部。",
            ),
        ];
        let spec = vec!["实施例：减震支架包括底座、支撑臂以及可选的支撑结构。".to_string()];

        let report = validate_claims_deterministic("idea-antecedent", &claims, &spec);
        assert!(!report.passed);
        assert!(
            report.issues.iter().any(|i| {
                i.claim_number == 2
                    && i.rule_id == "SYNTAX_ANTECEDENT_BASIS"
                    && i.snippet == "所述阻尼弹簧"
                    && i.severity == ClaimIssueSeverity::Fatal
            }),
            "必须检出“所述阻尼弹簧”缺少引用基础: {:?}",
            report.issues
        );
        // “所述支撑臂”在权利要求1中已引入，不应误报
        assert!(
            !report
                .issues
                .iter()
                .any(|i| i.rule_id == "SYNTAX_ANTECEDENT_BASIS" && i.snippet == "所述支撑臂"),
            "已存在引用基础的“所述支撑臂”不应误报: {:?}",
            report.issues
        );
    }

    #[test]
    fn detects_multiple_dependent_on_multiple_dependent() {
        let claims = vec![
            make_claim(
                1,
                "independent",
                "一种流体控制阀，包括阀体、阀芯和驱动电机。",
            ),
            make_claim(
                2,
                "dependent",
                "根据权利要求1所述的流体控制阀，其特征在于，所述阀芯表面设有密封圈。",
            ),
            make_claim(
                3,
                "dependent",
                "根据权利要求1或2所述的流体控制阀，其特征在于，所述驱动电机连接有编码器。",
            ),
            make_claim(
                4,
                "dependent",
                "根据权利要求2或3所述的流体控制阀，其特征在于，所述编码器连接至控制器。",
            ),
        ];
        let spec = vec![
            "具体实施方式：阀体、阀芯、驱动电机、密封圈、编码器及控制器按常规方式连接。"
                .to_string(),
        ];

        let report = validate_claims_deterministic("idea-multi-dep", &claims, &spec);
        assert!(!report.passed);
        assert!(
            report.issues.iter().any(|i| {
                i.claim_number == 4
                    && i.rule_id == "SYNTAX_MULTI_DEP_CHAIN"
                    && i.severity == ClaimIssueSeverity::Fatal
            }),
            "必须检出权利要求4多项从属引用多项从属权利要求3: {:?}",
            report.issues
        );
    }

    #[test]
    fn detects_unsupported_numeric_range_article_26_4() {
        let claims = vec![make_claim(
            1,
            "independent",
            "一种高温烧结炉，包括炉膛和加热元件，其特征在于，所述炉膛工作温度范围为150-200℃。",
        )];
        let spec = vec![
            "具体实施方式：实施例1中烧结炉包括炉膛和加热元件，实测工作温度控制在20-60℃之间，以保证能耗稳定。".to_string(),
        ];

        let report = validate_claims_deterministic("idea-numeric", &claims, &spec);
        assert!(!report.passed);
        assert!(
            report.issues.iter().any(|i| {
                i.claim_number == 1
                    && i.rule_id == "ART26_4_NUMERIC_UNSUPPORTED"
                    && i.snippet.contains("150-200")
            }),
            "必须检出无说明书实施例支撑的数值范围 150-200℃: {:?}",
            report.issues
        );
    }

    #[tokio::test]
    async fn validate_claims_queries_chunks_fts_and_gracefully_handles_offline_ai() {
        let db = Database::init(":memory:").expect("init memory db");
        let patent = Patent {
            id: "pat-emb-1".to_string(),
            patent_number: "CN99900001A".to_string(),
            title: "一种热管理电池箱".to_string(),
            abstract_text: "包括壳体与导热板".to_string(),
            claims: String::new(),
            description: "具体实施方式中工作温度范围为20-60℃".to_string(),
            applicant: String::new(),
            inventor: String::new(),
            filing_date: String::new(),
            publication_date: String::new(),
            ipc_codes: String::new(),
            source: "local".to_string(),
            legal_status: "有效".to_string(),
            pdf_url: String::new(),
            created_at: "2026-11-18T00:00:00Z".to_string(),
        };
        db.insert_patent(&patent).expect("insert patent");
        db.save_patent_chunks(
            "pat-emb-1",
            &[PatentChunk {
                id: "chunk-emb-1".to_string(),
                patent_id: "pat-emb-1".to_string(),
                chunk_index: 0,
                source_type: "description".to_string(),
                section_tag: "Desc:具体实施方式".to_string(),
                content: "实施例1：热管理电池箱包括壳体、电芯模组与导热板，实测工作温度为20-60℃，未涉及更高温度区间。".to_string(),
                embedding: vec![],
            }],
            "none",
        )
        .expect("save chunks");

        // 权1用 20-60℃ 有 chunks_fts 支撑；权2用 180-220℃ 无支撑
        let claims = vec![
            make_claim(
                1,
                "independent",
                "一种热管理电池箱，包括壳体、电芯模组与导热板，工作温度为20-60℃。",
            ),
            make_claim(
                2,
                "dependent",
                "根据权利要求1所述的热管理电池箱，其特征在于，热处理温度为180-220℃。",
            ),
        ];

        let report = validate_claims("idea-fts", &claims, &[], Some(&db), None).await;
        assert!(report.fts_hits_count >= 1, "应从 chunks_fts 检出实施例分块");
        assert!(
            !report
                .issues
                .iter()
                .any(|i| i.claim_number == 1 && i.rule_id == "ART26_4_NUMERIC_UNSUPPORTED"),
            "权1的 20-60℃ 已在 chunks_fts 中支撑，不应告警: {:?}",
            report.issues
        );
        assert!(
            report.issues.iter().any(|i| {
                i.claim_number == 2
                    && i.rule_id == "ART26_4_NUMERIC_UNSUPPORTED"
                    && i.snippet.contains("180-220")
                    && !i.evidence_chunks.is_empty()
            }),
            "权2的 180-220℃ 应检出无实施例支撑并附带检索分块记录: {:?}",
            report.issues
        );
    }
}
