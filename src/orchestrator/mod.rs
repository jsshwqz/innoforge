//! 编排器 / Orchestrator
//!
//! 替代原有 runner.rs 的线性编排，支持状态机驱动的迭代、跳转、分支。
//! 当前版本保持向后兼容：默认行为与原线性 Pipeline 一致，
//! 通过 OrchestratorCommand 可触发高级控制流。

pub mod command;
pub mod engine;

pub use command::OrchestratorCommand;

use crate::db::version::{Finding, IdeaBranch, IdeaVersion};
use crate::db::Database;
use crate::pipeline::context::{PipelineContext, ResearchState};
use crate::pipeline::state::PipelineStep;
use anyhow::Result;
use std::sync::Arc;

/// 保存版本快照到 idea_versions 表
pub fn save_version_snapshot(db: &Arc<Database>, ctx: &PipelineContext, step: PipelineStep) {
    let version_number = db.get_next_version_number(&ctx.idea_id).unwrap_or(1);

    let version = IdeaVersion {
        id: uuid::Uuid::new_v4().to_string(),
        idea_id: ctx.idea_id.clone(),
        version_number,
        context_json: serde_json::to_string(ctx).unwrap_or_default(),
        current_step: format!("{:?}", step),
        branch_id: ctx.branch_id.clone(),
        created_at: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
    };

    if let Err(e) = db.insert_idea_version(&version) {
        tracing::warn!("版本快照保存失败: {}", e);
    }
}

/// 记录失败路径到 findings 表
pub fn record_failure(db: &Arc<Database>, ctx: &PipelineContext, step: PipelineStep, error: &str) {
    let finding = Finding {
        id: uuid::Uuid::new_v4().to_string(),
        idea_id: ctx.idea_id.clone(),
        finding_type: "dead_end".to_string(),
        title: format!("步骤「{}」失败", step.description()),
        content: error.to_string(),
        source_step: format!("{:?}", step),
        branch_id: ctx.branch_id.clone(),
        created_at: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
    };

    if let Err(e) = db.insert_finding(&finding) {
        tracing::warn!("失败记录写入失败: {}", e);
    }
}

/// 创建新分支
pub fn create_branch(
    db: &Arc<Database>,
    ctx: &PipelineContext,
    branch_name: &str,
) -> Result<String> {
    let branch_id = uuid::Uuid::new_v4().to_string();
    let branch = IdeaBranch {
        id: branch_id.clone(),
        idea_id: ctx.idea_id.clone(),
        name: branch_name.to_string(),
        parent_branch_id: ctx.branch_id.clone(),
        parent_version_id: Some(ctx.parent_version_id.clone()),
        status: "active".to_string(),
        created_at: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
    };
    db.insert_idea_branch(&branch)?;
    tracing::info!("创建分支 '{}' (id: {})", branch_name, branch_id);
    Ok(branch_id)
}

// ── spawn_blocking 异步包装 / Async wrappers for blocking DB writes ──────────
//
// Issue #77: Orchestrator 的 run() 是 async fn，但 SQLite 写入是同步阻塞操作。
// 直接在 async worker 线程上调用会卡住整个 runtime。这些包装函数把同步 DB
// 调用移到 tokio 的 blocking thread pool，避免阻塞 async executor。
//
// Issue #77: The orchestrator's run() is async, but SQLite writes are synchronous.
// Calling them directly on the async worker stalls the runtime. These wrappers
// move the blocking DB calls onto tokio's blocking thread pool.

/// 异步保存管道快照（offload to spawn_blocking）
pub async fn save_pipeline_snapshot_blocking(
    db: Arc<Database>,
    idea_id: String,
    context_json: String,
    current_step: String,
) {
    let _ = tokio::task::spawn_blocking(move || {
        let _ = db.save_pipeline_snapshot(&idea_id, &context_json, &current_step);
    })
    .await;
}

/// 异步保存研发状态（offload to spawn_blocking）
pub async fn upsert_research_state_blocking(
    db: Arc<Database>,
    idea_id: String,
    state: ResearchState,
) {
    let _ = tokio::task::spawn_blocking(move || {
        let _ = db.upsert_research_state(&idea_id, &state);
    })
    .await;
}

/// 异步保存版本快照（offload to spawn_blocking）
pub async fn save_version_snapshot_blocking(
    db: Arc<Database>,
    ctx: PipelineContext,
    step: PipelineStep,
) {
    let _ = tokio::task::spawn_blocking(move || {
        save_version_snapshot(&db, &ctx, step);
    })
    .await;
}

/// 异步记录失败路径（offload to spawn_blocking）
pub async fn record_failure_blocking(
    db: Arc<Database>,
    ctx: PipelineContext,
    step: PipelineStep,
    error: String,
) {
    let _ = tokio::task::spawn_blocking(move || {
        record_failure(&db, &ctx, step, &error);
    })
    .await;
}

/// 异步删除管道快照（offload to spawn_blocking）
pub async fn delete_pipeline_snapshot_blocking(db: Arc<Database>, idea_id: String) {
    let _ = tokio::task::spawn_blocking(move || {
        let _ = db.delete_pipeline_snapshot(&idea_id);
    })
    .await;
}
