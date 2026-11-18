//! 卡片 ② 多模态想法共创与可分叉推演卡业务服务（`src/cards/idea_studio/service.rs`）
//!
//! 职责：
//! 1. 前舱：管理 `MultimodalSessionManager` 多轮图文共创会话，收敛《结构化技术方案基线卡》，
//!    并支持挂载我方前序成果（`own_asset`）做增量创新剥离；
//! 2. 后舱：管理 `DagRunner` 三档运行（QuickScout / RdExperiment / FullProtectionRebuttal）、
//!    卡点暂停恢复、任意步骤分叉对比路线 A/B，以及对照实验实测数据回填生成 `Exp:{id}` 铁证锚点。

use std::sync::OnceLock;

use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::cards::idea_studio::prompts;
use crate::core::multimodal::{
    CocreationTurnInput, CocreationTurnOutput, MultimodalSession, MultimodalSessionManager,
    SchemeBaselineCard,
};
use crate::core::runtime::llm_router::{builtin_nine_providers, DailyBudgetTracker};
use crate::core::runtime::{DagRunner, LlmProviderSpec, LlmRouter, TierDepthMode};
use crate::core::vault::models::{
    DagNode, DagRun, DagRunStatus, ExperimentRecord, ExperimentStatus, KnowledgeDoc,
    SseEnvelopeEvent,
};
use crate::core::vault::VaultService;
use crate::routes::{AppConfig, AppState};

static SESSION_MANAGER: OnceLock<MultimodalSessionManager> = OnceLock::new();
static DAG_RUNNER: OnceLock<DagRunner> = OnceLock::new();

pub fn session_manager() -> &'static MultimodalSessionManager {
    SESSION_MANAGER.get_or_init(MultimodalSessionManager::new)
}

pub fn dag_runner() -> &'static DagRunner {
    DAG_RUNNER.get_or_init(DagRunner::new)
}

/// 兼容 snake_case / PascalCase / 数字档位字符串解析
pub fn parse_tier_mode(raw: &str) -> TierDepthMode {
    match raw.trim() {
        "rd_experiment" | "RdExperiment" | "2" => TierDepthMode::RdExperiment,
        "full_protection_rebuttal" | "FullProtectionRebuttal" | "3" => {
            TierDepthMode::FullProtectionRebuttal
        }
        _ => TierDepthMode::QuickScout,
    }
}

/// 对照实验回填输入
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentSaveInput {
    pub exp_id: Option<String>,
    pub title: String,
    pub hypothesis: String,
    pub control_group_spec: String,
    pub test_group_spec: String,
    pub sandbox_output_json: Option<serde_json::Value>,
    pub measured_data_json: serde_json::Value,
    pub verified: bool,
}

fn ensure_vault_ready(state: &AppState) -> Result<()> {
    let conn = state.db.conn();
    VaultService::ensure_tables(&conn)
}

fn resolve_llm_endpoint(config: &AppConfig) -> Option<(String, String, String)> {
    let base_url = config.ai_base_url.trim();
    let api_key = config.ai_api_key.trim();
    let is_local = base_url.starts_with("http://127.0.0.1")
        || base_url.starts_with("http://localhost")
        || base_url.starts_with("http://[::1]");

    if !base_url.is_empty() && (!api_key.is_empty() || is_local) {
        let model = if !config.ai_model.trim().is_empty() {
            config.ai_model.trim().to_string()
        } else {
            "deepseek-chat".to_string()
        };
        return Some((base_url.to_string(), api_key.to_string(), model));
    }

    let fallbacks: [(&str, &str, &str); 6] = [
        (
            "https://api.deepseek.com/v1",
            config.ai_api_key_deepseek.trim(),
            "deepseek-chat",
        ),
        (
            "https://open.bigmodel.cn/api/paas/v4",
            config.ai_api_key_zhipu.trim(),
            "glm-4-plus",
        ),
        (
            "https://dashscope.aliyuncs.com/compatible-mode/v1",
            config.ai_api_key_qwen.trim(),
            "qwen-plus",
        ),
        (
            "https://api.openai.com/v1",
            config.ai_api_key_openai.trim(),
            "gpt-4o-mini",
        ),
        (
            "https://token.sensenova.cn/v1",
            config.ai_api_key_sensetime.trim(),
            "sensenova-6.8-flash",
        ),
        (
            "https://openrouter.ai/api/v1",
            config.ai_api_key_openrouter.trim(),
            "openai/gpt-4o-mini",
        ),
    ];

    for (url, key, model) in fallbacks {
        if !key.is_empty() {
            return Some((url.to_string(), key.to_string(), model.to_string()));
        }
    }

    None
}

fn build_llm_router(config: &AppConfig, state: &AppState) -> Option<LlmRouter> {
    let mut providers: Vec<LlmProviderSpec> = builtin_nine_providers();
    for p in &mut providers {
        let key = match p.id.as_str() {
            "deepseek" => config.ai_api_key_deepseek.trim(),
            "sensenova" => config.ai_api_key_sensetime.trim(),
            "zhipu" => config.ai_api_key_zhipu.trim(),
            "qwen" => config.ai_api_key_qwen.trim(),
            "openai" => config.ai_api_key_openai.trim(),
            "gemini" => config.ai_api_key_gemini.trim(),
            "anthropic" => config.ai_api_key_anthropic.trim(),
            "xiaomi" => config.ai_api_key_xiaomi.trim(),
            "openrouter" => config.ai_api_key_openrouter.trim(),
            _ => "",
        };
        if !key.is_empty() {
            p.api_key = key.to_string();
        } else if !config.ai_api_key.trim().is_empty() && p.id == "deepseek" {
            p.api_key = config.ai_api_key.trim().to_string();
        }
    }

    if !config.ai_base_url.trim().is_empty()
        && (!config.ai_api_key.trim().is_empty()
            || config.ai_base_url.contains("127.0.0.1")
            || config.ai_base_url.contains("localhost"))
    {
        providers.insert(
            0,
            LlmProviderSpec {
                id: "configured".to_string(),
                display_name: "Configured AI".to_string(),
                base_url: config.ai_base_url.trim().to_string(),
                default_model: if config.ai_model.trim().is_empty() {
                    "deepseek-chat".to_string()
                } else {
                    config.ai_model.trim().to_string()
                },
                expert_model: if config.ai_model_expert.trim().is_empty() {
                    "deepseek-reasoner".to_string()
                } else {
                    config.ai_model_expert.trim().to_string()
                },
                vision_model: None,
                supports_vision: false,
                max_context_tokens: 128_000,
                api_key: config.ai_api_key.trim().to_string(),
            },
        );
    }

    if !providers.iter().any(|p| p.is_usable()) {
        return None;
    }

    let tracker = {
        let conn = state.db.conn();
        DailyBudgetTracker::from_sqlite(&conn, 200).ok()
    };

    match tracker {
        Some(t) => LlmRouter::with_budget_tracker(providers, std::sync::Arc::new(t)).ok(),
        None => LlmRouter::new(providers, 200).ok(),
    }
}

/// 创建前舱多模态共创会话
pub async fn create_session(
    state: &AppState,
    title: Option<String>,
    mounted_own_doc_ids: Vec<String>,
) -> Result<MultimodalSession> {
    ensure_vault_ready(state)?;
    let mgr = session_manager();
    let sess = mgr.create_session(title).await;

    if !mounted_own_doc_ids.is_empty() {
        let input = CocreationTurnInput {
            user_text: "初始化挂载我方前序成果".to_string(),
            attachments: Vec::new(),
            mounted_own_doc_ids,
        };
        let _ = mgr
            .advance_cocreation_turn(&sess.session_id, input, None)
            .await;
        if let Some(updated) = mgr.get_session(&sess.session_id).await {
            return Ok(updated);
        }
    }

    Ok(sess)
}

/// 推进一轮前舱多模态共创，返回启发式追问与增量更新后的《结构化技术方案基线卡》
pub async fn advance_turn(
    state: &AppState,
    session_id: &str,
    input: CocreationTurnInput,
    lock_baseline: bool,
) -> Result<(CocreationTurnOutput, MultimodalSession)> {
    ensure_vault_ready(state)?;
    let endpoint = {
        let cfg = state.config.read().map(|g| g.clone()).unwrap_or_default();
        resolve_llm_endpoint(&cfg)
    };

    let mgr = session_manager();
    let mut output = mgr
        .advance_cocreation_turn(session_id, input, endpoint.as_ref())
        .await?;

    if lock_baseline {
        output.updated_baseline = mgr.lock_baseline(session_id).await?;
    }

    let session = mgr
        .get_session(session_id)
        .await
        .ok_or_else(|| anyhow!("共创会话 `{session_id}` 不存在"))?;

    Ok((output, session))
}

/// 加载挂载的我方前序资产摘要（供增量创新剥离与防自我公开提示）
fn load_mounted_docs(state: &AppState, doc_ids: &[String]) -> Vec<KnowledgeDoc> {
    if doc_ids.is_empty() {
        return Vec::new();
    }
    let conn = state.db.conn();
    let mut docs = Vec::new();
    for id in doc_ids {
        if let Ok(Some(doc)) = VaultService::get_document(&conn, id) {
            docs.push(doc);
        }
    }
    docs
}

/// 启动后舱可分叉推演流水线
pub async fn start_run(
    state: &AppState,
    session_id: Option<&str>,
    tier_raw: &str,
    scheme_baseline: Option<SchemeBaselineCard>,
    mut mounted_own_doc_ids: Vec<String>,
) -> Result<(DagRun, Vec<DagNode>)> {
    ensure_vault_ready(state)?;
    let tier = parse_tier_mode(tier_raw);

    let mut baseline = match scheme_baseline {
        Some(b) => b,
        None => {
            if let Some(sid) = session_id {
                let mgr = session_manager();
                if let Some(sess) = mgr.get_session(sid).await {
                    if !sess.baseline.locked {
                        let _ = mgr.lock_baseline(sid).await;
                    }
                    sess.baseline
                } else {
                    SchemeBaselineCard::default()
                }
            } else {
                SchemeBaselineCard::default()
            }
        }
    };

    for doc_id in &mounted_own_doc_ids {
        if !baseline.mounted_own_doc_ids.contains(doc_id) {
            baseline.mounted_own_doc_ids.push(doc_id.clone());
        }
    }
    if mounted_own_doc_ids.is_empty() {
        mounted_own_doc_ids = baseline.mounted_own_doc_ids.clone();
    }

    let baseline_json = serde_json::to_string(&baseline)?;
    let (run, nodes) = {
        let conn = state.db.conn();
        let run = dag_runner().start_run(
            &conn,
            "idea_studio",
            tier,
            &baseline_json,
            mounted_own_doc_ids,
        )?;
        let (_, nodes, _) = dag_runner().get_run_genealogy(&conn, &run.run_id)?;
        (run, nodes)
    };

    spawn_dag_execution(state.clone(), run.run_id.clone());
    Ok((run, nodes))
}

/// 从任意步骤分叉出对比路线 B，返回子运行、节点列表及父子分叉谱系
pub async fn fork_run(
    state: &AppState,
    parent_run_id: &str,
    fork_from_step: &str,
    override_baseline: Option<SchemeBaselineCard>,
    route_label: Option<String>,
) -> Result<(DagRun, Vec<DagNode>, Vec<DagRun>)> {
    ensure_vault_ready(state)?;
    let override_json = match override_baseline {
        Some(mut b) => {
            if let Some(label) = route_label {
                let trimmed = label.trim();
                if !trimmed.is_empty() && !b.title.contains(trimmed) {
                    b.title = format!("{} [{}]", b.title, trimmed);
                }
            }
            Some(serde_json::to_string(&b)?)
        }
        None => None,
    };

    let (child_run, child_nodes, sibling_forks) = {
        let conn = state.db.conn();
        let child = dag_runner().fork_run(
            &conn,
            parent_run_id,
            fork_from_step,
            override_json.as_deref(),
        )?;
        let (_, nodes, _) = dag_runner().get_run_genealogy(&conn, &child.run_id)?;
        let siblings = VaultService::list_child_forks(&conn, parent_run_id)?;
        (child, nodes, siblings)
    };

    spawn_dag_execution(state.clone(), child_run.run_id.clone());
    Ok((child_run, child_nodes, sibling_forks))
}

/// 提交人工卡点微调确认并恢复流水线执行
pub async fn resume_checkpoint(
    state: &AppState,
    run_id: &str,
    step_key: &str,
    confirmed_output: serde_json::Value,
) -> Result<(DagRun, Vec<DagNode>)> {
    ensure_vault_ready(state)?;
    let resolved_step = if step_key.trim().is_empty() {
        "experiment_design_checkpoint"
    } else {
        step_key.trim()
    };

    let (run, nodes) = {
        let conn = state.db.conn();
        let run =
            dag_runner().resume_from_checkpoint(&conn, run_id, resolved_step, confirmed_output)?;
        let (_, nodes, _) = dag_runner().get_run_genealogy(&conn, run_id)?;
        (run, nodes)
    };

    if run.status == DagRunStatus::Running {
        spawn_dag_execution(state.clone(), run.run_id.clone());
    }

    Ok((run, nodes))
}

/// 回填对照实验真实测量数据并生成 `Exp:{exp_id}` 铁证锚点
pub async fn record_experiment(
    state: &AppState,
    run_id: &str,
    input: ExperimentSaveInput,
) -> Result<(ExperimentRecord, Option<String>)> {
    ensure_vault_ready(state)?;
    if input.title.trim().is_empty() {
        bail!("实验标题不能为空 / Experiment title is required");
    }
    if input.measured_data_json.is_null() {
        bail!("实测数据不能为空 / measured_data_json is required");
    }
    if let Some(obj) = input.measured_data_json.as_object() {
        if obj.is_empty() {
            bail!("实测数据对象不能为空 / measured_data_json object cannot be empty");
        }
    }

    let exp_id = input
        .exp_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| format!("exp_{}", Uuid::new_v4().simple()));

    let status = if input.verified {
        ExperimentStatus::Verified
    } else {
        ExperimentStatus::RealDataFilled
    };

    let record = ExperimentRecord {
        exp_id: exp_id.clone(),
        run_id: Some(run_id.to_string()),
        title: input.title.trim().to_string(),
        hypothesis: input.hypothesis.trim().to_string(),
        control_group_spec: input.control_group_spec.trim().to_string(),
        test_group_spec: input.test_group_spec.trim().to_string(),
        sandbox_output_json: input
            .sandbox_output_json
            .as_ref()
            .and_then(|v| serde_json::to_string(v).ok()),
        measured_data_json: Some(serde_json::to_string(&input.measured_data_json)?),
        status,
    };

    let anchor = {
        let conn = state.db.conn();
        VaultService::save_and_verify_experiment(&conn, &record)?
    };

    dag_runner().emit_event(
        run_id,
        SseEnvelopeEvent::StageUpdate {
            stage: "experiment_measured".to_string(),
            percent: 100,
            elapsed_ms: 0,
            message: format!(
                "实测数据已入库并生成证据锚点 {}",
                anchor.as_deref().unwrap_or("Exp:pending")
            ),
        },
    );

    Ok((record, anchor))
}

/// 订阅运行事件流，并返回当前状态快照事件 + 实时广播接收器
pub fn subscribe_run_events(
    state: &AppState,
    run_id: &str,
) -> (Vec<SseEnvelopeEvent>, broadcast::Receiver<SseEnvelopeEvent>) {
    let rx = dag_runner().subscribe_events(run_id);
    let mut initial = Vec::new();

    if let Ok((run, nodes, _)) = {
        let conn = state.db.conn();
        dag_runner().get_run_genealogy(&conn, run_id)
    } {
        let total = nodes.len().max(1);
        let done = nodes.iter().filter(|n| n.status == "completed").count();
        let percent = ((done * 100) / total).min(100) as u8;
        let current_stage = nodes
            .iter()
            .find(|n| n.status != "completed")
            .map(|n| n.step_key.clone())
            .unwrap_or_else(|| "completed".to_string());

        initial.push(SseEnvelopeEvent::StageUpdate {
            stage: current_stage.clone(),
            percent,
            elapsed_ms: 0,
            message: format!(
                "Run {} status: {} ({}/{})",
                run.run_id,
                run.status.as_str(),
                done,
                total
            ),
        });

        if run.status == DagRunStatus::PausedCheckpoint {
            if let Some(paused_node) = nodes.iter().find(|n| n.status == "paused_checkpoint") {
                let candidates = paused_node
                    .checkpoint_payload_json
                    .as_deref()
                    .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
                    .unwrap_or_else(|| json!({}));
                initial.push(SseEnvelopeEvent::CheckpointRequired {
                    checkpoint_kind: paused_node.step_key.clone(),
                    candidates,
                });
            }
        } else if run.status == DagRunStatus::Completed {
            initial.push(SseEnvelopeEvent::Completed {
                artifact_id: run.run_id,
                summary: json!({ "completed_steps": done, "total_steps": total }),
            });
        }
    }

    (initial, rx)
}

fn spawn_dag_execution(state: AppState, run_id: String) {
    tokio::spawn(async move {
        let (run_opt, nodes) = {
            let conn = state.db.conn();
            match dag_runner().get_run_genealogy(&conn, &run_id) {
                Ok((r, n, _)) => (Some(r), n),
                Err(_) => (None, Vec::new()),
            }
        };

        let Some(run) = run_opt else {
            return;
        };
        if run.status != DagRunStatus::Running {
            return;
        }

        let tier = parse_tier_mode(&run.tier_mode);
        let baseline: SchemeBaselineCard =
            serde_json::from_str(&run.scheme_baseline_json).unwrap_or_default();
        let mounted_docs = load_mounted_docs(&state, &run.mounted_own_doc_ids);
        let router = {
            let cfg = state.config.read().map(|g| g.clone()).unwrap_or_default();
            build_llm_router(&cfg, &state)
        };

        for node in nodes {
            if node.status != "pending" {
                continue;
            }

            if node.step_key == "experiment_design_checkpoint" {
                let candidates = build_checkpoint_candidates(&baseline, &mounted_docs);
                let conn = state.db.conn();
                let _ =
                    dag_runner().pause_at_checkpoint(&conn, &run_id, &node.step_key, candidates);
                return;
            }

            let output = execute_single_step(
                router.as_ref(),
                tier,
                &node.step_key,
                &baseline,
                &mounted_docs,
            )
            .await;

            let conn = state.db.conn();
            if dag_runner()
                .complete_step(&conn, &run_id, &node.step_key, output)
                .is_err()
            {
                return;
            }
        }
    });
}

fn build_checkpoint_candidates(
    baseline: &SchemeBaselineCard,
    mounted_docs: &[KnowledgeDoc],
) -> serde_json::Value {
    let title = if baseline.title.trim().is_empty() {
        "候选技术方案".to_string()
    } else {
        baseline.title.clone()
    };
    let metrics = if baseline.experiment_metrics.is_empty() {
        vec![
            "核心性能指标提升率 (%)".to_string(),
            "功耗/成本对比".to_string(),
        ]
    } else {
        baseline.experiment_metrics.clone()
    };
    let own_titles: Vec<String> = mounted_docs
        .iter()
        .map(|d| {
            if d.title_zh.trim().is_empty() {
                d.title_en.clone()
            } else {
                d.title_zh.clone()
            }
        })
        .collect();

    json!({
        "title": format!("{title} 对照实验方案"),
        "hypothesis": format!("采用 {} 可在 {} 指标上显著优于常规对照基线", title, metrics.join("、")),
        "control_group_spec": "常规市售/前序基线方案（未引入本次增量特征）",
        "test_group_spec": format!("本方案原型（核心手段: {}）", baseline.technical_means.join("；")),
        "suggested_metrics": metrics,
        "mounted_own_assets": own_titles,
    })
}

async fn execute_single_step(
    router: Option<&LlmRouter>,
    tier: TierDepthMode,
    step_key: &str,
    baseline: &SchemeBaselineCard,
    mounted_docs: &[KnowledgeDoc],
) -> serde_json::Value {
    let sys_prompt = prompts::step_system_prompt(tier, step_key);
    let own_summary: Vec<String> = mounted_docs
        .iter()
        .map(|d| {
            let doc_title = if d.title_zh.trim().is_empty() {
                d.title_en.as_str()
            } else {
                d.title_zh.as_str()
            };
            format!("{doc_title} ({}, {:?})", d.doc_id, d.filing_status)
        })
        .collect();

    let user_payload = format!(
        "方案标题: {}\n核心痛点: {}\n技术手段: {}\n预期效果: {}\n实验指标: {}\n挂载我方前序成果: {}",
        baseline.title,
        baseline.core_problem,
        baseline.technical_means.join("; "),
        baseline.expected_effects.join("; "),
        baseline.experiment_metrics.join("; "),
        if own_summary.is_empty() {
            "无".to_string()
        } else {
            own_summary.join(" | ")
        }
    );

    if let Some(r) = router {
        let prefer_expert = matches!(
            tier,
            TierDepthMode::RdExperiment | TierDepthMode::FullProtectionRebuttal
        );
        if let Ok((reply, provider_id)) = r
            .chat_with_failover(sys_prompt, &user_payload, prefer_expert)
            .await
        {
            return json!({
                "step_key": step_key,
                "mode": "llm",
                "provider_id": provider_id,
                "summary": reply,
                "mounted_own_count": mounted_docs.len(),
            });
        }
    }

    // 确定性离线推演兜底（无外部 Key 时依然可完整闭环）
    let deterministic_summary = match step_key {
        "query_plan" => format!(
            "已生成四维检索规划：专利/论文/开源/标准检索词围绕「{}」与核心手段「{}」展开。",
            baseline.title,
            baseline.technical_means.join("、")
        ),
        "omni_search" => format!(
            "四维初筛完成：已比对挂载的 {} 项我方前序成果，建议重点剥离增量特征以规避自我公开风险。",
            mounted_docs.len()
        ),
        "quick_verdict" => format!(
            "快速探路初判：「{}」具备明确技术问题与工程手段，建议进入对照实验档验证核心指标（{}）。",
            baseline.title,
            baseline.experiment_metrics.join("、")
        ),
        "contradiction_analysis" => format!(
            "TRIZ 矛盾拆解：针对「{}」，已识别性能增益与工程实现复杂度的权衡点。",
            baseline.core_problem
        ),
        "rd_synthesis" => {
            "研发综合收敛：已整合对照实验设计，形成可执行研发备忘与指标验证清单。".to_string()
        }
        "claim_tree_draft" => format!(
            "权利要求树草案：独立权利要求聚焦增量特征「{}」，从属权利要求覆盖部件标号与参数范围。",
            baseline.technical_means.join("；")
        ),
        "devil_mock_rebuttal" => {
            "魔鬼审查员预演：已模拟提出创造性结合启示质疑与公开日冲突核查，建议补充实测数据锚点 Exp:{id}。".to_string()
        }
        "five_dim_gate" => {
            "五维门禁预检：请确认所有引证锚点与实测数据已入库，并遵循先专利申请日后论文/开源公开原则。".to_string()
        }
        _ => format!("步骤 {step_key} 已完成确定性推演。"),
    };

    json!({
        "step_key": step_key,
        "mode": "deterministic_offline",
        "summary": deterministic_summary,
        "mounted_own_count": mounted_docs.len(),
    })
}
