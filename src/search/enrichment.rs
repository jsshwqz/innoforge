//! 免费全文富化入口 / Free full-text enrichment entry（MB2）
//!
//! 本模块是 `api_enrich_patent_free`（`routes/patent.rs`）抓取+解析逻辑的**唯一出处**。
//! 路由侧和 pipeline 侧都调这里的 [`enrich_patent_free`]，不复制第二份 HTML 解析。
//!
//! ## 冷却表共用（MA6a 纪律）
//! 富化请求与 `google_patents_xhr` **同 Host（patents.google.com）、同反爬风险**，
//! 故必须共用 [`crate::search::breaker::global_table`] 判据——同表同函数，禁止旁路。
//! 命中冷却 → 直接降级为摘要档，返回 [`EnrichResult::CooldownSkipped`] 并如实说明。
//!
//! ## 设计取舍
//! - **不引入新 `SourceKind` 变体**：富化与 XHR 共用 `SourceKind::GooglePatentsXhr`，
//!   因为它们打的是同一个上游、同一套反爬规则，分两个变体只会让冷却表多一个键却
//!   互不感知，违背「源级冷却」语义。
//! - **不新增 crate 依赖**：`reqwest` 已在 `Cargo.toml`，本模块零新依赖。
//! - **富化失败不回滚**：专利入库是前提，富化是附带优化，失败只记日志不传播错误。

use crate::db::Database;
use crate::search::breaker::{cooldown_duration, global_table};
use crate::search::model::{FailKind, SourceKind};
use crate::types::patent::Patent;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

/// 免费富化尝试的结果。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum EnrichResult {
    /// 专利已有全文，无需富化。
    AlreadyEnriched,
    /// 成功抓取并解析了全文。
    Enriched {
        description_len: usize,
        claims_len: usize,
    },
    /// 冷却表中该源处于冷却期，未出网。
    CooldownSkipped { remaining_secs: u64 },
    /// 抓取或解析失败。
    Failed { reason: String },
    /// 专利不在数据库中。
    NotFound,
}

/// 富化状态的结构化报告（响应侧字段，沿 MA6b 纪律：只发结构化键）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrichmentStatus {
    /// 调用方传入的标识（source_id 或 patent_id）。
    pub patent_identifier: String,
    /// 专利号（从 DB 记录或 source_id 提取，可能为空）。
    pub patent_number: String,
    /// 是否已有全文（description 或 claims 非空）。
    pub has_full_text: bool,
    /// 富化结果原因（"enriched" / "already_enriched" / "cooldown_skipped: 120s" / "failed: ..." / "not_found"）。
    pub reason: String,
}

/// 默认富化上限：每次分析最多富化 top-N 篇专利。
pub const DEFAULT_ENRICH_TOP_N: usize = 5;

/// 从 Google Patents HTML 中提取指定 section（abstract/claims/description）。
///
/// 本函数原位于 `routes/patent.rs`，MB2 移入此模块作为**唯一出处**，
/// 路由侧和 pipeline 侧共用，不复制第二份。
pub fn extract_section(html: &str, section: &str) -> Option<String> {
    let marker = format!("itemprop=\"{}\"", section);
    let start = html.find(&marker)?;
    let section_html = &html[start..];
    let end = section_html
        .find("</section>")
        .unwrap_or(section_html.len().min(200_000));
    let content = &section_html[..end];
    let mut parts: Vec<String> = Vec::new();
    let mut pos = 0;
    let bytes = content.as_bytes();
    while pos < bytes.len() {
        if bytes[pos] == b'>' {
            pos += 1;
            let text_start = pos;
            while pos < bytes.len() && bytes[pos] != b'<' {
                pos += 1;
            }
            if pos > text_start {
                let text = &content[text_start..pos].trim();
                if !text.is_empty()
                    && text.len() > 1
                    && !text.starts_with("class=")
                    && !text.starts_with("itemprop=")
                    && !text.contains("data-")
                {
                    parts.push(text.to_string());
                }
            }
        } else {
            pos += 1;
        }
    }
    if parts.is_empty() {
        return None;
    }
    let result = parts.join("\n\n");
    if result.len() < 10 {
        return None;
    }
    Some(result)
}

/// 从 Google Patents HTML 中提取 IPC/CPC 分类号。
///
/// 本函数原位于 `routes/patent.rs`，MB2 移入此模块作为**唯一出处**。
pub fn extract_classifications(html: &str) -> Option<String> {
    let mut codes = Vec::new();
    let marker = "itemprop=\"Code\"";
    let mut search_start = 0;
    while let Some(pos) = html[search_start..].find(marker) {
        let abs_pos = search_start + pos;
        if let Some(content_pos) = html[abs_pos..].find("content=\"") {
            let val_start = abs_pos + content_pos + 9;
            if let Some(val_end) = html[val_start..].find('"') {
                let code = &html[val_start..val_start + val_end];
                let is_valid = !code.is_empty()
                    && code.len() >= 3
                    && code != "true"
                    && code != "false"
                    && code.chars().any(|c| c.is_ascii_alphabetic())
                    && code.chars().any(|c| c.is_ascii_digit());
                if is_valid && !codes.contains(&code.to_string()) {
                    codes.push(code.to_string());
                }
            }
        }
        search_start = abs_pos + marker.len();
        if codes.len() >= 20 {
            break;
        }
    }
    if codes.is_empty() {
        None
    } else {
        Some(codes.join(", "))
    }
}

fn has_full_text(p: &Patent) -> bool {
    p.description.len() > 50 && p.claims.len() > 50
}

fn cn_needs_refetch(p: &Patent) -> bool {
    let is_cn = p.country == "CN" || p.patent_number.starts_with("CN");
    is_cn
        && p.claims.len() > 50
        && !p
            .claims
            .chars()
            .any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c))
}

/// **可复用的免费富化入口**——路由侧和 pipeline 侧共用此函数。
///
/// 流程：查库 → 检查已有全文 → 检查冷却表 → 抓取 Google Patents HTML → 解析 → 保存 → 回写冷却表。
///
/// # 冷却表共用
/// 使用 `SourceKind::GooglePatentsXhr`（与 XHR 同源同 Host），通过
/// [`crate::search::breaker::global_table`] 查询/写入，**禁止旁路**。
pub async fn enrich_patent_free(db: &Database, patent_id: &str) -> EnrichResult {
    // 1. 查库
    let patent = match db.get_patent(patent_id) {
        Ok(Some(p)) => p,
        Ok(None) => return EnrichResult::NotFound,
        Err(e) => {
            tracing::warn!(
                "enrich_patent_free: DB lookup failed for {}: {}",
                patent_id,
                e
            );
            return EnrichResult::Failed {
                reason: format!("DB lookup failed: {e}"),
            };
        }
    };

    // 2. 检查已有全文
    if has_full_text(&patent) && !cn_needs_refetch(&patent) {
        return EnrichResult::AlreadyEnriched;
    }

    // 3. 检查冷却表（共用 MA6a breaker::global_table，同表同函数）
    {
        let table = global_table().lock().unwrap_or_else(|p| p.into_inner());
        if let Some(remaining) = table.remaining(SourceKind::GooglePatentsXhr, Instant::now()) {
            return EnrichResult::CooldownSkipped {
                remaining_secs: remaining.as_secs(),
            };
        }
    }

    // 4. 构造 URL 并抓取
    let pn = &patent.patent_number;
    let lang = if patent.country == "CN" || pn.starts_with("CN") {
        "zh"
    } else {
        "en"
    };
    let url = format!("https://patents.google.com/patent/{pn}/{lang}");
    tracing::info!("[ENRICH-FREE] Fetching {url}");

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .unwrap_or_default();

    let resp = match client
        .get(&url)
        .header(
            "User-Agent",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
        )
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("[ENRICH-FREE] Request failed for {pn}: {e}");
            return EnrichResult::Failed {
                reason: format!("请求失败: {e}"),
            };
        }
    };

    let status = resp.status();
    if !status.is_success() {
        // HTTP 错误：503/429 等反爬信号 → 冷却
        let fail_kind = if status.as_u16() == 429 || status.as_u16() == 503 {
            FailKind::Quota
        } else if status.as_u16() == 401 || status.as_u16() == 403 {
            FailKind::Auth
        } else {
            return EnrichResult::Failed {
                reason: format!("HTTP {status}"),
            };
        };

        // 写入冷却表
        let mut table = global_table().lock().unwrap_or_else(|p| p.into_inner());
        table.record(SourceKind::GooglePatentsXhr, fail_kind, Instant::now());

        let cooldown = cooldown_duration(SourceKind::GooglePatentsXhr, fail_kind)
            .map(|d| format!(" (cooldown {}s)", d.as_secs()))
            .unwrap_or_default();
        tracing::warn!("[ENRICH-FREE] HTTP {status} for {pn}{cooldown}");
        return EnrichResult::Failed {
            reason: format!("HTTP {status}{cooldown}"),
        };
    }

    let html = match resp.text().await {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!("[ENRICH-FREE] Body read failed for {pn}: {e}");
            return EnrichResult::Failed {
                reason: format!("读取失败: {e}"),
            };
        }
    };

    // 5. 解析 HTML
    let mut updated = patent.clone();

    if patent.abstract_text.is_empty() {
        if let Some(abs) = extract_section(&html, "abstract") {
            updated.abstract_text = abs;
        }
    }
    if patent.claims.is_empty() || patent.claims.len() < 50 {
        if let Some(claims) = extract_section(&html, "claims") {
            updated.claims = claims;
        }
    }
    if patent.description.is_empty() || patent.description.len() < 50 {
        if let Some(desc) = extract_section(&html, "description") {
            updated.description = desc;
        }
    }
    if patent.ipc_codes.is_empty() {
        if let Some(ipc) = extract_classifications(&html) {
            updated.ipc_codes = ipc;
        }
    }

    // 6. 保存
    let saved_id = match db.insert_patent(&updated) {
        Ok(id) => id,
        Err(e) => {
            tracing::warn!(
                "Failed to save enriched patent {}: {}",
                updated.patent_number,
                e
            );
            return EnrichResult::Failed {
                reason: format!("保存失败: {e}"),
            };
        }
    };

    // MB0: 顺手算 TF-IDF 向量（静默降级，不阻塞富化）
    crate::db::vector::try_compute_and_save_embedding(
        db,
        &saved_id,
        &format!("{} {}", updated.title, updated.abstract_text),
    );

    // MB3: 切片入库（单一写入口，与 MB0 同一挂点纪律）
    // 把全文切成 chunks 存入 patent_chunks 表，供 RAG 检索使用
    let chunks = crate::rag::build_chunks_from_patent(
        &saved_id,
        &updated.title,
        &updated.abstract_text,
        &updated.claims,
        &updated.description,
    );
    if !chunks.is_empty() {
        match db.save_patent_chunks(&saved_id, &chunks, "char-tfidf-v1") {
            Ok(n) => tracing::info!(
                "[MB3] Saved {} chunks for patent {}",
                n,
                updated.patent_number
            ),
            Err(e) => tracing::warn!(
                "[MB3] Failed to save chunks for patent {}: {}",
                updated.patent_number,
                e
            ),
        }
    }

    // 7. 成功 → 清零冷却表
    {
        let mut table = global_table().lock().unwrap_or_else(|p| p.into_inner());
        table.success(SourceKind::GooglePatentsXhr);
    }

    tracing::info!(
        "[ENRICH-FREE] Updated {} | abstract={} claims={} desc={}",
        updated.patent_number,
        updated.abstract_text.len(),
        updated.claims.len(),
        updated.description.len()
    );

    EnrichResult::Enriched {
        description_len: updated.description.len(),
        claims_len: updated.claims.len(),
    }
}

/// 从 `RankedMatch` 的 `source_id` 或 `source_url` 提取专利号。
///
/// `source_id` 格式：`patent_local_CN123456`（本地）或 `patent_online_0`（在线）。
/// `source_url` 格式：`https://patents.google.com/patent/CN123456`。
pub fn extract_patent_number(source_id: &str, source_url: &str) -> Option<String> {
    // 优先从 source_url 提取（更可靠）
    if let Some(pos) = source_url.find("/patent/") {
        let rest = &source_url[pos + 8..];
        let pn = rest.split('/').next().unwrap_or("");
        if !pn.is_empty() {
            return Some(pn.to_string());
        }
    }
    // 从 source_id 提取
    if let Some(pn) = source_id.strip_prefix("patent_local_") {
        if !pn.is_empty() {
            return Some(pn.to_string());
        }
    }
    // 兜底：source_id 本身可能是专利号
    if !source_id.is_empty() && !source_id.starts_with("patent_online_") {
        return Some(source_id.to_string());
    }
    None
}

/// 将 [`EnrichResult`] 转为 [`EnrichmentStatus`]（响应侧结构化字段）。
pub fn enrich_result_to_status(
    identifier: &str,
    patent_number: &str,
    result: &EnrichResult,
) -> EnrichmentStatus {
    match result {
        EnrichResult::AlreadyEnriched => EnrichmentStatus {
            patent_identifier: identifier.to_string(),
            patent_number: patent_number.to_string(),
            has_full_text: true,
            reason: "already_enriched".to_string(),
        },
        EnrichResult::Enriched {
            description_len,
            claims_len,
        } => EnrichmentStatus {
            patent_identifier: identifier.to_string(),
            patent_number: patent_number.to_string(),
            has_full_text: *description_len > 50 || *claims_len > 50,
            reason: "enriched".to_string(),
        },
        EnrichResult::CooldownSkipped { remaining_secs } => EnrichmentStatus {
            patent_identifier: identifier.to_string(),
            patent_number: patent_number.to_string(),
            has_full_text: false,
            reason: format!("cooldown_skipped: {remaining_secs}s"),
        },
        EnrichResult::Failed { reason } => EnrichmentStatus {
            patent_identifier: identifier.to_string(),
            patent_number: patent_number.to_string(),
            has_full_text: false,
            reason: format!("failed: {reason}"),
        },
        EnrichResult::NotFound => EnrichmentStatus {
            patent_identifier: identifier.to_string(),
            patent_number: patent_number.to_string(),
            has_full_text: false,
            reason: "not_found".to_string(),
        },
    }
}

// ─── MB2: 批量富化（分析前自动补全文）──────────────────────────────────

/// 批量富化单条结果中失败项的稳定原因码。
///
/// 禁止把中文文案塞进此枚举——前端按 `as_str()` 匹配结构化键，不猜文案。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EnrichFailReason {
    Cooldown,
    Blocked,
    Timeout,
    NotFound,
    ParseEmpty,
    Other,
}

impl EnrichFailReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Cooldown => "cooldown",
            Self::Blocked => "blocked",
            Self::Timeout => "timeout",
            Self::NotFound => "not_found",
            Self::ParseEmpty => "parse_empty",
            Self::Other => "other",
        }
    }
}

/// 批量富化中单条失败记录。
#[derive(Debug, Clone, serde::Serialize)]
pub struct EnrichFailEntry {
    pub patent_id: String,
    pub reason_code: EnrichFailReason,
    pub remaining_secs: Option<u64>,
}

/// 批量富化结构化出参。
///
/// 沿 MA6b 范式：空则整键省略（`failed` 为空时序列化端省略）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct BatchEnrichOutcome {
    pub requested: usize,
    pub enriched: usize,
    pub full_text_available_count: usize,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub failed: Vec<EnrichFailEntry>,
}

impl BatchEnrichOutcome {
    /// 序列化为 JSON Value，空 `failed` 键省略（MA6b 范式）。
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or_else(|_| serde_json::json!({}))
    }
}

/// 将单条 `EnrichResult` 转换为 `EnrichFailReason`（仅失败时）。
fn enrich_result_to_fail_reason(result: &EnrichResult) -> Option<(EnrichFailReason, Option<u64>)> {
    match result {
        EnrichResult::AlreadyEnriched | EnrichResult::Enriched { .. } => None,
        EnrichResult::CooldownSkipped { remaining_secs } => {
            Some((EnrichFailReason::Cooldown, Some(*remaining_secs)))
        }
        EnrichResult::Failed { reason } => {
            let r = reason.to_lowercase();
            let code = if r.contains("timeout") {
                EnrichFailReason::Timeout
            } else if r.contains("block") || r.contains("403") || r.contains("429") {
                EnrichFailReason::Blocked
            } else if r.contains("parse") || r.contains("empty") {
                EnrichFailReason::ParseEmpty
            } else {
                EnrichFailReason::Other
            };
            Some((code, None))
        }
        EnrichResult::NotFound => Some((EnrichFailReason::NotFound, None)),
    }
}

/// 批量富化：为分析前的 top-N 专利自动补全文。
///
/// - `db`：数据库连接
/// - `patent_ids`：待富化专利 ID 列表（已按相关性排序）
/// - `top_n`：最多富化前 N 条（默认 5）
///
/// 每条沿用 20s 超时（由 `enrich_patent_free` 内部控制）。
/// 冷却中 → 零出网，如实返回 `Cooldown` 原因码。
/// **禁止无界循环里逐条出网**（AGENTS.md 2.7）。
pub async fn enrich_batch(
    db: &crate::db::Database,
    patent_ids: &[String],
    top_n: usize,
) -> BatchEnrichOutcome {
    let ids: Vec<&String> = patent_ids.iter().take(top_n).collect();
    let requested = ids.len();
    let mut enriched = 0usize;
    let mut full_text_available_count = 0usize;
    let mut failed = Vec::new();

    for id in ids {
        let result = enrich_patent_free(db, id).await;
        match &result {
            EnrichResult::Enriched {
                description_len,
                claims_len,
            } => {
                enriched += 1;
                if *description_len > 50 && *claims_len > 50 {
                    full_text_available_count += 1;
                }
            }
            EnrichResult::AlreadyEnriched => {
                // 已有全文，计入 full_text_available_count
                if let Ok(Some(p)) = db.get_patent(id) {
                    if p.description.len() > 50 && p.claims.len() > 50 {
                        full_text_available_count += 1;
                    }
                }
            }
            _ => {
                if let Some((reason_code, remaining_secs)) = enrich_result_to_fail_reason(&result) {
                    failed.push(EnrichFailEntry {
                        patent_id: id.clone(),
                        reason_code,
                        remaining_secs,
                    });
                }
            }
        }
    }

    BatchEnrichOutcome {
        requested,
        enriched,
        full_text_available_count,
        failed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enrich_result_serializes_correctly() {
        let r = EnrichResult::Enriched {
            description_len: 5000,
            claims_len: 3000,
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("\"status\":\"enriched\""));
        assert!(json.contains("5000"));
        let r2: EnrichResult = serde_json::from_str(&json).unwrap();
        assert_eq!(r, r2);
    }

    #[test]
    fn enrich_result_cooldown_skipped_serializes() {
        let r = EnrichResult::CooldownSkipped {
            remaining_secs: 120,
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("\"status\":\"cooldown_skipped\""));
        assert!(json.contains("120"));
    }

    #[test]
    fn extract_section_finds_abstract() {
        let html = r#"<section itemprop="abstract"><div class="abstract">
<p>This invention relates to a novel method for energy conversion.</p>
</div></section>"#;
        let result = extract_section(html, "abstract");
        assert!(result.is_some());
        let text = result.unwrap();
        assert!(text.contains("energy conversion"));
    }

    #[test]
    fn extract_section_returns_none_for_missing_section() {
        let html = r#"<section itemprop="claims">Some claims text here.</section>"#;
        assert!(extract_section(html, "abstract").is_none());
    }

    #[test]
    fn extract_section_finds_claims() {
        let html = r#"<section itemprop="claims"><div class="claim">
<p>1. A method comprising the steps of receiving input data.</p>
<p>2. The method of claim 1 further comprising processing the data.</p>
</div></section>"#;
        let result = extract_section(html, "claims");
        assert!(result.is_some());
        let text = result.unwrap();
        assert!(text.contains("receiving input data"));
    }

    #[test]
    fn extract_classifications_finds_codes() {
        let html = r#"<span itemprop="Code" content="F02K7/06">F02K7/06</span>
<span itemprop="Code" content="H01M8/04">H01M8/04</span>"#;
        let result = extract_classifications(html);
        assert!(result.is_some());
        let codes = result.unwrap();
        assert!(codes.contains("F02K7/06"));
        assert!(codes.contains("H01M8/04"));
    }

    #[test]
    fn extract_classifications_returns_none_for_no_codes() {
        let html = r#"<div>No classifications here</div>"#;
        assert!(extract_classifications(html).is_none());
    }

    #[test]
    fn extract_patent_number_from_url() {
        let pn = extract_patent_number(
            "patent_online_0",
            "https://patents.google.com/patent/CN116401354A/en",
        );
        assert_eq!(pn, Some("CN116401354A".to_string()));
    }

    #[test]
    fn extract_patent_number_from_local_id() {
        let pn = extract_patent_number("patent_local_CN116401354A", "");
        assert_eq!(pn, Some("CN116401354A".to_string()));
    }

    #[test]
    fn extract_patent_number_from_bare_id() {
        let pn = extract_patent_number("US12345678B2", "");
        assert_eq!(pn, Some("US12345678B2".to_string()));
    }

    #[test]
    fn extract_patent_number_none_for_online_synthetic() {
        let pn = extract_patent_number("patent_online_3", "");
        assert_eq!(pn, None);
    }

    #[test]
    fn enrich_result_to_status_already_enriched() {
        let r = EnrichResult::AlreadyEnriched;
        let s = enrich_result_to_status("id1", "CN123", &r);
        assert!(s.has_full_text);
        assert_eq!(s.reason, "already_enriched");
        assert_eq!(s.patent_number, "CN123");
    }

    #[test]
    fn enrich_result_to_status_cooldown() {
        let r = EnrichResult::CooldownSkipped {
            remaining_secs: 120,
        };
        let s = enrich_result_to_status("id1", "CN123", &r);
        assert!(!s.has_full_text);
        assert_eq!(s.reason, "cooldown_skipped: 120s");
    }

    #[test]
    fn enrich_result_to_status_failed() {
        let r = EnrichResult::Failed {
            reason: "HTTP 503".to_string(),
        };
        let s = enrich_result_to_status("id1", "CN123", &r);
        assert!(!s.has_full_text);
        assert_eq!(s.reason, "failed: HTTP 503");
    }

    #[test]
    fn enrich_result_to_status_not_found() {
        let r = EnrichResult::NotFound;
        let s = enrich_result_to_status("id1", "", &r);
        assert!(!s.has_full_text);
        assert_eq!(s.reason, "not_found");
    }

    #[test]
    fn enrich_result_to_status_enriched_with_full_text() {
        let r = EnrichResult::Enriched {
            description_len: 5000,
            claims_len: 3000,
        };
        let s = enrich_result_to_status("id1", "CN123", &r);
        assert!(s.has_full_text);
        assert_eq!(s.reason, "enriched");
    }

    #[test]
    fn enrich_result_to_status_enriched_without_full_text() {
        let r = EnrichResult::Enriched {
            description_len: 10,
            claims_len: 20,
        };
        let s = enrich_result_to_status("id1", "CN123", &r);
        assert!(!s.has_full_text);
    }

    // ── MB2 测试 ──────────────────────────────────────────────

    #[test]
    fn mb2_extract_section_chinese_long_text_no_panic() {
        // 中文长文用例：证明 extract_section 字节扫描不会因中文越界 panic
        let html = r#"<meta itemprop="description" content="">
            <div class="description">
            本发明涉及一种基于深度学习的专利文本分析方法及系统。该方法包括以下步骤：
            首先获取专利文本数据，对所述文本数据进行预处理，包括分词、去停用词及词向量映射；
            然后构建专利文本的语义表示模型，利用注意力机制捕获上下文依赖关系；
            接着通过对比学习策略优化模型参数，使相似专利的语义表示在向量空间中相互靠近；
            最后基于训练好的模型对目标专利进行创新点识别和相似度检索。
            所述系统包括数据采集模块、预处理模块、语义建模模块、对比优化模块和检索输出模块。
            在一个优选实施例中，所述注意力机制采用多头自注意力结构，头数为8，隐藏层维度为512。
            在另一个实施例中，所述对比学习策略采用InfoNCE损失函数，温度参数设置为0.07。
            </div>"#;
        let result = extract_section(html, "description");
        assert!(result.is_some(), "Chinese long text should be extracted");
        let desc = result.unwrap();
        assert!(desc.contains("深度学习"), "Should contain Chinese content");
        assert!(desc.len() > 100, "Should extract substantial content");
    }

    #[test]
    fn mb2_enrich_fail_reason_as_str_stable() {
        // 原因码字符串稳定枚举锁定——前端按此匹配，禁止变更
        assert_eq!(EnrichFailReason::Cooldown.as_str(), "cooldown");
        assert_eq!(EnrichFailReason::Blocked.as_str(), "blocked");
        assert_eq!(EnrichFailReason::Timeout.as_str(), "timeout");
        assert_eq!(EnrichFailReason::NotFound.as_str(), "not_found");
        assert_eq!(EnrichFailReason::ParseEmpty.as_str(), "parse_empty");
        assert_eq!(EnrichFailReason::Other.as_str(), "other");
    }

    #[test]
    fn mb2_enrich_result_to_fail_reason_mapping() {
        // EnrichResult → EnrichFailReason 映射锁定
        let cooldown = EnrichResult::CooldownSkipped {
            remaining_secs: 120,
        };
        let (code, secs) = enrich_result_to_fail_reason(&cooldown).unwrap();
        assert_eq!(code, EnrichFailReason::Cooldown);
        assert_eq!(secs, Some(120));

        let not_found = EnrichResult::NotFound;
        let (code, _) = enrich_result_to_fail_reason(&not_found).unwrap();
        assert_eq!(code, EnrichFailReason::NotFound);

        let enriched = EnrichResult::Enriched {
            description_len: 5000,
            claims_len: 3000,
        };
        assert!(enrich_result_to_fail_reason(&enriched).is_none());

        let already = EnrichResult::AlreadyEnriched;
        assert!(enrich_result_to_fail_reason(&already).is_none());

        let timeout_fail = EnrichResult::Failed {
            reason: "timeout after 20s".to_string(),
        };
        let (code, _) = enrich_result_to_fail_reason(&timeout_fail).unwrap();
        assert_eq!(code, EnrichFailReason::Timeout);

        let blocked_fail = EnrichResult::Failed {
            reason: "HTTP 403 Forbidden".to_string(),
        };
        let (code, _) = enrich_result_to_fail_reason(&blocked_fail).unwrap();
        assert_eq!(code, EnrichFailReason::Blocked);
    }

    #[test]
    fn mb2_batch_enrich_outcome_serialization_skips_empty_failed() {
        // MA6b 范式：空 failed 键省略
        let outcome = BatchEnrichOutcome {
            requested: 5,
            enriched: 3,
            full_text_available_count: 2,
            failed: vec![],
        };
        let json = outcome.to_json();
        assert!(
            json.get("failed").is_none(),
            "Empty failed key should be omitted"
        );
        assert_eq!(json["requested"], 5);
        assert_eq!(json["enriched"], 3);
        assert_eq!(json["full_text_available_count"], 2);
    }

    #[test]
    fn mb2_batch_enrich_outcome_serialization_with_failures() {
        let outcome = BatchEnrichOutcome {
            requested: 3,
            enriched: 1,
            full_text_available_count: 1,
            failed: vec![
                EnrichFailEntry {
                    patent_id: "p1".to_string(),
                    reason_code: EnrichFailReason::Cooldown,
                    remaining_secs: Some(120),
                },
                EnrichFailEntry {
                    patent_id: "p2".to_string(),
                    reason_code: EnrichFailReason::Blocked,
                    remaining_secs: None,
                },
            ],
        };
        let json = outcome.to_json();
        assert!(
            json.get("failed").is_some(),
            "Non-empty failed key should be present"
        );
        let failed = json["failed"].as_array().unwrap();
        assert_eq!(failed.len(), 2);
        assert_eq!(failed[0]["reason_code"], "cooldown");
        assert_eq!(failed[0]["remaining_secs"], 120);
        assert_eq!(failed[1]["reason_code"], "blocked");
        assert!(failed[1].get("remaining_secs").is_none() || failed[1]["remaining_secs"].is_null());
    }

    #[test]
    fn mb2_enrich_fail_reason_serde_snake_case() {
        // serde rename_all = "snake_case" 锁定
        let json = serde_json::to_string(&EnrichFailReason::ParseEmpty).unwrap();
        assert_eq!(json, "\"parse_empty\"");
        let json = serde_json::to_string(&EnrichFailReason::NotFound).unwrap();
        assert_eq!(json, "\"not_found\"");
    }
}
