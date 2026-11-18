//! 四维全域情报检索与真实 DOI 核验内核（`src/core/omni_search/`）
//!
//! 严格对应 `AGENTS.md` 公理 3（四维全域情报检索：全球专利 + 学术论文 + 开源工程 + 行业标准）
//! 与 `docs/plans/2026-11-18-v1-overhaul-plan.md` §3.2.3 / 第六章 `T-203`。
//!
//! 所有 HTTP 客户端统一通过 [`build_http_client`] 构建，显式设置 `connect_timeout(500ms)` 与
//! `timeout(15s)`，严禁 `Client::new()`、严禁 `unwrap()`/`expect()`、严禁裸字节切片 `[..`。

pub mod code_std_sources;
pub mod fusion_rerank;
pub mod patent_sources;
pub mod query_planner;
pub mod scholar_sources;

pub use code_std_sources::{OpenSourceCodeAdapter, TechStandardAdapter};
pub use fusion_rerank::{
    CircuitBreaker, OmniSearchOutcome, OmniSearchService, SourceCallDiagnostic,
};
pub use patent_sources::{LocalSqlitePatentAdapter, SerpApiPatentAdapter};
pub use query_planner::{QueryPlan, QueryPlanner};
pub use scholar_sources::{
    build_bibtex_entry, normalize_and_verify_doi, ArxivAdapter, CrossrefAdapter, OpenAlexAdapter,
    SemanticScholarAdapter,
};

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use crate::core::vault::models::{OmniSearchHit, SearchRealm};

/// 异步适配器统一 Future 别名（支持 `Vec<Arc<dyn OmniSourceAdapter>>` 动态分发并发调度）
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// 四维全域检索源统一异步适配器 Trait
pub trait OmniSourceAdapter: Send + Sync {
    /// 适配器唯一标识名（如 `"openalex"`, `"crossref"`, `"arxiv"`, `"serpapi_patent"`）
    fn name(&self) -> &'static str;

    /// 所属检索圈层
    fn realm(&self) -> SearchRealm;

    /// 执行一次检索并归一化为 [`OmniSearchHit`]
    fn search<'a>(
        &'a self,
        plan: &'a QueryPlan,
        max_results: usize,
    ) -> BoxFuture<'a, anyhow::Result<Vec<OmniSearchHit>>>;
}

/// 统一构建带显式连接/请求超时的异步 HTTP 客户端（严禁 `Client::new()`）
pub fn build_http_client() -> anyhow::Result<reqwest::Client> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_millis(500))
        .timeout(Duration::from_secs(15))
        .user_agent("InnoForge/1.0 (https://github.com/jsshwqz/innoforge)")
        .build()
        .map_err(|err| anyhow::anyhow!("failed to build reqwest HTTP client: {err}"))
}

/// 按 Unicode 字符数安全截断文本（不使用裸字节切片，杜绝 UTF-8 边界 panic）
pub(crate) fn safe_truncate_chars(input: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let mut out = String::new();
    for (idx, ch) in input.chars().enumerate() {
        if idx >= max_chars {
            out.push_str("...");
            break;
        }
        out.push(ch);
    }
    out
}
