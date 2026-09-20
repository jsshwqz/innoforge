//! 检索域类型 / Search domain types
//!
//! 自 `src/patent.rs` 迁出（T1.1 缩水版，types-migration-map.md §1「检索族」）。
//! 字段、serde 属性与默认值函数逐字保持，仅更换定义位置。
//!
//! 本模块同时收留 types-migration-map.md §4 第 1 项登记的 **`SearchResult` 双定义**销账结果：
//! 原 `src/pipeline/context.rs` 中与检索响应同名的第二条定义（`id/title/snippet/link/source`）
//! 已按该表给的选项改名为 [`PipelinePatentHit`] 并归位到此，字段与 serde 形状不变。

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum SearchType {
    Applicant,    // 按申请人搜索
    Inventor,     // 按发明人搜索
    PatentNumber, // 按专利号搜索
    Keyword,      // 关键词搜索（标题/摘要）
    Mixed,        // 混合搜索
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchRequest {
    pub query: String,
    #[serde(default = "d1")]
    pub page: usize,
    #[serde(default = "d20")]
    pub page_size: usize,
    pub country: Option<String>,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub search_type: Option<String>, // "applicant", "inventor", "keyword", "mixed"
    pub sort_by: Option<String>,     // "relevance", "new", "old"
    #[serde(default)]
    pub ipc: Option<String>, // IPC classification filter (prefix match)
    #[serde(default)]
    pub cpc: Option<String>, // CPC classification filter (prefix match)
    #[serde(default)]
    pub region: Option<String>, // "cn" (国内) | "intl" (国外) | None (auto)
}

fn d1() -> usize {
    1
}
fn d20() -> usize {
    20
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchResult {
    pub patents: Vec<PatentSummary>,
    pub total: usize,
    pub page: usize,
    pub page_size: usize,
    pub search_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub categories: Option<Vec<CategoryGroup>>,
    #[serde(default)]
    pub dedup_removed: usize,
}

/// Group of search results by category (applicant, country, etc.)
#[derive(Debug, Serialize, Deserialize)]
pub struct CategoryGroup {
    pub label: String,
    pub count: usize,
}

/// MA1 追加 `PartialEq`：`search::model::MergedPatent` / `SearchOutcome` 按 spec §1 派生
/// `PartialEq`（多源合并结果需要可比较、可断言），其字段 `summary` 要求本类型可比；
/// 同时检索层的「改造前后逐字段等价」单测直接比对 `PatentSummary`。
/// serde 形状与字段均未变，属纯派生扩展。
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct PatentSummary {
    pub id: String,
    pub patent_number: String,
    pub title: String,
    pub abstract_text: String,
    pub applicant: String,
    pub inventor: String,
    pub filing_date: String,
    pub country: String,
    #[serde(default)]
    pub relevance_score: Option<f64>,
    #[serde(default)]
    pub score_source: Option<String>, // 评分来源说明
}

/// 流水线单条检索命中 / A single hit produced by the idea-validation pipeline.
///
/// 曾用名 `pipeline::context::SearchResult`——它与检索响应 [`SearchResult`] 同形异义
/// （本类型是「一条命中」，[`SearchResult`] 是「一次检索的完整响应」），
/// 是 types-migration-map.md §4 第 1 项登记的重复定义。改名后两义分离，字段与
/// serde 形状保持逐字不变（DB 中 `search_cache` 已存的历史 JSON 仍可正常反序列化）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelinePatentHit {
    pub id: String,
    pub title: String,
    pub snippet: String,
    pub link: String,
    pub source: String,
}

/// RAG 参考切片 — 检索到的专利文档片段
///
/// 自 `src/pipeline/context.rs` 迁出（types-migration-map.md §2：rag 与 db 双方使用，
/// 故归检索域）。字段与 serde 属性逐字保持。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReferenceChunk {
    pub id: String,
    pub patent_id: String,
    pub chunk_index: i32,
    pub source_type: String,
    pub content: String,
    pub relevance_score: f32,
}

#[cfg(test)]
mod tests {
    use super::PipelinePatentHit;

    /// 改名不得破坏历史缓存的反序列化：`search_cache` 里的旧 JSON 键集与此完全一致。
    #[test]
    fn pipeline_hit_deserializes_legacy_search_result_json() {
        let legacy =
            r#"[{"id":"web_0","title":"T","snippet":"S","link":"https://x","source":"serpapi"}]"#;
        let hits: Vec<PipelinePatentHit> =
            serde_json::from_str(legacy).expect("legacy pipeline hit json must deserialize");
        assert_eq!(1, hits.len());
        let hit = hits.first().unwrap();
        assert_eq!("web_0", hit.id);
        assert_eq!("T", hit.title);
        assert_eq!("S", hit.snippet);
        assert_eq!("https://x", hit.link);
        assert_eq!("serpapi", hit.source);
    }
}
