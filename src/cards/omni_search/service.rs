//! 卡片 ① 全域情报检索业务编排服务（`src/cards/omni_search/service.rs`）
//!
//! 整合本地 SQLite 金库（`VaultService`）与四维并发检索内核（`OmniSearchService`），
//! 提供四维检索、AI 可选检索式改写、一键打标入库、混合段落召回、文献结构化精读与多格式导出。

use std::collections::HashSet;
use std::sync::Arc;

use anyhow::Result;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::core::omni_search::{
    ArxivAdapter, CrossrefAdapter, LocalSqlitePatentAdapter, OmniSearchService, OmniSourceAdapter,
    OpenAlexAdapter, OpenSourceCodeAdapter, QueryPlan, QueryPlanner, SemanticScholarAdapter,
    SerpApiPatentAdapter, SourceCallDiagnostic, TechStandardAdapter,
};
use crate::core::runtime::forge_export::{CompileDeliverableRequest, ForgeExporter};
use crate::core::vault::models::{
    AssetRole, DeliverableFormat, DeliverableGenre, DocChunk, DocKind, FigureNumeral,
    HybridEvidenceBundle, KnowledgeDoc, OmniSearchHit, SearchRealm, SearchSceneMode, SectionKind,
};
use crate::core::vault::VaultService;
use crate::routes::AppState;

use super::prompts::{
    build_query_rewrite_user_prompt, format_deterministic_match_reason,
    OMNI_QUERY_REWRITE_SYSTEM_PROMPT,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OmniSearchQueryResult {
    pub plan: QueryPlan,
    pub hits: Vec<OmniSearchHit>,
    pub diagnostics: Vec<SourceCallDiagnostic>,
    pub total_before_dedup: usize,
    pub saved_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocViewerPayload {
    pub doc: KnowledgeDoc,
    pub doc_chunks: Vec<DocChunk>,
    pub figure_numerals: Vec<FigureNumeral>,
}

#[derive(Debug, Clone)]
pub struct ExportedFile {
    pub filename: String,
    pub content_type: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Deserialize)]
struct AiRewriteJson {
    #[serde(default)]
    keywords: Vec<String>,
    #[serde(default)]
    expanded_terms: Vec<String>,
    #[serde(default)]
    patent_boolean_query: String,
    #[serde(default)]
    scholar_query: String,
    #[serde(default)]
    code_query: String,
    #[serde(default)]
    standard_query: String,
}

pub fn realm_for_doc_kind(kind: DocKind) -> SearchRealm {
    match kind {
        DocKind::Patent => SearchRealm::Patent,
        DocKind::JournalPaper | DocKind::ConfPaper | DocKind::Preprint => SearchRealm::Scholar,
        DocKind::CodeRepo => SearchRealm::CodeRepo,
        DocKind::Standard | DocKind::InternalScheme => SearchRealm::Standard,
    }
}

/// 构建四维查询规划（支持可选 AI 改写，超时或未配置时静默回退确定性规划）
pub async fn build_query_plan(
    state: &AppState,
    raw_query: &str,
    enable_ai_rewrite: bool,
) -> QueryPlan {
    if !enable_ai_rewrite {
        return QueryPlanner::plan_deterministic(raw_query);
    }

    let (ready, client) = {
        let cfg = state.config.read().unwrap_or_else(|e| e.into_inner());
        (cfg.ai_fast_channel_ready(), cfg.ai_client_fast())
    };
    if !ready {
        return QueryPlanner::plan_deterministic(raw_query);
    }

    QueryPlanner::plan_with_optional_rewrite(
        raw_query,
        Some(move |base_plan: QueryPlan| async move {
            let user_prompt = build_query_rewrite_user_prompt(
                &base_plan.raw_query,
                &base_plan.patent_boolean_query,
            );
            let reply = client
                .chat_with_system(OMNI_QUERY_REWRITE_SYSTEM_PROMPT, &user_prompt, 0.2)
                .await?;
            let json_str = extract_json_object(&reply).unwrap_or_else(|| reply.trim());
            let parsed: AiRewriteJson = serde_json::from_str(json_str)?;

            Ok(QueryPlan {
                raw_query: base_plan.raw_query,
                keywords: if parsed.keywords.is_empty() {
                    base_plan.keywords
                } else {
                    parsed.keywords
                },
                expanded_terms: if parsed.expanded_terms.is_empty() {
                    base_plan.expanded_terms
                } else {
                    parsed.expanded_terms
                },
                patent_boolean_query: if parsed.patent_boolean_query.trim().is_empty() {
                    base_plan.patent_boolean_query
                } else {
                    parsed.patent_boolean_query
                },
                scholar_query: if parsed.scholar_query.trim().is_empty() {
                    base_plan.scholar_query
                } else {
                    parsed.scholar_query
                },
                code_query: if parsed.code_query.trim().is_empty() {
                    base_plan.code_query
                } else {
                    parsed.code_query
                },
                standard_query: if parsed.standard_query.trim().is_empty() {
                    base_plan.standard_query
                } else {
                    parsed.standard_query
                },
                used_ai_rewrite: true,
            })
        }),
    )
    .await
}

fn extract_json_object(raw: &str) -> Option<&str> {
    let start = raw.find('{')?;
    let end = raw.rfind('}')?;
    if end >= start {
        raw.get(start..=end)
    } else {
        None
    }
}

/// 检索本地 SQLite 金库 `knowledge_docs`
fn search_local_vault_docs(
    conn: &Connection,
    plan: &QueryPlan,
    realms: &[SearchRealm],
    asset_role: Option<AssetRole>,
    limit: usize,
) -> Result<Vec<OmniSearchHit>> {
    let docs = VaultService::list_documents(conn, asset_role, None, 200, 0)?;
    let query_lower = plan.raw_query.to_lowercase();
    let kw_lowers: Vec<String> = plan
        .keywords
        .iter()
        .map(|k| k.to_lowercase())
        .filter(|k| !k.is_empty())
        .collect();

    let mut hits = Vec::new();
    for doc in docs {
        let realm = realm_for_doc_kind(doc.doc_kind);
        if !realms.is_empty() && !realms.contains(&realm) {
            continue;
        }

        let haystack = format!(
            "{} {} {} {} {}",
            doc.external_id,
            doc.title_zh,
            doc.title_en,
            doc.abstract_text,
            doc.authors_or_assignees
        )
        .to_lowercase();

        let mut matched_terms = Vec::new();
        if !query_lower.is_empty() && haystack.contains(&query_lower) {
            matched_terms.push(plan.raw_query.clone());
        }
        for kw in &kw_lowers {
            if haystack.contains(kw) && !matched_terms.iter().any(|t| t.eq_ignore_ascii_case(kw)) {
                matched_terms.push(kw.clone());
            }
        }

        if matched_terms.is_empty() && !query_lower.is_empty() {
            continue;
        }

        let score = 0.85 + (matched_terms.len() as f64) * 0.05;
        let match_reason =
            format_deterministic_match_reason("local_vault", &matched_terms, doc.doi_verified);
        let source_url = if doc.external_id.starts_with("10.") {
            format!("https://doi.org/{}", doc.external_id)
        } else {
            format!("/doc-viewer?id={}", doc.doc_id)
        };

        hits.push(OmniSearchHit {
            doc,
            realm,
            source_provider: "local_vault".to_string(),
            relevance_score: score,
            match_reason,
            source_url,
        });

        if hits.len() >= limit {
            break;
        }
    }

    Ok(hits)
}

/// 组装四维全域检索适配器链
fn build_omni_adapters(state: &AppState) -> Vec<Arc<dyn OmniSourceAdapter>> {
    let serpapi_key = {
        let cfg = state.config.read().unwrap_or_else(|e| e.into_inner());
        cfg.next_serpapi_key()
    };

    let mut adapters: Vec<Arc<dyn OmniSourceAdapter>> = Vec::new();
    adapters.push(Arc::new(LocalSqlitePatentAdapter::new(&state.db_path)));

    if let Some(key) = serpapi_key {
        if let Ok(serp) = SerpApiPatentAdapter::new(Some(key)) {
            adapters.push(Arc::new(serp));
        }
    }

    if let Ok(oa) = OpenAlexAdapter::new() {
        adapters.push(Arc::new(oa));
    }
    if let Ok(cr) = CrossrefAdapter::new() {
        adapters.push(Arc::new(cr));
    }
    if let Ok(ax) = ArxivAdapter::new() {
        adapters.push(Arc::new(ax));
    }
    if let Ok(s2) = SemanticScholarAdapter::new() {
        adapters.push(Arc::new(s2));
    }
    if let Ok(gh) = OpenSourceCodeAdapter::new() {
        adapters.push(Arc::new(gh));
    }
    if let Ok(std_adapter) = TechStandardAdapter::new() {
        adapters.push(Arc::new(std_adapter));
    }

    adapters
}

/// 执行卡片 ① 四维全域检索（本地金库 + 在线四维并发检索 + 可选一键入库）
#[allow(clippy::too_many_arguments)]
pub async fn execute_omni_search(
    state: &AppState,
    query: &str,
    realms: &[SearchRealm],
    asset_role: Option<AssetRole>,
    enable_ai_rewrite: bool,
    save_to_vault: bool,
    max_per_source: usize,
) -> Result<OmniSearchQueryResult> {
    let plan = build_query_plan(state, query, enable_ai_rewrite).await;

    let local_hits = {
        let conn = state.db.conn();
        let _ = VaultService::ensure_tables(&conn);
        search_local_vault_docs(&conn, &plan, realms, asset_role, max_per_source.max(20))?
    };

    let adapters = build_omni_adapters(state);
    let omni_service = OmniSearchService::new(adapters);
    let online_outcome = omni_service
        .search_across_realms(&plan, realms, max_per_source.clamp(1, 30))
        .await;

    let mut seen_keys: HashSet<String> = HashSet::new();
    let mut merged_hits: Vec<OmniSearchHit> = Vec::new();

    for hit in local_hits.into_iter().chain(online_outcome.hits) {
        let dedup_key = if !hit.doc.external_id.trim().is_empty() {
            format!(
                "{}:{}",
                hit.doc.doc_kind.as_str(),
                hit.doc.external_id.trim().to_lowercase()
            )
        } else {
            hit.doc.doc_id.clone()
        };
        if seen_keys.insert(dedup_key) {
            merged_hits.push(hit);
        }
    }

    let mut saved_count = 0usize;
    if save_to_vault && !merged_hits.is_empty() {
        let target_role = asset_role.unwrap_or(AssetRole::GeneralRef);
        let conn = state.db.conn();
        let _ = VaultService::ensure_tables(&conn);
        for hit in &mut merged_hits {
            hit.doc.asset_role = target_role;
            if VaultService::upsert_document_with_chunks(&conn, &hit.doc).is_ok() {
                saved_count += 1;
            }
        }
    }

    Ok(OmniSearchQueryResult {
        plan,
        hits: merged_hits,
        diagnostics: online_outcome.diagnostics,
        total_before_dedup: online_outcome.total_before_dedup,
        saved_count,
    })
}

/// 将单篇命中结果一键打标入库（如 `[我方已有成果]` `AssetRole::OwnAsset`）
pub fn save_single_doc_to_vault(
    state: &AppState,
    mut doc: KnowledgeDoc,
    role: AssetRole,
) -> Result<KnowledgeDoc> {
    doc.asset_role = role;
    let conn = state.db.conn();
    VaultService::ensure_tables(&conn)?;
    VaultService::upsert_document_with_chunks(&conn, &doc)?;
    Ok(doc)
}

/// 执行 RRF 混合段落召回（带父权项链与附图部件词典）
pub fn search_hybrid_chunks(
    state: &AppState,
    query: &str,
    mode: SearchSceneMode,
    doc_ids: Option<&[String]>,
    role_filter: Option<AssetRole>,
    top_k: usize,
    char_budget: usize,
) -> Result<HybridEvidenceBundle> {
    let conn = state.db.conn();
    VaultService::ensure_tables(&conn)?;
    VaultService::hybrid_search_chunks(
        &conn,
        query,
        mode,
        doc_ids,
        role_filter,
        None,
        top_k.clamp(1, 50),
        char_budget.clamp(500, 32000),
    )
}

/// 读取完整文献详情（`KnowledgeDoc` + `doc_chunks` 权项树/段落 + `figure_numerals` 附图部件词典）
pub fn get_doc_with_structure(state: &AppState, doc_id: &str) -> Result<Option<DocViewerPayload>> {
    let conn = state.db.conn();
    VaultService::ensure_tables(&conn)?;
    let Some(doc) = VaultService::get_document(&conn, doc_id)? else {
        return Ok(None);
    };

    let mut stmt = conn.prepare(
        "SELECT chunk_id, doc_id, section_kind, section_tag, claim_no, parent_claim_no,
                para_start, para_end, embodiment_group, raw_content, fts_tokens, char_count
         FROM doc_chunks
         WHERE doc_id = ?1
         ORDER BY COALESCE(claim_no, 999999) ASC, COALESCE(para_start, 999999) ASC, chunk_id ASC",
    )?;
    let rows = stmt.query_map(params![doc_id], |row| {
        let kind_str: String = row.get(2)?;
        Ok(DocChunk {
            chunk_id: row.get(0)?,
            doc_id: row.get(1)?,
            section_kind: SectionKind::parse(&kind_str),
            section_tag: row.get(3)?,
            claim_no: row.get(4)?,
            parent_claim_no: row.get(5)?,
            para_start: row.get(6)?,
            para_end: row.get(7)?,
            embodiment_group: row.get(8)?,
            raw_content: row.get(9)?,
            fts_tokens: row.get(10)?,
            embedding_blob: None,
            char_count: row.get::<_, i64>(11)?.max(0) as usize,
        })
    })?;

    let mut doc_chunks = Vec::new();
    for r in rows {
        doc_chunks.push(r?);
    }

    let figure_numerals = VaultService::get_figure_numerals(&conn, doc_id)?;

    Ok(Some(DocViewerPayload {
        doc,
        doc_chunks,
        figure_numerals,
    }))
}

fn escape_csv_cell(val: &str) -> String {
    let needs_quote =
        val.contains(',') || val.contains('"') || val.contains('\n') || val.contains('\r');
    if needs_quote {
        format!("\"{}\"", val.replace('"', "\"\""))
    } else {
        val.to_string()
    }
}

/// 导出四维全域检索结果（支持 `csv` / `xlsx` / `pdf`）
pub async fn export_omni_hits(
    state: &AppState,
    query: &str,
    format: &str,
    realms: &[SearchRealm],
    asset_role: Option<AssetRole>,
    provided_hits: &[OmniSearchHit],
) -> Result<ExportedFile> {
    let hits: Vec<OmniSearchHit> = if !provided_hits.is_empty() {
        provided_hits.to_vec()
    } else {
        let outcome =
            execute_omni_search(state, query, realms, asset_role, false, false, 15).await?;
        outcome.hits
    };

    let ts = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let fmt_lower = format.trim().to_ascii_lowercase();

    match fmt_lower.as_str() {
        "xlsx" => {
            let mut workbook = rust_xlsxwriter::Workbook::new();
            let sheet = workbook.add_worksheet();
            let header_fmt = rust_xlsxwriter::Format::new().set_bold();
            let headers = [
                "Doc ID",
                "Kind",
                "Role",
                "External ID / DOI",
                "Title",
                "Authors / Assignees",
                "Date",
                "Venue / Authority",
                "DOI Verified",
                "Source Provider",
                "Match Reason",
                "Source URL",
            ];
            for (col, h) in headers.iter().enumerate() {
                let _ = sheet.write_string_with_format(0, col as u16, *h, &header_fmt);
            }
            for (idx, hit) in hits.iter().enumerate() {
                let r = (idx + 1) as u32;
                let title = if !hit.doc.title_zh.trim().is_empty() {
                    hit.doc.title_zh.as_str()
                } else {
                    hit.doc.title_en.as_str()
                };
                let _ = sheet.write_string(r, 0, &hit.doc.doc_id);
                let _ = sheet.write_string(r, 1, hit.doc.doc_kind.as_str());
                let _ = sheet.write_string(r, 2, hit.doc.asset_role.as_str());
                let _ = sheet.write_string(r, 3, &hit.doc.external_id);
                let _ = sheet.write_string(r, 4, title);
                let _ = sheet.write_string(r, 5, &hit.doc.authors_or_assignees);
                let _ = sheet.write_string(r, 6, &hit.doc.pub_or_filing_date);
                let _ = sheet.write_string(r, 7, &hit.doc.venue_or_authority);
                let _ = sheet.write_string(
                    r,
                    8,
                    if hit.doc.doi_verified {
                        "true"
                    } else {
                        "false"
                    },
                );
                let _ = sheet.write_string(r, 9, &hit.source_provider);
                let _ = sheet.write_string(r, 10, &hit.match_reason);
                let _ = sheet.write_string(r, 11, &hit.source_url);
            }
            let bytes = workbook.save_to_buffer()?;
            Ok(ExportedFile {
                filename: format!("omni_search_{ts}.xlsx"),
                content_type: "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
                    .to_string(),
                bytes,
            })
        }
        "pdf" => {
            let mut sections = Vec::new();
            let mut body = String::new();
            for (idx, hit) in hits.iter().take(25).enumerate() {
                let title = if !hit.doc.title_en.trim().is_empty() {
                    &hit.doc.title_en
                } else {
                    &hit.doc.title_zh
                };
                body.push_str(&format!(
                    "{}. [{}] {} ({}) - {}\n",
                    idx + 1,
                    hit.doc.doc_kind.as_str(),
                    title,
                    hit.doc.external_id,
                    hit.source_provider
                ));
            }
            if body.is_empty() {
                body.push_str("No matching records found.");
            }
            sections.push((format!("Omni-Search Report: {}", query), body));

            let compiled = {
                let conn = state.db.conn();
                ForgeExporter::compile_deliverable(
                    &conn,
                    &CompileDeliverableRequest {
                        title: format!("Omni-Search Survey: {}", query),
                        genre: DeliverableGenre::SurveyFtoReport,
                        format: DeliverableFormat::Pdf,
                        sections,
                        original_text_for_diff: None,
                        amended_text_for_diff: None,
                        related_doc_ids: Vec::new(),
                        reference_docs: hits.iter().map(|h| h.doc.clone()).collect(),
                        knowhow_markers: Vec::new(),
                        acknowledge_priority_warning: true,
                    },
                )?
            };
            Ok(ExportedFile {
                filename: format!("omni_search_{ts}.pdf"),
                content_type: "application/pdf".to_string(),
                bytes: compiled.bytes,
            })
        }
        _ => {
            let mut csv = String::from(
                "\u{FEFF}DocID,Kind,Role,ExternalID,Title,Authors,Date,Venue,DOIVerified,Provider,MatchReason,URL\n",
            );
            for hit in &hits {
                let title = if !hit.doc.title_zh.trim().is_empty() {
                    hit.doc.title_zh.as_str()
                } else {
                    hit.doc.title_en.as_str()
                };
                csv.push_str(&format!(
                    "{},{},{},{},{},{},{},{},{},{},{},{}\n",
                    escape_csv_cell(&hit.doc.doc_id),
                    escape_csv_cell(hit.doc.doc_kind.as_str()),
                    escape_csv_cell(hit.doc.asset_role.as_str()),
                    escape_csv_cell(&hit.doc.external_id),
                    escape_csv_cell(title),
                    escape_csv_cell(&hit.doc.authors_or_assignees),
                    escape_csv_cell(&hit.doc.pub_or_filing_date),
                    escape_csv_cell(&hit.doc.venue_or_authority),
                    if hit.doc.doi_verified {
                        "true"
                    } else {
                        "false"
                    },
                    escape_csv_cell(&hit.source_provider),
                    escape_csv_cell(&hit.match_reason),
                    escape_csv_cell(&hit.source_url),
                ));
            }
            Ok(ExportedFile {
                filename: format!("omni_search_{ts}.csv"),
                content_type: "text/csv; charset=utf-8".to_string(),
                bytes: csv.into_bytes(),
            })
        }
    }
}
