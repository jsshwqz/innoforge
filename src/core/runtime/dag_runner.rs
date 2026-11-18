//! 可分叉、可卡点暂停恢复的任务状态机与统一 SSE 事件流（T-205）
//!
//! - [`TierDepthMode`]：三档运行深度（快速探路 / 研发与对照实验 / 全套保护与抗辩预演）
//! - [`DagRunner`]：管理活跃运行 `broadcast::Sender<SseEnvelopeEvent>` 通道，
//!   支持卡点暂停/恢复、普通步骤推进、任意步骤分叉（Fork）与谱系查询。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, bail, Result};
use chrono::Utc;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::core::vault::models::{DagNode, DagRun, DagRunStatus, SseEnvelopeEvent};
use crate::core::vault::repo::{
    get_dag_run, list_child_forks, list_dag_nodes, upsert_dag_node, upsert_dag_run,
};

const SSE_CHANNEL_CAPACITY: usize = 128;

/// 三档运行深度模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TierDepthMode {
    /// 1 档：快速探路档（3 步）
    #[default]
    QuickScout,
    /// 2 档：研发与对照实验档（5 步）
    RdExperiment,
    /// 3 档：全套保护与抗辩预演档（7 步）
    FullProtectionRebuttal,
}

impl TierDepthMode {
    /// 返回当前档位对应的有序步骤键列表
    pub fn steps(&self) -> Vec<&'static str> {
        match self {
            Self::QuickScout => vec!["query_plan", "omni_search", "quick_verdict"],
            Self::RdExperiment => vec![
                "query_plan",
                "omni_search",
                "contradiction_analysis",
                "experiment_design_checkpoint",
                "rd_synthesis",
            ],
            Self::FullProtectionRebuttal => vec![
                "query_plan",
                "omni_search",
                "contradiction_analysis",
                "experiment_design_checkpoint",
                "claim_tree_draft",
                "devil_mock_rebuttal",
                "five_dim_gate",
            ],
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::QuickScout => "quick_scout",
            Self::RdExperiment => "rd_experiment",
            Self::FullProtectionRebuttal => "full_protection_rebuttal",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s.trim() {
            "rd_experiment" | "2" => Self::RdExperiment,
            "full_protection_rebuttal" | "3" => Self::FullProtectionRebuttal,
            _ => Self::QuickScout,
        }
    }
}

/// 统一可分叉、可卡点暂停恢复、带标准 SSE 广播的任务状态机
#[derive(Debug, Clone, Default)]
pub struct DagRunner {
    channels: Arc<Mutex<HashMap<String, broadcast::Sender<SseEnvelopeEvent>>>>,
}

impl DagRunner {
    pub fn new() -> Self {
        Self {
            channels: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn get_or_create_sender(&self, run_id: &str) -> broadcast::Sender<SseEnvelopeEvent> {
        let mut guard = match self.channels.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        guard
            .entry(run_id.to_string())
            .or_insert_with(|| {
                let (tx, _) = broadcast::channel(SSE_CHANNEL_CAPACITY);
                tx
            })
            .clone()
    }

    /// 订阅指定运行的 SSE 事件流
    pub fn subscribe_events(&self, run_id: &str) -> broadcast::Receiver<SseEnvelopeEvent> {
        self.get_or_create_sender(run_id).subscribe()
    }

    /// 向指定运行的订阅者广播 SSE 事件
    pub fn emit_event(&self, run_id: &str, event: SseEnvelopeEvent) {
        let tx = self.get_or_create_sender(run_id);
        let _ = tx.send(event);
    }

    /// 启动新运行：持久化 `DagRun`（Running）并按档位初始化全部 `DagNode`（pending），推送首条 `StageUpdate`
    pub fn start_run(
        &self,
        conn: &Connection,
        card_kind: &str,
        tier: TierDepthMode,
        scheme_baseline_json: &str,
        mounted_own_doc_ids: Vec<String>,
    ) -> Result<DagRun> {
        let now = Utc::now().to_rfc3339();
        let run_id = format!("run_{}", Uuid::new_v4().simple());

        let run = DagRun {
            run_id: run_id.clone(),
            card_kind: card_kind.to_string(),
            parent_run_id: None,
            fork_from_step: None,
            tier_mode: tier.as_str().to_string(),
            scheme_baseline_json: scheme_baseline_json.to_string(),
            mounted_own_doc_ids,
            status: DagRunStatus::Running,
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        upsert_dag_run(conn, &run)?;

        let steps = tier.steps();
        for (idx, step_key) in steps.iter().enumerate() {
            let node = DagNode {
                node_id: format!("node_{}", Uuid::new_v4().simple()),
                run_id: run_id.clone(),
                step_key: (*step_key).to_string(),
                step_index: idx as i32,
                status: "pending".to_string(),
                input_json: "{}".to_string(),
                output_json: "{}".to_string(),
                checkpoint_payload_json: None,
                updated_at: now.clone(),
            };
            upsert_dag_node(conn, &node)?;
        }

        let first_stage = steps.first().copied().unwrap_or("init").to_string();
        self.emit_event(
            &run_id,
            SseEnvelopeEvent::StageUpdate {
                stage: first_stage,
                percent: 0,
                elapsed_ms: 0,
                message: format!("Started {} run ({})", card_kind, tier.as_str()),
            },
        );

        Ok(run)
    }

    /// 在指定步骤卡点暂停：将节点置为 `paused_checkpoint` 并保存候选项，将 `DagRun` 置为 `PausedCheckpoint`
    pub fn pause_at_checkpoint(
        &self,
        conn: &Connection,
        run_id: &str,
        step_key: &str,
        candidates: serde_json::Value,
    ) -> Result<DagRun> {
        let mut run =
            get_dag_run(conn, run_id)?.ok_or_else(|| anyhow!("dag_run not found: {run_id}"))?;
        let nodes = list_dag_nodes(conn, run_id)?;
        let mut target_node = nodes
            .into_iter()
            .find(|n| n.step_key == step_key)
            .ok_or_else(|| anyhow!("step_key '{step_key}' not found in run '{run_id}'"))?;

        let now = Utc::now().to_rfc3339();
        let candidates_json = serde_json::to_string(&candidates)?;

        target_node.status = "paused_checkpoint".to_string();
        target_node.checkpoint_payload_json = Some(candidates_json);
        target_node.updated_at = now.clone();
        upsert_dag_node(conn, &target_node)?;

        run.status = DagRunStatus::PausedCheckpoint;
        run.updated_at = now;
        upsert_dag_run(conn, &run)?;

        self.emit_event(
            run_id,
            SseEnvelopeEvent::CheckpointRequired {
                checkpoint_kind: step_key.to_string(),
                candidates,
            },
        );

        Ok(run)
    }

    /// 从卡点恢复：落库确认输出并将节点置为 `completed`，恢复运行或推进至 `Completed`
    pub fn resume_from_checkpoint(
        &self,
        conn: &Connection,
        run_id: &str,
        step_key: &str,
        confirmed_output: serde_json::Value,
    ) -> Result<DagRun> {
        self.finish_step_internal(
            conn,
            run_id,
            step_key,
            confirmed_output,
            "Resumed and completed checkpoint",
        )
    }

    /// 推进普通步骤完成并广播进度百分比
    pub fn complete_step(
        &self,
        conn: &Connection,
        run_id: &str,
        step_key: &str,
        output: serde_json::Value,
    ) -> Result<DagRun> {
        self.finish_step_internal(conn, run_id, step_key, output, "Completed step")
    }

    fn finish_step_internal(
        &self,
        conn: &Connection,
        run_id: &str,
        step_key: &str,
        output: serde_json::Value,
        action_label: &str,
    ) -> Result<DagRun> {
        let mut run =
            get_dag_run(conn, run_id)?.ok_or_else(|| anyhow!("dag_run not found: {run_id}"))?;
        let nodes = list_dag_nodes(conn, run_id)?;
        let total = nodes.len();

        let mut target_node = nodes
            .iter()
            .find(|n| n.step_key == step_key)
            .cloned()
            .ok_or_else(|| anyhow!("step_key '{step_key}' not found in run '{run_id}'"))?;

        let now = Utc::now().to_rfc3339();
        target_node.status = "completed".to_string();
        target_node.output_json = serde_json::to_string(&output)?;
        target_node.updated_at = now.clone();
        upsert_dag_node(conn, &target_node)?;

        let completed_count = nodes
            .iter()
            .filter(|n| n.step_key == step_key || n.status == "completed")
            .count();
        let all_done = total > 0 && completed_count == total;

        run.status = if all_done {
            DagRunStatus::Completed
        } else {
            DagRunStatus::Running
        };
        run.updated_at = now;
        upsert_dag_run(conn, &run)?;

        let percent = (completed_count * 100)
            .checked_div(total)
            .map(|p| p.min(100) as u8)
            .unwrap_or(100);

        self.emit_event(
            run_id,
            SseEnvelopeEvent::StageUpdate {
                stage: step_key.to_string(),
                percent,
                elapsed_ms: 0,
                message: format!("{action_label} '{step_key}' ({completed_count}/{total})"),
            },
        );

        if all_done {
            self.emit_event(
                run_id,
                SseEnvelopeEvent::Completed {
                    artifact_id: run.run_id.clone(),
                    summary: output,
                },
            );
        }

        Ok(run)
    }

    /// 从父任务的任意步骤 `fork_from_step` 分叉出对比路线：
    /// 自动复制 `fork_from_step` 之前已完成的节点快照，将 `fork_from_step` 及后续步骤重置为 `pending`
    pub fn fork_run(
        &self,
        conn: &Connection,
        parent_run_id: &str,
        fork_from_step: &str,
        override_baseline_json: Option<&str>,
    ) -> Result<DagRun> {
        let parent = get_dag_run(conn, parent_run_id)?
            .ok_or_else(|| anyhow!("parent dag_run not found: {parent_run_id}"))?;
        let parent_nodes = list_dag_nodes(conn, parent_run_id)?;

        let fork_index = parent_nodes
            .iter()
            .find(|n| n.step_key == fork_from_step)
            .map(|n| n.step_index)
            .ok_or_else(|| {
                anyhow!(
                    "fork_from_step '{fork_from_step}' not found in parent run '{parent_run_id}'"
                )
            })?;

        if parent_nodes.is_empty() {
            bail!("parent dag_run '{parent_run_id}' has no nodes");
        }

        let now = Utc::now().to_rfc3339();
        let child_run_id = format!("run_{}", Uuid::new_v4().simple());

        let child_run = DagRun {
            run_id: child_run_id.clone(),
            card_kind: parent.card_kind,
            parent_run_id: Some(parent.run_id.clone()),
            fork_from_step: Some(fork_from_step.to_string()),
            tier_mode: parent.tier_mode,
            scheme_baseline_json: override_baseline_json
                .map(|s| s.to_string())
                .unwrap_or(parent.scheme_baseline_json),
            mounted_own_doc_ids: parent.mounted_own_doc_ids,
            status: DagRunStatus::Running,
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        upsert_dag_run(conn, &child_run)?;

        let total = parent_nodes.len();
        let mut reused_completed = 0usize;

        for pnode in parent_nodes {
            let can_reuse = pnode.step_index < fork_index && pnode.status == "completed";
            let child_node = if can_reuse {
                reused_completed += 1;
                DagNode {
                    node_id: format!("node_{}", Uuid::new_v4().simple()),
                    run_id: child_run_id.clone(),
                    step_key: pnode.step_key,
                    step_index: pnode.step_index,
                    status: "completed".to_string(),
                    input_json: pnode.input_json,
                    output_json: pnode.output_json,
                    checkpoint_payload_json: pnode.checkpoint_payload_json,
                    updated_at: now.clone(),
                }
            } else {
                DagNode {
                    node_id: format!("node_{}", Uuid::new_v4().simple()),
                    run_id: child_run_id.clone(),
                    step_key: pnode.step_key,
                    step_index: pnode.step_index,
                    status: "pending".to_string(),
                    input_json: "{}".to_string(),
                    output_json: "{}".to_string(),
                    checkpoint_payload_json: None,
                    updated_at: now.clone(),
                }
            };
            upsert_dag_node(conn, &child_node)?;
        }

        let percent = (reused_completed * 100)
            .checked_div(total)
            .map(|p| p.min(100) as u8)
            .unwrap_or(0);

        self.emit_event(
            &child_run_id,
            SseEnvelopeEvent::StageUpdate {
                stage: fork_from_step.to_string(),
                percent,
                elapsed_ms: 0,
                message: format!(
                    "Forked from '{parent_run_id}' at '{fork_from_step}' (reused {reused_completed}/{total} steps)"
                ),
            },
        );

        Ok(child_run)
    }

    /// 查询当前运行、步骤节点列表及其子分叉列表
    pub fn get_run_genealogy(
        &self,
        conn: &Connection,
        root_or_child_id: &str,
    ) -> Result<(DagRun, Vec<DagNode>, Vec<DagRun>)> {
        let run = get_dag_run(conn, root_or_child_id)?
            .ok_or_else(|| anyhow!("dag_run not found: {root_or_child_id}"))?;
        let nodes = list_dag_nodes(conn, &run.run_id)?;
        let child_forks = list_child_forks(conn, &run.run_id)?;
        Ok((run, nodes, child_forks))
    }
}
