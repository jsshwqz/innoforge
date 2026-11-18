//! 卡片 ⑦：系统配置与知识金库卡 HTTP 路由处理器（`src/cards/settings_vault/routes.rs`）
//!
//! 覆盖第五章系统与金库 8 条正交 REST 接口：
//! 1. `GET /api/settings/health`
//! 2. `GET /api/settings/config`
//! 3. `POST /api/settings/config`
//! 4. `POST /api/settings/probe-provider`
//! 5. `POST /api/vault/import-pack`
//! 6. `GET /api/vault/docs`
//! 7. `PATCH /api/vault/docs`
//! 8. `DELETE /api/vault/docs`

use axum::{extract::State, http::StatusCode, Json};
use serde_json::json;

use crate::routes::AppState;

use super::service;
pub use super::service::{
    ProbeProviderReq, SettingsConfigUpdateReq, VaultDeleteDocQuery, VaultDocsQuery,
    VaultImportPackReq, VaultPatchDocReq,
};

/// `GET /api/settings/health`：首启四维环境健康快速自检（纯本机无阻塞毫秒级检查）
pub async fn api_settings_health(State(s): State<AppState>) -> Json<serde_json::Value> {
    Json(service::check_environment_health(&s))
}

/// `GET /api/settings/config`：返回脱敏后的系统与 9 服务商容灾配置快照
pub async fn api_settings_get_config(State(s): State<AppState>) -> Json<serde_json::Value> {
    Json(service::get_masked_config(&s))
}

/// `POST /api/settings/config`：更新并持久化系统配置与服务商容灾链
pub async fn api_settings_save_config(
    State(s): State<AppState>,
    Json(req): Json<SettingsConfigUpdateReq>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    service::save_config(&s, &req).map(Json).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "status": "error",
                "code": "invalid_config",
                "message": e.to_string()
            })),
        )
    })
}

/// `POST /api/settings/probe-provider`：3 秒短超时异步探测服务商连通性与模型列表
pub async fn api_settings_probe_provider(
    State(s): State<AppState>,
    Json(req): Json<ProbeProviderReq>,
) -> Json<serde_json::Value> {
    Json(service::probe_provider(&s, &req).await)
}

/// `POST /api/vault/import-pack`：免 Python 原生公开文献/专利数据包流式灌库（闭环 `#67`）
pub async fn api_vault_import_pack(
    State(s): State<AppState>,
    Json(req): Json<VaultImportPackReq>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    service::import_data_pack(&s, &req).map(Json).map_err(|e| {
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({
                "status": "error",
                "code": "pack_import_failed",
                "message": e.to_string()
            })),
        )
    })
}

/// `GET /api/vault/docs`：分页查询本机知识金库文献与实验/特征统计
pub async fn api_vault_list_docs(
    axum::extract::Query(q): axum::extract::Query<VaultDocsQuery>,
    State(s): State<AppState>,
) -> Json<serde_json::Value> {
    Json(service::list_vault_docs(&s, &q))
}

/// `PATCH /api/vault/docs`：更新文献 `asset_role`（如 `own_asset`）与 `filing_status`
pub async fn api_vault_patch_doc(
    State(s): State<AppState>,
    Json(req): Json<VaultPatchDocReq>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    service::patch_vault_doc(&s, &req).map(Json).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "status": "error",
                "code": "vault_patch_failed",
                "message": e.to_string()
            })),
        )
    })
}

/// `DELETE /api/vault/docs`：从本机知识金库删除指定文献及其关联切片与词典
pub async fn api_vault_delete_doc(
    axum::extract::Query(q): axum::extract::Query<VaultDeleteDocQuery>,
    State(s): State<AppState>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    service::delete_vault_doc(&s, &q.doc_id)
        .map(Json)
        .map_err(|e| {
            (
                StatusCode::NOT_FOUND,
                Json(json!({
                    "status": "error",
                    "code": "vault_delete_failed",
                    "message": e.to_string()
                })),
            )
        })
}
