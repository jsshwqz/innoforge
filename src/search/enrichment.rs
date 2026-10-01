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
}
