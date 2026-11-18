//! O(1) 原文/DOI/实验铁证回查模块（`src/core/evidence_rebuttal/anchor_verifier.rs`）
//!
//! 正则扫描草稿中的四类标准引证锚点：
//! - 专利权利要求锚点：`[doc_id#Claim:N]` 或 `Claim:N` / `权利要求N`
//! - 专利说明书段落锚点：`[doc_id#Desc:[0024]]` 或 `Desc:[0024]` / `第[0024]段` / `[0024]`
//! - 论文与真实 DOI 锚点：`DOI:10.xxxx/...`、`[doc_id#Paper:Sec3.1]`、`Paper:Sec3.1`、`Paper:Table2`
//! - 对照实验锚点：`Exp:{exp_id}`
//!
//! 全部回查通过 SQLite 唯一索引与主键单语句 O(1) 精确命中，零全表扫描。

use std::sync::OnceLock;

use anyhow::Result;
use regex::Regex;
use rusqlite::{params, Connection, OptionalExtension};

use crate::core::vault::models::{CitationVerdict, VerifiedCitation};

/// 字符边界安全截断（严禁裸字节切片）
pub fn safe_truncate(s: &str, max_chars: usize) -> String {
    let mut chars = s.chars();
    let taken: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{}...", taken)
    } else {
        taken
    }
}

fn compile_regex(slot: &'static OnceLock<Option<Regex>>, pattern: &str) -> Option<&'static Regex> {
    slot.get_or_init(|| Regex::new(pattern).ok()).as_ref()
}

/// 扫描草稿中的全部引证锚点并在 SQLite 金库中执行 O(1) 核验
pub fn verify_all_citations(
    conn: &Connection,
    draft_text: &str,
    default_doc_id: Option<&str>,
) -> Result<Vec<VerifiedCitation>> {
    static RE_DOC_CLAIM: OnceLock<Option<Regex>> = OnceLock::new();
    static RE_DOC_DESC: OnceLock<Option<Regex>> = OnceLock::new();
    static RE_DOC_PAPER: OnceLock<Option<Regex>> = OnceLock::new();
    static RE_DOI: OnceLock<Option<Regex>> = OnceLock::new();
    static RE_EXP: OnceLock<Option<Regex>> = OnceLock::new();
    static RE_CLAIM: OnceLock<Option<Regex>> = OnceLock::new();
    static RE_DESC: OnceLock<Option<Regex>> = OnceLock::new();
    static RE_BARE_PARA: OnceLock<Option<Regex>> = OnceLock::new();
    static RE_PAPER: OnceLock<Option<Regex>> = OnceLock::new();

    let mut matched_spans: Vec<(usize, usize)> = Vec::new();
    let mut results: Vec<(usize, VerifiedCitation)> = Vec::new();

    let overlaps = |spans: &[(usize, usize)], start: usize, end: usize| -> bool {
        spans.iter().any(|&(s, e)| start < e && end > s)
    };

    // 1. [doc_id#Claim:N]
    if let Some(re) = compile_regex(&RE_DOC_CLAIM, r"\[([A-Za-z0-9_.\-]+)#(?i:Claim):(\d+)\]") {
        for cap in re.captures_iter(draft_text) {
            if let (Some(full), Some(doc_m), Some(num_m)) = (cap.get(0), cap.get(1), cap.get(2)) {
                if !overlaps(&matched_spans, full.start(), full.end()) {
                    matched_spans.push((full.start(), full.end()));
                    let anchor_tag = format!("Claim:{}", num_m.as_str());
                    let verified =
                        verify_doc_chunk(conn, full.as_str(), doc_m.as_str(), &anchor_tag)?;
                    results.push((full.start(), verified));
                }
            }
        }
    }

    // 2. [doc_id#Desc:[0024]] 或 [doc_id#Desc:[0023-0024]]
    if let Some(re) = compile_regex(
        &RE_DOC_DESC,
        r"\[([A-Za-z0-9_.\-]+)#(?i:Desc):\[(\d{1,5}(?:-\d{1,5})?)\]\]",
    ) {
        for cap in re.captures_iter(draft_text) {
            if let (Some(full), Some(doc_m), Some(para_m)) = (cap.get(0), cap.get(1), cap.get(2)) {
                if !overlaps(&matched_spans, full.start(), full.end()) {
                    matched_spans.push((full.start(), full.end()));
                    let anchor_tag = format!("Desc:[{}]", para_m.as_str());
                    let verified =
                        verify_doc_chunk(conn, full.as_str(), doc_m.as_str(), &anchor_tag)?;
                    results.push((full.start(), verified));
                }
            }
        }
    }

    // 3. [doc_id#Paper:Sec3.1] / [doc_id#Paper:Table2]
    if let Some(re) = compile_regex(
        &RE_DOC_PAPER,
        r"\[([A-Za-z0-9_.\-]+)#(Paper:[A-Za-z0-9_.\-]+)\]",
    ) {
        for cap in re.captures_iter(draft_text) {
            if let (Some(full), Some(doc_m), Some(tag_m)) = (cap.get(0), cap.get(1), cap.get(2)) {
                if !overlaps(&matched_spans, full.start(), full.end()) {
                    matched_spans.push((full.start(), full.end()));
                    let verified =
                        verify_doc_chunk(conn, full.as_str(), doc_m.as_str(), tag_m.as_str())?;
                    results.push((full.start(), verified));
                }
            }
        }
    }

    // 4. DOI:10.xxxx/...
    if let Some(re) = compile_regex(&RE_DOI, r"(?i)DOI:\s*(10\.\d{4,9}/[^\s\]\)）,;；。，<>]+)")
    {
        for cap in re.captures_iter(draft_text) {
            if let (Some(full), Some(doi_m)) = (cap.get(0), cap.get(1)) {
                if !overlaps(&matched_spans, full.start(), full.end()) {
                    matched_spans.push((full.start(), full.end()));
                    let doi_clean = doi_m.as_str().trim_end_matches('.');
                    let verified = verify_doi(conn, full.as_str(), doi_clean)?;
                    results.push((full.start(), verified));
                }
            }
        }
    }

    // 5. Exp:{exp_id}
    if let Some(re) = compile_regex(&RE_EXP, r"\bExp:([A-Za-z0-9_\-]+)") {
        for cap in re.captures_iter(draft_text) {
            if let (Some(full), Some(exp_m)) = (cap.get(0), cap.get(1)) {
                if !overlaps(&matched_spans, full.start(), full.end()) {
                    matched_spans.push((full.start(), full.end()));
                    let verified = verify_experiment(conn, full.as_str(), exp_m.as_str())?;
                    results.push((full.start(), verified));
                }
            }
        }
    }

    // 6. 搭配默认对比文献的独立锚点
    let fallback_doc = default_doc_id.unwrap_or("").trim();

    // 6a. Claim:N 或 权利要求N
    if let Some(re) = compile_regex(&RE_CLAIM, r"(?:(?i:Claim):|权利要求\s*)(\d+)") {
        for cap in re.captures_iter(draft_text) {
            if let (Some(full), Some(num_m)) = (cap.get(0), cap.get(1)) {
                if !overlaps(&matched_spans, full.start(), full.end()) {
                    matched_spans.push((full.start(), full.end()));
                    let anchor_tag = format!("Claim:{}", num_m.as_str());
                    let verified = if fallback_doc.is_empty() {
                        VerifiedCitation {
                            raw_mention: full.as_str().to_string(),
                            doc_id: String::new(),
                            anchor_tag,
                            verdict: CitationVerdict::Unverified,
                            matched_excerpt: String::new(),
                            reason: "未指定对比文献 doc_id，无法定位权利要求锚点".to_string(),
                        }
                    } else {
                        verify_doc_chunk(conn, full.as_str(), fallback_doc, &anchor_tag)?
                    };
                    results.push((full.start(), verified));
                }
            }
        }
    }

    // 6b. Desc:[0024] 或 第[0024]段 / 第0024段
    if let Some(re) = compile_regex(
        &RE_DESC,
        r"(?:(?i:Desc):\[(\d{1,5}(?:-\d{1,5})?)\]|第\[?(\d{4})\]?段)",
    ) {
        for cap in re.captures_iter(draft_text) {
            if let Some(full) = cap.get(0) {
                if !overlaps(&matched_spans, full.start(), full.end()) {
                    let para = cap
                        .get(1)
                        .or_else(|| cap.get(2))
                        .map(|m| m.as_str())
                        .unwrap_or("");
                    if !para.is_empty() {
                        matched_spans.push((full.start(), full.end()));
                        let anchor_tag = format!("Desc:[{}]", para);
                        let verified = if fallback_doc.is_empty() {
                            VerifiedCitation {
                                raw_mention: full.as_str().to_string(),
                                doc_id: String::new(),
                                anchor_tag,
                                verdict: CitationVerdict::Unverified,
                                matched_excerpt: String::new(),
                                reason: "未指定对比文献 doc_id，无法定位说明书段落锚点".to_string(),
                            }
                        } else {
                            verify_doc_chunk(conn, full.as_str(), fallback_doc, &anchor_tag)?
                        };
                        results.push((full.start(), verified));
                    }
                }
            }
        }
    }

    // 6c. 裸段落号 [0024]（仅在未与上述规则重叠时匹配）
    if let Some(re) = compile_regex(&RE_BARE_PARA, r"\[(\d{4})\]") {
        for cap in re.captures_iter(draft_text) {
            if let (Some(full), Some(para_m)) = (cap.get(0), cap.get(1)) {
                if !overlaps(&matched_spans, full.start(), full.end()) {
                    matched_spans.push((full.start(), full.end()));
                    let anchor_tag = format!("Desc:[{}]", para_m.as_str());
                    let verified = if fallback_doc.is_empty() {
                        VerifiedCitation {
                            raw_mention: full.as_str().to_string(),
                            doc_id: String::new(),
                            anchor_tag,
                            verdict: CitationVerdict::Unverified,
                            matched_excerpt: String::new(),
                            reason: "未指定对比文献 doc_id，无法定位说明书段落锚点".to_string(),
                        }
                    } else {
                        verify_doc_chunk(conn, full.as_str(), fallback_doc, &anchor_tag)?
                    };
                    results.push((full.start(), verified));
                }
            }
        }
    }

    // 6d. Paper:Sec... / Paper:Table... / Paper:Fig...
    if let Some(re) = compile_regex(&RE_PAPER, r"\b(Paper:[A-Za-z0-9_.\-]+)") {
        for cap in re.captures_iter(draft_text) {
            if let (Some(full), Some(tag_m)) = (cap.get(0), cap.get(1)) {
                if !overlaps(&matched_spans, full.start(), full.end()) {
                    matched_spans.push((full.start(), full.end()));
                    let anchor_tag = tag_m.as_str().to_string();
                    let verified = if fallback_doc.is_empty() {
                        VerifiedCitation {
                            raw_mention: full.as_str().to_string(),
                            doc_id: String::new(),
                            anchor_tag,
                            verdict: CitationVerdict::Unverified,
                            matched_excerpt: String::new(),
                            reason: "未指定论文 doc_id，无法定位论文章节/图表锚点".to_string(),
                        }
                    } else {
                        verify_doc_chunk(conn, full.as_str(), fallback_doc, &anchor_tag)?
                    };
                    results.push((full.start(), verified));
                }
            }
        }
    }

    results.sort_by_key(|(pos, _)| *pos);
    Ok(results.into_iter().map(|(_, v)| v).collect())
}

/// 通过唯一索引 `(doc_id, section_tag)` O(1) 查询 `doc_chunks`
fn verify_doc_chunk(
    conn: &Connection,
    raw_mention: &str,
    doc_id: &str,
    anchor_tag: &str,
) -> Result<VerifiedCitation> {
    let row: Option<(String, String)> = conn
        .query_row(
            "SELECT dc.raw_content, kd.fulltext_status
             FROM doc_chunks dc
             JOIN knowledge_docs kd ON kd.doc_id = dc.doc_id
             WHERE dc.doc_id = ?1 AND dc.section_tag = ?2
             LIMIT 1",
            params![doc_id, anchor_tag],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;

    if let Some((raw_content, fulltext_status)) = row {
        let excerpt = safe_truncate(&raw_content, 160);
        if fulltext_status.trim() == "summary_only" {
            return Ok(VerifiedCitation {
                raw_mention: raw_mention.to_string(),
                doc_id: doc_id.to_string(),
                anchor_tag: anchor_tag.to_string(),
                verdict: CitationVerdict::Summary,
                matched_excerpt: excerpt,
                reason: "文献仅含摘要级证据（fulltext_status=summary_only），缺少全文切片"
                    .to_string(),
            });
        }
        return Ok(VerifiedCitation {
            raw_mention: raw_mention.to_string(),
            doc_id: doc_id.to_string(),
            anchor_tag: anchor_tag.to_string(),
            verdict: CitationVerdict::Verified,
            matched_excerpt: excerpt,
            reason: "原文切片唯一索引 O(1) 精确命中".to_string(),
        });
    }

    // 检查文献是否存在，给出明确失配/伪造原因
    let doc_status: Option<String> = conn
        .query_row(
            "SELECT fulltext_status FROM knowledge_docs WHERE doc_id = ?1 LIMIT 1",
            params![doc_id],
            |r| r.get(0),
        )
        .optional()?;

    let reason = match doc_status {
        None => format!("目标文献 {} 在本地知识金库中不存在（疑似伪造文献）", doc_id),
        Some(_) => format!(
            "文献 {} 中不存在锚点 {}（疑似伪造段落号/章节号或切片失配）",
            doc_id, anchor_tag
        ),
    };

    Ok(VerifiedCitation {
        raw_mention: raw_mention.to_string(),
        doc_id: doc_id.to_string(),
        anchor_tag: anchor_tag.to_string(),
        verdict: CitationVerdict::Unverified,
        matched_excerpt: String::new(),
        reason,
    })
}

/// O(1) 校验真实 DOI（`knowledge_docs.doi_verified == 1`）
fn verify_doi(conn: &Connection, raw_mention: &str, doi_clean: &str) -> Result<VerifiedCitation> {
    let doi_lower = doi_clean.trim().to_ascii_lowercase();
    let row: Option<(String, String, String, i64, String, String)> = conn
        .query_row(
            "SELECT doc_id, title_zh, title_en, doi_verified, fulltext_status, abstract_text
             FROM knowledge_docs
             WHERE LOWER(external_id) = ?1 OR LOWER(doc_id) = ?1
             LIMIT 1",
            params![doi_lower],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            },
        )
        .optional()?;

    match row {
        None => Ok(VerifiedCitation {
            raw_mention: raw_mention.to_string(),
            doc_id: String::new(),
            anchor_tag: format!("DOI:{}", doi_clean),
            verdict: CitationVerdict::Unverified,
            matched_excerpt: String::new(),
            reason: format!(
                "DOI {} 在本地知识金库中不存在（疑似幻觉伪造引文）",
                doi_clean
            ),
        }),
        Some((doc_id, title_zh, title_en, doi_verified, fulltext_status, abstract_text)) => {
            if doi_verified != 1 {
                return Ok(VerifiedCitation {
                    raw_mention: raw_mention.to_string(),
                    doc_id,
                    anchor_tag: format!("DOI:{}", doi_clean),
                    verdict: CitationVerdict::Unverified,
                    matched_excerpt: String::new(),
                    reason: format!("DOI {} 未通过元数据源核验（doi_verified != 1）", doi_clean),
                });
            }
            let title = if !title_en.is_empty() {
                title_en
            } else {
                title_zh
            };
            let excerpt_src = if abstract_text.is_empty() {
                title
            } else {
                format!("{} — {}", title, abstract_text)
            };
            let excerpt = safe_truncate(&excerpt_src, 160);
            if fulltext_status.trim() == "summary_only" {
                Ok(VerifiedCitation {
                    raw_mention: raw_mention.to_string(),
                    doc_id,
                    anchor_tag: format!("DOI:{}", doi_clean),
                    verdict: CitationVerdict::Summary,
                    matched_excerpt: excerpt,
                    reason: "DOI 已核验，但该文献当前仅含摘要（fulltext_status=summary_only）"
                        .to_string(),
                })
            } else {
                Ok(VerifiedCitation {
                    raw_mention: raw_mention.to_string(),
                    doc_id,
                    anchor_tag: format!("DOI:{}", doi_clean),
                    verdict: CitationVerdict::Verified,
                    matched_excerpt: excerpt,
                    reason: "DOI 已核验且具备完整文献信息".to_string(),
                })
            }
        }
    }
}

/// O(1) 校验对照实验实测账本（`experiment_records`）
fn verify_experiment(
    conn: &Connection,
    raw_mention: &str,
    exp_id: &str,
) -> Result<VerifiedCitation> {
    let row: Option<(String, Option<String>, String)> = conn
        .query_row(
            "SELECT title, measured_data_json, status
             FROM experiment_records
             WHERE exp_id = ?1
             LIMIT 1",
            params![exp_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;

    match row {
        None => Ok(VerifiedCitation {
            raw_mention: raw_mention.to_string(),
            doc_id: exp_id.to_string(),
            anchor_tag: format!("Exp:{}", exp_id),
            verdict: CitationVerdict::Unverified,
            matched_excerpt: String::new(),
            reason: format!("对照实验记录 Exp:{} 不存在", exp_id),
        }),
        Some((title, measured_data_json, status)) => {
            let st = status.trim();
            if st == "real_data_filled" || st == "verified" {
                let data_str = measured_data_json.unwrap_or_default();
                let excerpt = if data_str.trim().is_empty() {
                    safe_truncate(&title, 160)
                } else {
                    safe_truncate(&format!("{} | {}", title, data_str), 160)
                };
                Ok(VerifiedCitation {
                    raw_mention: raw_mention.to_string(),
                    doc_id: exp_id.to_string(),
                    anchor_tag: format!("Exp:{}", exp_id),
                    verdict: CitationVerdict::Verified,
                    matched_excerpt: excerpt,
                    reason: format!("对照实验已回填实测数据（status={}）", st),
                })
            } else {
                Ok(VerifiedCitation {
                    raw_mention: raw_mention.to_string(),
                    doc_id: exp_id.to_string(),
                    anchor_tag: format!("Exp:{}", exp_id),
                    verdict: CitationVerdict::Unverified,
                    matched_excerpt: String::new(),
                    reason: format!(
                        "实验 Exp:{} 当前状态为 {}，尚未回填真实测量数据（禁止作为实测铁证）",
                        exp_id, st
                    ),
                })
            }
        }
    }
}
