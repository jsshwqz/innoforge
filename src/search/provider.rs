//! 检索源抽象 / The `SearchProvider` trait（spec §1「统一契约」，先于任何源实现）
//!
//! 为什么手写 `Pin<Box<dyn Future>>` 而不用 `async_trait`：AGENTS.md 与本里程碑禁止引入新 crate 依赖，
//! 而 `async_trait` 不在 `Cargo.toml` 里。代价是实现方要写 `Box::pin(async move { … })`，
//! 收益是 trait 保持 object-safe —— MA2 的源注册表需要 `Vec<Box<dyn SearchProvider>>`。

use crate::search::model::{SearchOutcome, SearchQuery, SourceKind};
use crate::types::search::PatentSummary;
use std::future::Future;
use std::pin::Pin;

/// 一个可插拔的在线检索源。实现方**必须**自带超时（spec §1 建议 15s，MA1 沿用现役 30s，
/// 收紧在 MA2 执行链落实），并以 [`SearchOutcome::attempts`] 汇报每一次尝试的结果或失败分类。
pub trait SearchProvider: Send + Sync {
    /// 源标识，用于 `AttemptReport` 与 MA5 诊断面板。
    fn kind(&self) -> SourceKind;

    /// 关键词/条件检索。永不返回 `Err`：失败以 `AttemptStatus::Failed(FailKind)` 记账，
    /// 由调用方（MA2 执行链）决定是否切下一源。
    fn search<'a>(
        &'a self,
        query: SearchQuery,
    ) -> Pin<Box<dyn Future<Output = SearchOutcome> + Send + 'a>>;

    /// 精确专利号直查（可选能力，spec §1 未列，MA1 按现役 `/api/search/online` 行为保留）。
    /// 默认 `None` = 该源没有 details 形态的端点；`Vec<Box<dyn SearchProvider>>` 要求方法
    /// 在所有实现上同签名，故用默认实现而非 `Option<...>` 字段表达「可选」。
    #[allow(dead_code)] // MA2 起 Google Patents XHR 走默认实现；MA1 只有 SerpAPI 覆写
    fn lookup_exact<'a>(
        &'a self,
        _query: String,
    ) -> Pin<Box<dyn Future<Output = Option<PatentSummary>> + Send + 'a>> {
        Box::pin(async move { None })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::search::providers::serpapi::SerpApiProvider;
    use std::sync::Arc;

    /// object-safety 证明：MA2 需要把多个源装进同一注册表。
    /// 若 trait 因新增非 dyn-compatible 方法而破功，本用例编译不过。
    #[test]
    fn trait_is_dyn_compatible_and_registry_builds() {
        let db = Arc::new(Database::init(":memory:").expect("in-memory db"));
        let provider = SerpApiProvider::new("test-key".to_string(), db);
        let registry: Vec<Box<dyn SearchProvider>> = vec![Box::new(provider)];
        assert_eq!(1, registry.len());
        assert_eq!(
            SourceKind::SerpApi,
            registry.first().expect("one provider").kind()
        );
    }
}
