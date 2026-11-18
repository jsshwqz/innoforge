//! 卡片 ⑤ 审查与审稿三栖抗辩核心业务服务（`src/cards/rebuttal_hub/service.rs`）
//!
//! 职责：
//! - 三栖案件创建与多轮历史（`patent_oa / patent_pre_mock / paper_rebuttal`），持久化到 SQLite `oa_rounds` 与 `app_settings` (`rebuttal_case:{id}`)；
//! - 绝限倒计时计算（复用 `crate::oa_deadline::compute_deadline`）；
//! - `mock_review`：结合对比专利与真实 DOI 论文生成高强度质疑清单；
//! - `draft`：输出「修改权项/正文 vs 纯争辩」双轨策略裁决与逐条绑定原文/实验 `Exp:{id}` 锚点的答复草稿；
//! - `verify`：调用 `EvidenceRebuttalService` 执行 O(1) 锚点核验、跨实施例拼凑识破、第 33 条修改超范围/论文返修一致性硬门禁与五维体检单；
//! - `export`：调用 `ForgeExporter` 产出带 `<w:del>`/`<w:ins>` Word 修订留痕的 `.docx` 答复包。

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{anyhow, Result};
use chrono::{Local, NaiveDate};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::core::evidence_rebuttal::{EvidenceRebuttalService, GateEvaluationInput};
use crate::core::runtime::forge_export::{
    CompileDeliverableRequest, CompiledDeliverable, ForgeExporter,
};
use crate::core::runtime::llm_router::{
    builtin_nine_providers, ollama_local_provider, DailyBudgetTracker, LlmProviderSpec, LlmRouter,
};
use crate::core::vault::models::{
    DeliverableFormat, DeliverableGenre, FiveDimGateReport, KnowledgeDoc, SearchSceneMode,
    StitchingAlert, VerifiedCitation,
};
use crate::core::vault::VaultService;
use crate::db::oa::OaRound;
use crate::oa_deadline::{compute_deadline, is_valid_oa_type, DeadlineResult};
use crate::routes::{AppConfig, AppState};

use super::prompts::{
    build_draft_user_payload, build_mock_review_user_payload, draft_rebuttal_system_prompt,
    mock_review_system_prompt, RebuttalMode,
};

const CASE_KEY_PREFIX: &str = "rebuttal_case:";

/// 模拟审查 / 同行评审单条质疑项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewIssueItem {
    pub issue_id: String,
    pub severity: String,
    pub category: String,
    pub title: String,
    pub detail: String,
    #[serde(default)]
    pub cited_anchors: Vec<String>,
    #[serde(default)]
    pub recommended_track: String,
}

/// 逐条答复条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PointResponseItem {
    pub issue_id: String,
    pub track: String,
    pub response_text: String,
    #[serde(default)]
    pub bound_anchors: Vec<String>,
}

/// 双轨策略裁决与答复草稿结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RebuttalDraftResult {
    pub strategy_track: String,
    pub strategy_rationale: String,
    #[serde(default)]
    pub point_by_point_responses: Vec<PointResponseItem>,
    pub full_draft_text: String,
    pub suggested_amended_text: String,
    pub provider_used: String,
    pub generated_offline_fallback: bool,
}

/// 单轮抗辩/返修历史记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RebuttalRoundRecord {
    pub round_id: String,
    pub round_number: i32,
    pub round_label: String,
    pub oa_type: String,
    pub dispatch_date: Option<String>,
    pub deadline_info: Option<DeadlineResult>,
    pub notice_text: String,
    pub strategy_used: Option<String>,
    pub draft_text: Option<String>,
    pub gate_decision: Option<String>,
    pub overall_score: Option<f64>,
    pub created_at: String,
}

/// 三栖抗辩案件主实体（持久化于 `app_settings` `rebuttal_case:{id}`）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RebuttalCase {
    pub case_id: String,
    pub mode: RebuttalMode,
    pub title: String,
    pub own_doc_id: Option<String>,
    #[serde(default)]
    pub reference_doc_ids: Vec<String>,
    #[serde(default)]
    pub experiment_ids: Vec<String>,
    pub oa_type: Option<String>,
    pub dispatch_date: Option<String>,
    pub deadline_info: Option<DeadlineResult>,
    #[serde(default)]
    pub original_claim_or_text: String,
    #[serde(default)]
    pub amended_claim_or_text: String,
    #[serde(default)]
    pub notice_or_review_text: String,
    #[serde(default)]
    pub knowhow_markers: Vec<String>,
    #[serde(default)]
    pub rounds: Vec<RebuttalRoundRecord>,
    #[serde(default)]
    pub latest_mock_issues: Vec<ReviewIssueItem>,
    pub latest_draft: Option<RebuttalDraftResult>,
    pub latest_gate_report: Option<FiveDimGateReport>,
    pub created_at: String,
    pub updated_at: String,
}

/// 创建案件请求载荷
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateCaseInput {
    pub mode: String,
    pub title: String,
    pub own_doc_id: Option<String>,
    #[serde(default)]
    pub reference_doc_ids: Vec<String>,
    #[serde(default)]
    pub experiment_ids: Vec<String>,
    pub oa_type: Option<String>,
    pub dispatch_date: Option<String>,
    #[serde(default)]
    pub original_claim_or_text: String,
    #[serde(default)]
    pub amended_claim_or_text: String,
    #[serde(default)]
    pub notice_or_review_text: String,
    #[serde(default)]
    pub knowhow_markers: Vec<String>,
}

/// 新增轮次请求载荷
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddRoundInput {
    pub round_label: Option<String>,
    pub oa_type: Option<String>,
    pub dispatch_date: Option<String>,
    pub notice_text: String,
    pub strategy_used: Option<String>,
    pub draft_text: Option<String>,
    pub notes: Option<String>,
}

/// 核验完整输出包
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RebuttalVerifyOutcome {
    pub case_id: String,
    pub mode: RebuttalMode,
    pub gate_report: FiveDimGateReport,
    pub scope_report: crate::core::evidence_rebuttal::ScopeConsistencyReport,
    pub stitching_alerts: Vec<StitchingAlert>,
    pub priority_check: crate::core::vault::models::PriorityCheckResult,
}

fn now_iso() -> String {
    Local::now().format("%Y-%m-%dT%H:%M:%S").to_string()
}

fn case_storage_key(case_id: &str) -> String {
    format!("{}{}", CASE_KEY_PREFIX, case_id.trim())
}

fn safe_truncate_chars(text: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for (idx, ch) in text.chars().enumerate() {
        if idx >= max_chars {
            out.push_str("...");
            break;
        }
        out.push(ch);
    }
    out
}

fn compute_deadline_opt(
    oa_type: Option<&str>,
    dispatch_date: Option<&str>,
) -> Option<DeadlineResult> {
    let raw_date = dispatch_date?.trim();
    if raw_date.is_empty() {
        return None;
    }
    let parsed_date = NaiveDate::parse_from_str(raw_date, "%Y-%m-%d").ok()?;
    let effective_type = oa_type
        .map(str::trim)
        .filter(|t| !t.is_empty() && is_valid_oa_type(t))
        .unwrap_or("first_exam");
    Some(compute_deadline(effective_type, parsed_date))
}

fn target_genre_for_mode(mode: RebuttalMode) -> DeliverableGenre {
    match mode {
        RebuttalMode::PaperRebuttal => DeliverableGenre::PaperRebuttalPackage,
        RebuttalMode::PatentOa | RebuttalMode::PatentPreMock => DeliverableGenre::OaPackage,
    }
}

fn build_llm_router_from_config(cfg: &AppConfig, conn: &Connection) -> Result<LlmRouter> {
    let mut providers: Vec<LlmProviderSpec> = builtin_nine_providers()
        .into_iter()
        .map(|mut p| {
            let key = match p.id.as_str() {
                "deepseek" => &cfg.ai_api_key_deepseek,
                "anthropic" => &cfg.ai_api_key_anthropic,
                "xiaomi" => &cfg.ai_api_key_xiaomi,
                "sensenova" => &cfg.ai_api_key_sensetime,
                "openrouter" => &cfg.ai_api_key_openrouter,
                "gemini" => &cfg.ai_api_key_gemini,
                "zhipu" => &cfg.ai_api_key_zhipu,
                "qwen" => &cfg.ai_api_key_qwen,
                "openai" => &cfg.ai_api_key_openai,
                _ => &cfg.ai_api_key,
            };
            if !key.trim().is_empty() {
                p.api_key = key.trim().to_string();
            }
            p
        })
        .collect();

    let base_lower = cfg.ai_base_url.trim().to_ascii_lowercase();
    if base_lower.starts_with("http://127.0.0.1") || base_lower.starts_with("http://localhost") {
        let model = if cfg.ai_model.trim().is_empty() {
            None
        } else {
            Some(cfg.ai_model.as_str())
        };
        providers.push(ollama_local_provider(model, false));
    }

    match DailyBudgetTracker::from_sqlite(conn, 500) {
        Ok(tracker) => LlmRouter::with_budget_tracker(providers, std::sync::Arc::new(tracker)),
        Err(_) => LlmRouter::new(providers, 500),
    }
}

fn extract_json_block(raw: &str) -> Option<serde_json::Value> {
    let trimmed = raw.trim();
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed) {
        return Some(val);
    }
    if let (Some(start), Some(end)) = (trimmed.find('{'), trimmed.rfind('}')) {
        if start < end {
            if let Some(slice) = trimmed.get(start..=end) {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(slice) {
                    return Some(val);
                }
            }
        }
    }
    None
}

fn collect_evidence_context(
    conn: &Connection,
    case: &RebuttalCase,
    extra_query: &str,
) -> Result<(String, Vec<KnowledgeDoc>, Vec<String>)> {
    let _ = VaultService::ensure_tables(conn);

    let mut docs = Vec::new();
    let mut all_anchors = Vec::new();
    let mut lines = Vec::new();

    if let Some(own_id) = case.own_doc_id.as_deref().filter(|s| !s.trim().is_empty()) {
        if let Some(doc) = VaultService::get_document(conn, own_id)? {
            lines.push(format!(
                "【我方主体文献】doc_id={}, kind={}, title={}, filing_status={}",
                doc.doc_id,
                doc.doc_kind.as_str(),
                if doc.title_zh.is_empty() {
                    &doc.title_en
                } else {
                    &doc.title_zh
                },
                doc.filing_status.as_str()
            ));
            let bundle = VaultService::hybrid_search_chunks(
                conn,
                if extra_query.trim().is_empty() {
                    &case.title
                } else {
                    extra_query
                },
                SearchSceneMode::AnchorExact,
                Some(std::slice::from_ref(&doc.doc_id)),
                None,
                None,
                6,
                4000,
            )?;
            for hit in bundle.hits {
                let anchor = format!("[{}#{}]", hit.chunk.doc_id, hit.chunk.section_tag);
                all_anchors.push(anchor.clone());
                lines.push(format!(
                    "  - {} ({}): {}",
                    anchor,
                    hit.chunk.embodiment_group.as_deref().unwrap_or("通用"),
                    safe_truncate_chars(&hit.chunk.raw_content, 240)
                ));
            }
            docs.push(doc);
        }
    }

    for ref_id in &case.reference_doc_ids {
        let trimmed = ref_id.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(doc) = VaultService::get_document(conn, trimmed)? {
            let doi_note = if doc.doi_verified {
                format!("DOI已核验({})", doc.external_id)
            } else {
                format!("文献标识({})", doc.external_id)
            };
            lines.push(format!(
                "【对比/参考文献】doc_id={}, kind={}, {}, title={}",
                doc.doc_id,
                doc.doc_kind.as_str(),
                doi_note,
                if doc.title_zh.is_empty() {
                    &doc.title_en
                } else {
                    &doc.title_zh
                }
            ));
            let bundle = VaultService::hybrid_search_chunks(
                conn,
                if extra_query.trim().is_empty() {
                    &case.title
                } else {
                    extra_query
                },
                SearchSceneMode::PatentCompare,
                Some(std::slice::from_ref(&doc.doc_id)),
                None,
                None,
                5,
                4000,
            )?;
            for hit in bundle.hits {
                let anchor = format!("[{}#{}]", hit.chunk.doc_id, hit.chunk.section_tag);
                all_anchors.push(anchor.clone());
                lines.push(format!(
                    "  - {} (实施例={}): {}",
                    anchor,
                    hit.chunk.embodiment_group.as_deref().unwrap_or("未标注"),
                    safe_truncate_chars(&hit.chunk.raw_content, 220)
                ));
            }
            docs.push(doc);
        }
    }

    for exp_id in &case.experiment_ids {
        let trimmed = exp_id.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(exp) = VaultService::get_experiment(conn, trimmed)? {
            let anchor = format!("[Exp:{}]", exp.exp_id);
            all_anchors.push(anchor.clone());
            lines.push(format!(
                "【对照实验实测】{} {} | 状态={:?} | 实测数据={}",
                anchor,
                exp.title,
                exp.status,
                exp.measured_data_json.as_deref().unwrap_or("待回填")
            ));
        }
    }

    Ok((lines.join("\n"), docs, all_anchors))
}

pub fn load_case(state: &AppState, case_id: &str) -> Result<RebuttalCase> {
    let key = case_storage_key(case_id);
    let raw = state
        .db
        .get_setting(&key)?
        .ok_or_else(|| anyhow!("抗辩案件不存在: {}", case_id))?;
    let mut case: RebuttalCase = serde_json::from_str(&raw)?;
    case.deadline_info =
        compute_deadline_opt(case.oa_type.as_deref(), case.dispatch_date.as_deref());
    Ok(case)
}

pub fn save_case(state: &AppState, case: &RebuttalCase) -> Result<()> {
    let key = case_storage_key(&case.case_id);
    let json = serde_json::to_string(case)?;
    state.db.set_setting(&key, &json)?;
    Ok(())
}

/// 创建三栖抗辩案件
pub fn create_case(state: &AppState, input: CreateCaseInput) -> Result<RebuttalCase> {
    let mode = RebuttalMode::parse(&input.mode);
    let case_id = format!("reb_{}", Uuid::new_v4().simple());
    let now = now_iso();
    let deadline_info =
        compute_deadline_opt(input.oa_type.as_deref(), input.dispatch_date.as_deref());

    let title = if input.title.trim().is_empty() {
        match mode {
            RebuttalMode::PatentOa => "未命名专利 OA 抗辩案".to_string(),
            RebuttalMode::PatentPreMock => "未命名新专利魔鬼预审案".to_string(),
            RebuttalMode::PaperRebuttal => "未命名期刊论文 Rebuttal 案".to_string(),
        }
    } else {
        input.title.trim().to_string()
    };

    let mut rounds = Vec::new();
    if !input.notice_or_review_text.trim().is_empty() {
        rounds.push(RebuttalRoundRecord {
            round_id: format!("rnd_{}", Uuid::new_v4().simple()),
            round_number: 1,
            round_label: match mode {
                RebuttalMode::PatentOa => "一通 OA1".to_string(),
                RebuttalMode::PatentPreMock => "Pre-Filing R1".to_string(),
                RebuttalMode::PaperRebuttal => "R1 Major/Minor".to_string(),
            },
            oa_type: input
                .oa_type
                .clone()
                .unwrap_or_else(|| "first_exam".to_string()),
            dispatch_date: input.dispatch_date.clone(),
            deadline_info: deadline_info.clone(),
            notice_text: input.notice_or_review_text.clone(),
            strategy_used: None,
            draft_text: None,
            gate_decision: None,
            overall_score: None,
            created_at: now.clone(),
        });
    }

    let case = RebuttalCase {
        case_id,
        mode,
        title,
        own_doc_id: input.own_doc_id.filter(|s| !s.trim().is_empty()),
        reference_doc_ids: input.reference_doc_ids,
        experiment_ids: input.experiment_ids,
        oa_type: input.oa_type.filter(|s| !s.trim().is_empty()),
        dispatch_date: input.dispatch_date.filter(|s| !s.trim().is_empty()),
        deadline_info,
        original_claim_or_text: input.original_claim_or_text,
        amended_claim_or_text: input.amended_claim_or_text,
        notice_or_review_text: input.notice_or_review_text,
        knowhow_markers: input.knowhow_markers,
        rounds,
        latest_mock_issues: Vec::new(),
        latest_draft: None,
        latest_gate_report: None,
        created_at: now.clone(),
        updated_at: now,
    };

    save_case(state, &case)?;
    Ok(case)
}

/// 新增一通→二通 / R1→R2 轮次，并同步写入 `oa_rounds` 表与 `app_settings`
pub fn add_round(
    state: &AppState,
    case_id: &str,
    input: AddRoundInput,
) -> Result<(RebuttalCase, RebuttalRoundRecord)> {
    let mut case = load_case(state, case_id)?;
    let patent_key = case
        .own_doc_id
        .clone()
        .unwrap_or_else(|| case.case_id.clone());

    let db_next = state.db.next_round_number(&patent_key).unwrap_or(1);
    let round_number = std::cmp::max(db_next, (case.rounds.len() as i32) + 1);
    let round_type = input
        .oa_type
        .clone()
        .or_else(|| case.oa_type.clone())
        .unwrap_or_else(|| "second_rejection".to_string());
    let round_label = input
        .round_label
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| match case.mode {
            RebuttalMode::PatentOa => format!("第{}通 OA{}", round_number, round_number),
            RebuttalMode::PatentPreMock => format!("Pre-Filing R{}", round_number),
            RebuttalMode::PaperRebuttal => format!("R{} Revision", round_number),
        });

    let dispatch_date = input.dispatch_date.or_else(|| case.dispatch_date.clone());
    let deadline_info = compute_deadline_opt(Some(&round_type), dispatch_date.as_deref());
    let now = now_iso();
    let round_id = format!("rnd_{}", Uuid::new_v4().simple());

    let oa_round = OaRound {
        id: round_id.clone(),
        patent_id: patent_key,
        round_number,
        round_type: round_type.clone(),
        oa_date: dispatch_date.clone(),
        oa_text: Some(input.notice_text.clone()),
        response_date: None,
        response_text: input.draft_text.clone(),
        result: None,
        result_date: None,
        strategy_used: input.strategy_used.clone(),
        quality_score: None,
        notes: input.notes,
        created_at: now.clone(),
        updated_at: now.clone(),
    };
    let _ = state.db.save_oa_round(&oa_round);

    let record = RebuttalRoundRecord {
        round_id,
        round_number,
        round_label,
        oa_type: round_type.clone(),
        dispatch_date: dispatch_date.clone(),
        deadline_info: deadline_info.clone(),
        notice_text: input.notice_text.clone(),
        strategy_used: input.strategy_used,
        draft_text: input.draft_text,
        gate_decision: None,
        overall_score: None,
        created_at: now.clone(),
    };

    case.oa_type = Some(round_type);
    if dispatch_date.is_some() {
        case.dispatch_date = dispatch_date;
    }
    case.deadline_info = deadline_info;
    case.notice_or_review_text = input.notice_text;
    case.rounds.push(record.clone());
    case.updated_at = now;

    save_case(state, &case)?;
    Ok((case, record))
}

fn fallback_mock_issues(case: &RebuttalCase, anchors: &[String]) -> Vec<ReviewIssueItem> {
    let first_anchor = anchors
        .first()
        .cloned()
        .unwrap_or_else(|| "[UNVERIFIED:待补充锚点]".to_string());
    match case.mode {
        RebuttalMode::PatentOa => vec![
            ReviewIssueItem {
                issue_id: "OA-1".to_string(),
                severity: "high".to_string(),
                category: "创造性（第22条第3款）三步法".to_string(),
                title: "对比文件结合启示与区别技术特征认定".to_string(),
                detail: "审查员认为独立权利要求的区别技术特征已被对比文件公开或属于本领域惯用手段，需逐项比对结构/参数边界并核查是否存在跨实施例拼凑。".to_string(),
                cited_anchors: vec![first_anchor.clone()],
                recommended_track: "pure_argument".to_string(),
            },
            ReviewIssueItem {
                issue_id: "OA-2".to_string(),
                severity: "medium".to_string(),
                category: "权利要求支持与清楚（第26条第4款）".to_string(),
                title: "上位概括与数值范围实施例支撑核查".to_string(),
                detail: "需核验权利要求中的关键工艺参数/部件连接关系是否在说明书具体实施例中具备原文支撑（严守第33条）。".to_string(),
                cited_anchors: vec![first_anchor],
                recommended_track: "amend_and_argue".to_string(),
            },
        ],
        RebuttalMode::PatentPreMock => vec![
            ReviewIssueItem {
                issue_id: "DEVIL-1".to_string(),
                severity: "high".to_string(),
                category: "新颖性/创造性前置雷区".to_string(),
                title: "上位特征与现有对比专利/DOI论文高度重合风险".to_string(),
                detail: "魔鬼审查员视角：当前独立权项缺乏定量阈值或协同约束，极易被对比文件结合公开论文直接覆盖，建议将从属权项核心限定前置或补充梯度实验对照。".to_string(),
                cited_anchors: vec![first_anchor.clone()],
                recommended_track: "amend_and_argue".to_string(),
            },
            ReviewIssueItem {
                issue_id: "DEVIL-2".to_string(),
                severity: "medium".to_string(),
                category: "公开充分（第26条第3款）".to_string(),
                title: "技术效果缺少量化实测对照支撑".to_string(),
                detail: "建议在交底书/说明书中挂载真实对照实验记录 [Exp:*] 以证明意料不到的技术效果。".to_string(),
                cited_anchors: vec![first_anchor],
                recommended_track: "add_experiment_and_revise".to_string(),
            },
        ],
        RebuttalMode::PaperRebuttal => vec![
            ReviewIssueItem {
                issue_id: "R1-C1".to_string(),
                severity: "high".to_string(),
                category: "Methodology & Novelty".to_string(),
                title: "与最新 SOTA 文献的机制差异及消融实验充分性".to_string(),
                detail: "Reviewer #1 质疑核心模块相较于既有 DOI 文献的本质增益，要求补充消融实验（Ablation Study）及置信区间/显著性检验。".to_string(),
                cited_anchors: vec![first_anchor.clone()],
                recommended_track: "add_experiment_and_revise".to_string(),
            },
            ReviewIssueItem {
                issue_id: "R2-C1".to_string(),
                severity: "medium".to_string(),
                category: "Reproducibility & Limitations".to_string(),
                title: "实验超参数披露与失效边界讨论".to_string(),
                detail: "Reviewer #2 要求明确训练集划分、硬件开销并在 Discussion 中补充方法局限性说明。".to_string(),
                cited_anchors: vec![first_anchor],
                recommended_track: "amend_and_argue".to_string(),
            },
        ],
    }
}

/// 生成高强度模拟审查 / 同行评审质疑清单
pub async fn run_mock_review(
    state: &AppState,
    case_id: &str,
    focus_query: &str,
) -> Result<(RebuttalCase, Vec<ReviewIssueItem>, String, bool)> {
    let mut case = load_case(state, case_id)?;
    let (evidence_ctx, _docs, anchors) = {
        let conn = state.db.conn();
        collect_evidence_context(&conn, &case, focus_query)?
    };

    let own_text = if !case.amended_claim_or_text.trim().is_empty() {
        &case.amended_claim_or_text
    } else {
        &case.original_claim_or_text
    };
    let sys_prompt = mock_review_system_prompt(case.mode);
    let user_payload = build_mock_review_user_payload(
        case.mode,
        &case.title,
        own_text,
        &case.notice_or_review_text,
        &evidence_ctx,
    );

    let router = {
        let cfg = state
            .config
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let conn = state.db.conn();
        build_llm_router_from_config(&cfg, &conn)?
    };

    let (issues, provider_used, offline_fallback) = match router
        .chat_with_failover(sys_prompt, &user_payload, true)
        .await
    {
        Ok((reply, provider)) => {
            let parsed = extract_json_block(&reply)
                .and_then(|v| v.get("issues").cloned())
                .and_then(|arr| serde_json::from_value::<Vec<ReviewIssueItem>>(arr).ok())
                .filter(|list| !list.is_empty());
            match parsed {
                Some(list) => (list, provider, false),
                None => (fallback_mock_issues(&case, &anchors), provider, true),
            }
        }
        Err(_) => (
            fallback_mock_issues(&case, &anchors),
            "offline_deterministic".to_string(),
            true,
        ),
    };

    case.latest_mock_issues = issues.clone();
    case.updated_at = now_iso();
    save_case(state, &case)?;

    Ok((case, issues, provider_used, offline_fallback))
}

fn fallback_draft_result(
    case: &RebuttalCase,
    anchors: &[String],
    preferred_track: &str,
) -> RebuttalDraftResult {
    let track = if !preferred_track.trim().is_empty() {
        preferred_track.trim().to_string()
    } else if !case.amended_claim_or_text.trim().is_empty()
        && case.amended_claim_or_text.trim() != case.original_claim_or_text.trim()
    {
        "amend_and_argue".to_string()
    } else {
        "pure_argument".to_string()
    };

    let anchor_str = if anchors.is_empty() {
        String::new()
    } else {
        anchors
            .iter()
            .take(3)
            .cloned()
            .collect::<Vec<_>>()
            .join(" ")
    };

    let issues: Vec<ReviewIssueItem> = if case.latest_mock_issues.is_empty() {
        fallback_mock_issues(case, anchors)
    } else {
        case.latest_mock_issues.clone()
    };

    let mut responses = Vec::new();
    let mut full_parts = Vec::new();
    for item in &issues {
        let bound = if item.cited_anchors.is_empty() {
            anchors.iter().take(2).cloned().collect::<Vec<_>>()
        } else {
            item.cited_anchors.clone()
        };
        let bound_inline = bound.join(" ");
        let resp_text = match case.mode {
            RebuttalMode::PatentOa | RebuttalMode::PatentPreMock => format!(
                "针对【{}】{}：依据《专利审查指南》三步法，对比文件未给出将区别特征结合并获得本申请技术效果的技术启示；且相关技术特征已在原申请文件 {} 中明确记载，符合专利法第33条规定。",
                item.issue_id, item.title, bound_inline
            ),
            RebuttalMode::PaperRebuttal => format!(
                "Response to {}: We thank the reviewer for this insightful comment regarding {}. We have clarified the methodological boundary and verified the quantitative results in {}.",
                item.issue_id, item.title, bound_inline
            ),
        };
        full_parts.push(resp_text.clone());
        responses.push(PointResponseItem {
            issue_id: item.issue_id.clone(),
            track: track.clone(),
            response_text: resp_text,
            bound_anchors: bound,
        });
    }

    let suggested_amended = if !case.amended_claim_or_text.trim().is_empty() {
        case.amended_claim_or_text.clone()
    } else {
        case.original_claim_or_text.clone()
    };

    let header = match case.mode {
        RebuttalMode::PatentOa => format!(
            "【意见陈述书草稿（{}）】\n策略轨道：{}\n引证核验锚点：{}\n\n",
            case.title, track, anchor_str
        ),
        RebuttalMode::PatentPreMock => format!(
            "【新专利提交前魔鬼预审加固方案（{}）】\n策略轨道：{}\n\n",
            case.title, track
        ),
        RebuttalMode::PaperRebuttal => format!(
            "Response to Reviewers ({})\nStrategy: {}\nVerified Anchors: {}\n\n",
            case.title, track, anchor_str
        ),
    };

    RebuttalDraftResult {
        strategy_track: track,
        strategy_rationale:
            "优先核查对比文件是否存在跨实施例强行拼凑特征；若原说明书/实验记录已具明确锚点支撑，则在不超第33条/不虚构数据前提下执行精准限缩或逐条实证回复。"
                .to_string(),
        point_by_point_responses: responses,
        full_draft_text: format!("{}{}", header, full_parts.join("\n\n")),
        suggested_amended_text: suggested_amended,
        provider_used: "offline_deterministic".to_string(),
        generated_offline_fallback: true,
    }
}

/// 生成双轨策略裁决与逐条绑定锚点的答复草稿
pub async fn run_draft(
    state: &AppState,
    case_id: &str,
    preferred_track: &str,
    user_instruction: &str,
) -> Result<(RebuttalCase, RebuttalDraftResult)> {
    let mut case = load_case(state, case_id)?;
    let (evidence_ctx, _docs, anchors) = {
        let conn = state.db.conn();
        collect_evidence_context(&conn, &case, user_instruction)?
    };

    let issues_json =
        serde_json::to_string_pretty(&case.latest_mock_issues).unwrap_or_else(|_| "[]".to_string());
    let sys_prompt = draft_rebuttal_system_prompt(case.mode);
    let user_payload = build_draft_user_payload(
        case.mode,
        &case.title,
        &case.original_claim_or_text,
        &case.amended_claim_or_text,
        &issues_json,
        &evidence_ctx,
        preferred_track,
    );

    let router = {
        let cfg = state
            .config
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let conn = state.db.conn();
        build_llm_router_from_config(&cfg, &conn)?
    };

    let draft_result = match router
        .chat_with_failover(sys_prompt, &user_payload, true)
        .await
    {
        Ok((reply, provider)) => {
            if let Some(val) = extract_json_block(&reply) {
                let track = val
                    .get("strategy_track")
                    .and_then(|v| v.as_str())
                    .unwrap_or("amend_and_argue")
                    .to_string();
                let rationale = val
                    .get("strategy_rationale")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let points = val
                    .get("point_by_point_responses")
                    .and_then(|v| serde_json::from_value::<Vec<PointResponseItem>>(v.clone()).ok())
                    .unwrap_or_default();
                let full_draft = val
                    .get("full_draft_text")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&reply)
                    .to_string();
                let amended = val
                    .get("suggested_amended_text")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&case.amended_claim_or_text)
                    .to_string();
                RebuttalDraftResult {
                    strategy_track: track,
                    strategy_rationale: rationale,
                    point_by_point_responses: points,
                    full_draft_text: full_draft,
                    suggested_amended_text: amended,
                    provider_used: provider,
                    generated_offline_fallback: false,
                }
            } else {
                RebuttalDraftResult {
                    strategy_track: if preferred_track.trim().is_empty() {
                        "amend_and_argue".to_string()
                    } else {
                        preferred_track.trim().to_string()
                    },
                    strategy_rationale: "AI 已生成完整抗辩论述文本".to_string(),
                    point_by_point_responses: Vec::new(),
                    full_draft_text: reply,
                    suggested_amended_text: case.amended_claim_or_text.clone(),
                    provider_used: provider,
                    generated_offline_fallback: false,
                }
            }
        }
        Err(_) => fallback_draft_result(&case, &anchors, preferred_track),
    };

    if case.amended_claim_or_text.trim().is_empty()
        && !draft_result.suggested_amended_text.trim().is_empty()
    {
        case.amended_claim_or_text = draft_result.suggested_amended_text.clone();
    }
    case.latest_draft = Some(draft_result.clone());
    case.updated_at = now_iso();
    save_case(state, &case)?;

    Ok((case, draft_result))
}

/// 调用 `EvidenceRebuttalService` 执行五维硬阻断门禁、第33条/论文一致性与跨实施例拼凑核验
pub fn run_verify(
    state: &AppState,
    case_id: &str,
    draft_text_override: Option<&str>,
    original_override: Option<&str>,
    amended_override: Option<&str>,
    require_citations: bool,
) -> Result<(RebuttalCase, RebuttalVerifyOutcome)> {
    let mut case = load_case(state, case_id)?;

    let draft_text = draft_text_override
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
        .or_else(|| {
            case.latest_draft
                .as_ref()
                .map(|d| d.full_draft_text.clone())
        })
        .unwrap_or_default();

    let original_text = original_override
        .map(ToString::to_string)
        .unwrap_or_else(|| case.original_claim_or_text.clone());
    let amended_text = amended_override
        .map(ToString::to_string)
        .unwrap_or_else(|| case.amended_claim_or_text.clone());

    let own_doc_id = case.own_doc_id.clone().unwrap_or_default();
    let mut related_doc_ids = Vec::new();
    if !own_doc_id.is_empty() {
        related_doc_ids.push(own_doc_id.clone());
    }
    for r in &case.reference_doc_ids {
        if !r.trim().is_empty() && !related_doc_ids.contains(r) {
            related_doc_ids.push(r.clone());
        }
    }

    let (gate_report, scope_report, stitching_alerts, priority_check) = {
        let conn = state.db.conn();
        let _ = VaultService::ensure_tables(&conn);

        let citations: Vec<VerifiedCitation> = EvidenceRebuttalService::verify_all_citations(
            &conn,
            &draft_text,
            case.own_doc_id.as_deref(),
        )?;

        // 按 doc_id 聚合引用锚点，检测对比文献是否存在跨实施例强行拼凑
        let mut by_doc: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for c in &citations {
            if !c.doc_id.is_empty() && c.doc_id != own_doc_id {
                by_doc
                    .entry(c.doc_id.clone())
                    .or_default()
                    .push(c.anchor_tag.clone());
            }
        }
        let mut stitching_alerts = Vec::new();
        for (doc_id, anchors) in by_doc {
            if let Some(alert) = EvidenceRebuttalService::detect_cross_embodiment_stitching(
                &conn, &doc_id, &anchors,
            )? {
                stitching_alerts.push(alert);
            }
        }

        let scope_report = match case.mode {
            RebuttalMode::PaperRebuttal => {
                EvidenceRebuttalService::check_paper_rebuttal_consistency(
                    &conn,
                    &own_doc_id,
                    &draft_text,
                )?
            }
            RebuttalMode::PatentOa | RebuttalMode::PatentPreMock => {
                EvidenceRebuttalService::check_patent_art33_scope(
                    &conn,
                    &own_doc_id,
                    &original_text,
                    &amended_text,
                    &draft_text,
                )?
            }
        };

        let (priority_check, _redacted) = EvidenceRebuttalService::check_disclosure_priority(
            &conn,
            &related_doc_ids,
            target_genre_for_mode(case.mode),
            &case.knowhow_markers,
            &draft_text,
        )?;

        let gate_report = EvidenceRebuttalService::evaluate_submission_gate(
            &conn,
            &GateEvaluationInput {
                draft_text: draft_text.clone(),
                default_doc_id: case.own_doc_id.clone(),
                require_citations,
                stitching_alerts: stitching_alerts.clone(),
                scope_report: Some(scope_report.clone()),
                priority_check: Some(priority_check.clone()),
            },
        )?;

        (gate_report, scope_report, stitching_alerts, priority_check)
    };

    case.original_claim_or_text = original_text;
    case.amended_claim_or_text = amended_text;
    case.latest_gate_report = Some(gate_report.clone());
    if let Some(last_round) = case.rounds.last_mut() {
        last_round.draft_text = Some(draft_text);
        last_round.gate_decision = Some(format!("{:?}", gate_report.decision));
        last_round.overall_score = Some(gate_report.overall_score);
    }
    case.updated_at = now_iso();
    save_case(state, &case)?;

    let outcome = RebuttalVerifyOutcome {
        case_id: case.case_id.clone(),
        mode: case.mode,
        gate_report,
        scope_report,
        stitching_alerts,
        priority_check,
    };

    Ok((case, outcome))
}

/// 调用 `ForgeExporter` 产出带 `<w:del>`/`<w:ins>` Word 修订留痕的 `.docx`（或指定格式）答复包
pub fn run_export(
    state: &AppState,
    case_id: &str,
    format_str: Option<&str>,
    acknowledge_priority_warning: bool,
) -> Result<CompiledDeliverable> {
    let case = load_case(state, case_id)?;

    let format = match format_str
        .unwrap_or("docx")
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "md" | "markdown" => DeliverableFormat::Md,
        "tex" | "latex" => DeliverableFormat::Tex,
        "pdf" => DeliverableFormat::Pdf,
        "xlsx" => DeliverableFormat::Xlsx,
        _ => DeliverableFormat::Docx,
    };

    let draft_body = case
        .latest_draft
        .as_ref()
        .map(|d| d.full_draft_text.clone())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| case.notice_or_review_text.clone());

    let mut sections = vec![(
        "抗辩策略与意见正文 / Rebuttal Statement".to_string(),
        draft_body,
    )];
    if !case.latest_mock_issues.is_empty() {
        let issue_lines: Vec<String> = case
            .latest_mock_issues
            .iter()
            .map(|it| {
                format!(
                    "- [{}][{}] {}: {} (Anchors: {})",
                    it.issue_id,
                    it.severity,
                    it.title,
                    it.detail,
                    it.cited_anchors.join(", ")
                )
            })
            .collect();
        sections.push((
            "审查/审稿质疑拆解表 / Review Issues Breakdown".to_string(),
            issue_lines.join("\n"),
        ));
    }

    let mut related_doc_ids = BTreeSet::new();
    if let Some(own) = &case.own_doc_id {
        if !own.trim().is_empty() {
            related_doc_ids.insert(own.clone());
        }
    }
    for r in &case.reference_doc_ids {
        if !r.trim().is_empty() {
            related_doc_ids.insert(r.clone());
        }
    }

    let conn = state.db.conn();
    let _ = VaultService::ensure_tables(&conn);

    let mut reference_docs = Vec::new();
    for doc_id in &case.reference_doc_ids {
        if let Some(doc) = VaultService::get_document(&conn, doc_id)? {
            reference_docs.push(doc);
        }
    }

    let orig_diff = if case.original_claim_or_text.trim().is_empty() {
        None
    } else {
        Some(case.original_claim_or_text.clone())
    };
    let amended_diff = if case.amended_claim_or_text.trim().is_empty() {
        None
    } else {
        Some(case.amended_claim_or_text.clone())
    };

    let req = CompileDeliverableRequest {
        title: case.title.clone(),
        genre: target_genre_for_mode(case.mode),
        format,
        sections,
        original_text_for_diff: orig_diff,
        amended_text_for_diff: amended_diff,
        related_doc_ids: related_doc_ids.into_iter().collect(),
        reference_docs,
        knowhow_markers: case.knowhow_markers.clone(),
        acknowledge_priority_warning,
    };

    ForgeExporter::compile_deliverable(&conn, &req)
}
