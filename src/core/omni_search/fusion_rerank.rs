//! 四维并发调度、熔断保护、跨源去重合并与可解释重排（`fusion_rerank.rs`）
//!
//! 承载 [`CircuitBreaker`] 与 [`OmniSearchService`]：
//! - 连续失败 ≥ 3 次自动熔断冷却，单源故障或超时绝不拖垮其他圈层；
//! - `search_across_realms` 使用 `futures::future::join_all` 并发调度全部勾选圈层适配器；
//! - 按归一化 DOI / 归一化专利公开号 / 代码仓库名 / 标准编号跨源去重合并，保留高信息量记录
//!   并合并 `source_provider`（如 `"openalex+crossref"`），生成可解释 `match_reason`。

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::core::vault::models::{canonical_patent_key, OmniSearchHit, SearchRealm};

use super::query_planner::QueryPlan;
use super::OmniSourceAdapter;

const DEFAULT_FAILURE_THRESHOLD: u32 = 3;
const DEFAULT_COOLDOWN: Duration = Duration::from_secs(60);
const PER_SOURCE_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, Default)]
struct BreakerEntry {
    consecutive_failures: u32,
    open_until: Option<Instant>,
}

/// 轻量源级熔断器：连续失败 ≥ `failure_threshold` 次后自动熔断冷却
#[derive(Debug, Clone)]
pub struct CircuitBreaker {
    failure_threshold: u32,
    cooldown: Duration,
    states: Arc<Mutex<HashMap<String, BreakerEntry>>>,
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self::new(DEFAULT_FAILURE_THRESHOLD, DEFAULT_COOLDOWN)
    }
}

impl CircuitBreaker {
    pub fn new(failure_threshold: u32, cooldown: Duration) -> Self {
        Self {
            failure_threshold: failure_threshold.max(1),
            cooldown,
            states: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// 检查指定检索源当前是否允许发起请求
    pub fn allow_request(&self, source_name: &str) -> bool {
        let Ok(mut guard) = self.states.lock() else {
            return true;
        };
        let Some(entry) = guard.get_mut(source_name) else {
            return true;
        };
        if let Some(until) = entry.open_until {
            if Instant::now() < until {
                return false;
            }
            // 冷却期结束，进入半开试探状态
            entry.open_until = None;
        }
        true
    }

    /// 是否处于熔断打开状态
    pub fn is_open(&self, source_name: &str) -> bool {
        let Ok(guard) = self.states.lock() else {
            return false;
        };
        guard
            .get(source_name)
            .and_then(|e| e.open_until)
            .map(|until| Instant::now() < until)
            .unwrap_or(false)
    }

    /// 获取当前连续失败计数
    pub fn consecutive_failures(&self, source_name: &str) -> u32 {
        let Ok(guard) = self.states.lock() else {
            return 0;
        };
        guard
            .get(source_name)
            .map(|e| e.consecutive_failures)
            .unwrap_or(0)
    }

    /// 记录一次成功调用，重置连续失败计数并关闭熔断器
    pub fn record_success(&self, source_name: &str) {
        if let Ok(mut guard) = self.states.lock() {
            guard.insert(
                source_name.to_string(),
                BreakerEntry {
                    consecutive_failures: 0,
                    open_until: None,
                },
            );
        }
    }

    /// 记录一次失败调用；若达到阈值则开启熔断冷却窗口
    pub fn record_failure(&self, source_name: &str) {
        if let Ok(mut guard) = self.states.lock() {
            let entry = guard.entry(source_name.to_string()).or_default();
            entry.consecutive_failures = entry.consecutive_failures.saturating_add(1);
            if entry.consecutive_failures >= self.failure_threshold {
                entry.open_until = Some(Instant::now() + self.cooldown);
            }
        }
    }
}

/// 单个检索源执行诊断信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceCallDiagnostic {
    pub source: String,
    pub realm: SearchRealm,
    pub status: String,
    pub hit_count: usize,
    pub elapsed_ms: u64,
    pub error: Option<String>,
}

/// 四维全域并发检索汇总结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OmniSearchOutcome {
    pub hits: Vec<OmniSearchHit>,
    pub diagnostics: Vec<SourceCallDiagnostic>,
    pub total_before_dedup: usize,
}

/// 四维全域情报检索统一调度服务
pub struct OmniSearchService {
    adapters: Vec<Arc<dyn OmniSourceAdapter>>,
    breaker: CircuitBreaker,
}

impl OmniSearchService {
    pub fn new(adapters: Vec<Arc<dyn OmniSourceAdapter>>) -> Self {
        Self {
            adapters,
            breaker: CircuitBreaker::default(),
        }
    }

    pub fn with_breaker_config(
        adapters: Vec<Arc<dyn OmniSourceAdapter>>,
        failure_threshold: u32,
        cooldown: Duration,
    ) -> Self {
        Self {
            adapters,
            breaker: CircuitBreaker::new(failure_threshold, cooldown),
        }
    }

    pub fn breaker(&self) -> &CircuitBreaker {
        &self.breaker
    }

    /// 按勾选圈层并发调度全部已启用适配器，经熔断保护、跨源去重合并与相关性重排返回结果
    pub async fn search_across_realms(
        &self,
        plan: &QueryPlan,
        realms: &[SearchRealm],
        max_per_source: usize,
    ) -> OmniSearchOutcome {
        let active_adapters: Vec<Arc<dyn OmniSourceAdapter>> = self
            .adapters
            .iter()
            .filter(|a| realms.is_empty() || realms.contains(&a.realm()))
            .cloned()
            .collect();

        let futures = active_adapters.into_iter().map(|adapter| async move {
            let source_name = adapter.name().to_string();
            let realm = adapter.realm();

            if !self.breaker.allow_request(&source_name) {
                return (
                    Vec::new(),
                    SourceCallDiagnostic {
                        source: source_name,
                        realm,
                        status: "circuit_open".to_string(),
                        hit_count: 0,
                        elapsed_ms: 0,
                        error: Some("source circuit breaker is open".to_string()),
                    },
                );
            }

            let started = Instant::now();
            let call_res =
                tokio::time::timeout(PER_SOURCE_TIMEOUT, adapter.search(plan, max_per_source))
                    .await;
            let elapsed_ms = started.elapsed().as_millis() as u64;

            match call_res {
                Ok(Ok(hits)) => {
                    self.breaker.record_success(&source_name);
                    let count = hits.len();
                    (
                        hits,
                        SourceCallDiagnostic {
                            source: source_name,
                            realm,
                            status: "ok".to_string(),
                            hit_count: count,
                            elapsed_ms,
                            error: None,
                        },
                    )
                }
                Ok(Err(err)) => {
                    self.breaker.record_failure(&source_name);
                    (
                        Vec::new(),
                        SourceCallDiagnostic {
                            source: source_name,
                            realm,
                            status: "error".to_string(),
                            hit_count: 0,
                            elapsed_ms,
                            error: Some(err.to_string()),
                        },
                    )
                }
                Err(_) => {
                    self.breaker.record_failure(&source_name);
                    (
                        Vec::new(),
                        SourceCallDiagnostic {
                            source: source_name,
                            realm,
                            status: "timeout".to_string(),
                            hit_count: 0,
                            elapsed_ms,
                            error: Some("adapter search timed out".to_string()),
                        },
                    )
                }
            }
        });

        let results = futures::future::join_all(futures).await;

        let mut raw_hits = Vec::new();
        let mut diagnostics = Vec::with_capacity(results.len());
        for (hits, diag) in results {
            raw_hits.extend(hits);
            diagnostics.push(diag);
        }

        let total_before_dedup = raw_hits.len();
        let merged = deduplicate_and_merge(raw_hits);
        let reranked = rerank_with_explanations(merged, plan);

        OmniSearchOutcome {
            hits: reranked,
            diagnostics,
            total_before_dedup,
        }
    }
}

fn deduplicate_and_merge(hits: Vec<OmniSearchHit>) -> Vec<OmniSearchHit> {
    let mut index_by_key: HashMap<String, usize> = HashMap::new();
    let mut merged: Vec<OmniSearchHit> = Vec::new();

    for hit in hits {
        let key = compute_dedup_key(&hit);
        if let Some(&existing_idx) = index_by_key.get(&key) {
            if let Some(existing) = merged.get_mut(existing_idx) {
                merge_hits(existing, hit);
            }
        } else {
            let idx = merged.len();
            index_by_key.insert(key, idx);
            merged.push(hit);
        }
    }

    merged
}

fn compute_dedup_key(hit: &OmniSearchHit) -> String {
    match hit.realm {
        SearchRealm::Scholar => {
            if hit.doc.doi_verified && !hit.doc.external_id.trim().is_empty() {
                return format!("doi:{}", hit.doc.external_id.trim().to_lowercase());
            }
            let title_key = normalize_title_key(best_title(hit));
            if !title_key.is_empty() {
                return format!("scholar_title:{title_key}");
            }
            format!("scholar_id:{}", hit.doc.doc_id)
        }
        SearchRealm::Patent => {
            let pat_key = canonical_patent_key(&hit.doc.external_id);
            if !pat_key.is_empty() {
                return format!("patent:{pat_key}");
            }
            let title_key = normalize_title_key(best_title(hit));
            if !title_key.is_empty() {
                return format!("patent_title:{title_key}");
            }
            format!("patent_id:{}", hit.doc.doc_id)
        }
        SearchRealm::CodeRepo => {
            if !hit.doc.external_id.trim().is_empty() {
                format!("code:{}", hit.doc.external_id.trim().to_lowercase())
            } else {
                format!("code_url:{}", hit.source_url.trim().to_lowercase())
            }
        }
        SearchRealm::Standard => {
            if !hit.doc.external_id.trim().is_empty() {
                format!("std:{}", hit.doc.external_id.trim().to_uppercase())
            } else {
                format!("std_title:{}", normalize_title_key(best_title(hit)))
            }
        }
    }
}

fn merge_hits(existing: &mut OmniSearchHit, incoming: OmniSearchHit) {
    // 合并 source_provider，例如 "openalex" + "crossref" -> "openalex+crossref"
    let mut providers: Vec<String> = existing
        .source_provider
        .split('+')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    for p in incoming.source_provider.split('+') {
        let p_trimmed = p.trim();
        if !p_trimmed.is_empty() && !providers.iter().any(|x| x == p_trimmed) {
            providers.push(p_trimmed.to_string());
        }
    }
    existing.source_provider = providers.join("+");

    // 保留高信息量字段：优先 doi_verified=true、更长摘要、非空 BibTeX、更完整作者与期刊
    if incoming.doc.doi_verified && !existing.doc.doi_verified {
        existing.doc.doi_verified = true;
        existing.doc.external_id = incoming.doc.external_id;
    }
    if incoming.doc.abstract_text.chars().count() > existing.doc.abstract_text.chars().count() {
        existing.doc.abstract_text = incoming.doc.abstract_text;
    }
    if existing.doc.bibtex_raw.trim().is_empty() && !incoming.doc.bibtex_raw.trim().is_empty() {
        existing.doc.bibtex_raw = incoming.doc.bibtex_raw;
    }
    if existing.doc.authors_or_assignees.trim().is_empty()
        && !incoming.doc.authors_or_assignees.trim().is_empty()
    {
        existing.doc.authors_or_assignees = incoming.doc.authors_or_assignees;
    }
    if existing.doc.venue_or_authority.trim().is_empty()
        && !incoming.doc.venue_or_authority.trim().is_empty()
    {
        existing.doc.venue_or_authority = incoming.doc.venue_or_authority;
    }
    if existing.doc.pub_or_filing_date.trim().is_empty()
        && !incoming.doc.pub_or_filing_date.trim().is_empty()
    {
        existing.doc.pub_or_filing_date = incoming.doc.pub_or_filing_date;
    }
    if existing.source_url.trim().is_empty() && !incoming.source_url.trim().is_empty() {
        existing.source_url = incoming.source_url;
    }

    // 多源交叉印证小幅提权
    let base_max = existing.relevance_score.max(incoming.relevance_score);
    existing.relevance_score = (base_max + 0.08).min(1.0);
}

fn rerank_with_explanations(mut hits: Vec<OmniSearchHit>, plan: &QueryPlan) -> Vec<OmniSearchHit> {
    let mut query_terms: Vec<String> = Vec::new();
    for term in plan.keywords.iter().chain(plan.expanded_terms.iter()) {
        let lower = term.trim().to_lowercase();
        if !lower.is_empty() && !query_terms.contains(&lower) {
            query_terms.push(lower);
        }
    }

    // 统计各圈层命中数，用于生成圈层互补说明
    let mut realm_counts: BTreeMap<&'static str, usize> = BTreeMap::new();
    for hit in &hits {
        *realm_counts.entry(realm_label(hit.realm)).or_insert(0) += 1;
    }

    for hit in &mut hits {
        let haystack = format!(
            "{} {} {} {}",
            hit.doc.title_zh, hit.doc.title_en, hit.doc.abstract_text, hit.doc.external_id
        )
        .to_lowercase();

        let matched: Vec<&str> = query_terms
            .iter()
            .filter(|t| haystack.contains(t.as_str()))
            .map(|t| t.as_str())
            .take(5)
            .collect();

        let overlap_bonus = if query_terms.is_empty() {
            0.0
        } else {
            (matched.len() as f64 / query_terms.len().min(6) as f64) * 0.12
        };
        let doi_bonus = if hit.doc.doi_verified { 0.03 } else { 0.0 };
        hit.relevance_score = (hit.relevance_score + overlap_bonus + doi_bonus).clamp(0.0, 1.0);

        let kw_note = if matched.is_empty() {
            "语义关联召回".to_string()
        } else {
            format!("命中词: [{}]", matched.join(", "))
        };
        let realm_note = realm_complement_note(hit.realm);
        if hit.match_reason.trim().is_empty() {
            hit.match_reason = format!("{kw_note} · {realm_note}");
        } else {
            hit.match_reason = format!("{} | {kw_note} · {realm_note}", hit.match_reason);
        }
    }

    hits.sort_by(|a, b| {
        b.relevance_score
            .partial_cmp(&a.relevance_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    hits
}

fn best_title(hit: &OmniSearchHit) -> &str {
    if !hit.doc.title_en.trim().is_empty() {
        &hit.doc.title_en
    } else {
        &hit.doc.title_zh
    }
}

fn normalize_title_key(title: &str) -> String {
    title
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

fn realm_label(realm: SearchRealm) -> &'static str {
    match realm {
        SearchRealm::Patent => "专利",
        SearchRealm::Scholar => "学术论文",
        SearchRealm::CodeRepo => "开源工程",
        SearchRealm::Standard => "行业标准",
    }
}

fn realm_complement_note(realm: SearchRealm) -> &'static str {
    match realm {
        SearchRealm::Patent => "专利权属与 FTO 边界参考",
        SearchRealm::Scholar => "机理验证与学术 Baseline 参考",
        SearchRealm::CodeRepo => "开源工程实现与协议合规参考",
        SearchRealm::Standard => "行业标准与测试规范基线",
    }
}
