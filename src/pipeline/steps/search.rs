//! Step 3-4: SearchWeb + SearchPatents
//!
//! 类型：CODE（HTTP 请求，不调用 LLM）
//!
//! MC1 起：SearchPatents 改走 M-A 多源检索链（SourceChain），
//! 复用 routes/search.rs 的 online_chain_providers 装配逻辑。
//! SearchWeb 仍走 SerpAPI（网络搜索与专利搜索语义不同）。

use crate::db::Database;
use crate::pipeline::context::{PipelineContext, PipelinePatentHit};
use crate::search::chain::SourceChain;
use crate::search::model::{Lang, SearchQuery};
use crate::search::provider::SearchProvider;
use crate::search::providers::epo_ops::EpoOpsProvider;
use crate::search::providers::google_patents_xhr::GooglePatentsXhrProvider;
use crate::search::providers::serpapi::SerpApiProvider;
use crate::types::search::SearchType;
use anyhow::Result;
use reqwest::Client;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::Duration;

const SEARCH_UPSTREAM_TIMEOUT_SECS: u64 = 8;

/// 检测查询是否包含中文字符 / Check if query contains CJK characters
fn contains_cjk(s: &str) -> bool {
    s.chars()
        .any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c) || ('\u{3400}'..='\u{4dbf}').contains(&c))
}

/// 中文字符集补充的 SerpAPI 参数（地理 + 语言约束）
fn serpapi_cn_params(query: &str) -> Vec<(&'static str, String)> {
    if contains_cjk(query) {
        vec![("hl", "zh-cn".to_string()), ("gl", "cn".to_string())]
    } else {
        vec![]
    }
}

/// 计算查询哈希（用于缓存键）/ Compute query hash for cache key
fn query_hash(query: &str, source: &str) -> String {
    let mut h = DefaultHasher::new();
    query.hash(&mut h);
    source.hash(&mut h);
    format!("{:016x}", h.finish())
}

/// 尝试从缓存加载搜索结果 / Try loading search results from cache
fn try_cache(db: &Database, queries: &[String], source: &str) -> Option<Vec<PipelinePatentHit>> {
    let combined = queries.join("|");
    let hash = query_hash(&combined, source);
    if let Ok(Some(json)) = db.get_search_cache(&hash) {
        if let Ok(results) = serde_json::from_str::<Vec<PipelinePatentHit>>(&json) {
            tracing::info!("搜索缓存命中: {} ({} 条结果)", source, results.len());
            return Some(results);
        }
    }
    None
}

/// 写入搜索缓存 / Save search results to cache
fn save_cache(db: &Database, queries: &[String], source: &str, results: &[PipelinePatentHit]) {
    let combined = queries.join("|");
    let hash = query_hash(&combined, source);
    if let Ok(json) = serde_json::to_string(results) {
        let _ = db.set_search_cache(&hash, &combined, &json, source);
    }
}

/// 构建在线多源检索链 providers（与 routes/search.rs::online_chain_providers 同逻辑）
///
/// MC1：pipeline 检索步骤复用 M-A 多源降级链。
fn build_online_chain(
    db: &Arc<Database>,
    serpapi_key: Option<String>,
    epo_credentials: Option<(String, String)>,
) -> Vec<Arc<dyn SearchProvider>> {
    let mut providers: Vec<Arc<dyn SearchProvider>> = Vec::new();
    if let Some(api_key) = serpapi_key {
        providers.push(Arc::new(SerpApiProvider::new(api_key, db.clone())));
    }
    // 免费无 Key 的降级源，恒登记（MA2a）
    providers.push(Arc::new(GooglePatentsXhrProvider::new(db.clone())));
    if let Some((epo_key, epo_secret)) = epo_credentials {
        providers.push(Arc::new(EpoOpsProvider::new(
            epo_key,
            epo_secret,
            db.clone(),
        )));
    }
    providers
}

/// 将 PatentSummary 映射回 PipelinePatentHit
fn summary_to_hit(summary: &crate::types::search::PatentSummary, idx: usize) -> PipelinePatentHit {
    PipelinePatentHit {
        id: format!("online_{}", idx),
        title: summary.title.clone(),
        snippet: if summary.abstract_text.is_empty() {
            summary.applicant.clone()
        } else {
            summary.abstract_text.clone()
        },
        link: format!(
            "https://patents.google.com/patent/{}",
            summary.patent_number
        ),
        source: summary
            .score_source
            .clone()
            .unwrap_or_else(|| "online".into()),
    }
}

/// 执行 Step 3: 网络搜索（仅 SerpAPI，其他源已屏蔽）
pub async fn search_web(ctx: &mut PipelineContext, serpapi_key: &str, db: &Database) -> Result<()> {
    if serpapi_key.is_empty() || serpapi_key == "your-serpapi-key-here" {
        // SerpAPI 未配置，跳过网络搜索
        return Ok(());
    }

    // 缓存命中则直接返回 / Return cached results if available
    let queries: Vec<String> = ctx
        .expanded_queries
        .iter()
        .map(|q| super::parse::sanitize_search_query(q))
        .filter(|q| q.chars().count() >= 2)
        .take(3)
        .collect();
    let queries = if queries.is_empty() {
        let t = super::parse::sanitize_search_query(&ctx.title);
        if t.is_empty() {
            ctx.expanded_queries.iter().take(3).cloned().collect()
        } else {
            vec![t]
        }
    } else {
        queries
    };
    if let Some(cached) = try_cache(db, &queries, "web") {
        ctx.web_results = cached;
        return Ok(());
    }

    let client = Client::builder()
        .timeout(Duration::from_secs(SEARCH_UPSTREAM_TIMEOUT_SECS))
        .build()
        .expect("搜索步骤上游客户端构建失败——禁止静默退回无超时客户端，见 issue #55");
    let mut all_results = Vec::new();
    let mut seen_urls: HashSet<String> = HashSet::new();

    // SerpAPI 搜索
    for query in &queries {
        let mut params = vec![
            (
                "q",
                format!("{} site:patents.google.com OR technology OR patent", query),
            ),
            ("api_key", serpapi_key.to_string()),
            ("num", "10".to_string()),
        ];
        // 中文查询添加语言和地理参数
        for (k, v) in serpapi_cn_params(query) {
            params.push((k, v));
        }
        let resp = client
            .get("https://serpapi.com/search.json")
            .query(&params)
            .send()
            .await;

        if let Ok(resp) = resp {
            if let Ok(json) = resp.json::<serde_json::Value>().await {
                if let Some(results) = json["organic_results"].as_array() {
                    for r in results {
                        let link = r["link"].as_str().unwrap_or("").to_string();
                        if link.is_empty() || seen_urls.contains(&link) {
                            continue;
                        }
                        seen_urls.insert(link.clone());
                        all_results.push(PipelinePatentHit {
                            id: format!("web_{}", all_results.len()),
                            title: r["title"].as_str().unwrap_or("").to_string(),
                            snippet: r["snippet"].as_str().unwrap_or("").to_string(),
                            link,
                            source: "serpapi".into(),
                        });
                    }
                }
            }
        }
    }

    // 写入缓存 / Persist to cache
    if !all_results.is_empty() {
        save_cache(db, &queries, "web", &all_results);
    }

    ctx.web_results = all_results;
    Ok(())
}

/// 根据当前自动补搜轮次选择本轮要执行的专利查询子集：
/// - 初始轮（`auto_refill_rounds == 0`）：取前 3 条核心扩展查询；
/// - 补搜轮（`auto_refill_rounds > 0`）：取最新追加到尾部的至多 3 条补搜查询，
///   避免重跑前 3 条命中旧缓存原地空转。
fn select_patent_search_queries(ctx: &PipelineContext) -> Vec<String> {
    // 严格保持空查询语义（例如 quick_mode 跳过 ExpandQuery 时 expanded_queries 为空，不得私自触发在线检索，见 issue #70）
    if ctx.expanded_queries.is_empty() {
        return Vec::new();
    }
    let raw_slice: Vec<String> = if ctx.auto_refill_rounds > 0 && ctx.expanded_queries.len() > 3 {
        let skip = ctx.expanded_queries.len().saturating_sub(3);
        ctx.expanded_queries.iter().skip(skip).cloned().collect()
    } else {
        ctx.expanded_queries.iter().take(5).cloned().collect()
    };

    let mut clean: Vec<String> = Vec::new();
    for q in raw_slice {
        let s = super::parse::sanitize_search_query(&q);
        if s.chars().count() >= 2 && !clean.contains(&s) {
            clean.push(s);
            if clean.len() >= 3 {
                break;
            }
        }
    }
    if clean.is_empty() {
        let title_q = super::parse::sanitize_search_query(&ctx.title);
        if title_q.chars().count() >= 2 {
            clean.push(title_q);
        }
        for kw in &ctx.keywords {
            let s = super::parse::sanitize_search_query(kw);
            if s.chars().count() >= 2 && !clean.contains(&s) {
                clean.push(s);
                if clean.len() >= 3 {
                    break;
                }
            }
        }
    }
    clean
}

/// 将新检索命中的专利按标题去重合并到目标集合中，返回实际新增条数。
fn merge_unique_patent_hits(
    target: &mut Vec<PipelinePatentHit>,
    seen_titles: &mut HashSet<String>,
    incoming: Vec<PipelinePatentHit>,
) -> usize {
    let mut added = 0;
    for hit in incoming {
        let title_key = hit
            .title
            .chars()
            .take(20)
            .collect::<String>()
            .to_lowercase();
        if title_key.is_empty() || seen_titles.contains(&title_key) {
            continue;
        }
        seen_titles.insert(title_key);
        target.push(hit);
        added += 1;
    }
    added
}

/// 执行本地一阶段 FTS 检索（同步逻辑，供 `spawn_blocking` 与离线单测共用），返回命中列表及供二阶段雪球补漏的种子 ID/专利号。
fn collect_local_stage1_hits_sync(
    db: &Database,
    queries: &[String],
) -> (Vec<PipelinePatentHit>, Vec<String>, Vec<String>) {
    let mut hits = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut seed_ids: Vec<String> = Vec::new();
    let mut seed_numbers: Vec<String> = Vec::new();

    for query in queries {
        let mut local_queries = vec![query.clone()];
        let cjk_only: Vec<char> = query
            .chars()
            .filter(|c| (*c as u32) >= 0x4E00 && (*c as u32) <= 0x9FFF)
            .collect();
        if cjk_only.len() >= 4 {
            let head_stem: String = cjk_only.iter().take(cjk_only.len() - 1).collect();
            let mid_stem: String = cjk_only
                .iter()
                .skip(1)
                .take(cjk_only.len().saturating_sub(2))
                .collect();
            if head_stem.chars().count() >= 3 && !local_queries.contains(&head_stem) {
                local_queries.push(head_stem);
            }
            if mid_stem.chars().count() >= 3 && !local_queries.contains(&mid_stem) {
                local_queries.push(mid_stem);
            }
        }
        let mut hits_for_q = 0usize;
        for lq in &local_queries {
            if let Ok((local_results, _total, _detected)) =
                db.search_smart(lq, Some(&SearchType::Mixed), None, None, None, 1, 20)
            {
                for p in local_results {
                    let title_key = p.title.chars().take(20).collect::<String>().to_lowercase();
                    if seen.contains(&title_key) {
                        continue;
                    }
                    seen.insert(title_key);
                    if !p.id.trim().is_empty() && seed_ids.len() < 12 && !seed_ids.contains(&p.id) {
                        seed_ids.push(p.id.clone());
                    }
                    if !p.patent_number.trim().is_empty()
                        && seed_numbers.len() < 16
                        && !seed_numbers.contains(&p.patent_number)
                    {
                        seed_numbers.push(p.patent_number.clone());
                    }
                    hits.push(PipelinePatentHit {
                        id: format!("patent_local_{}", p.patent_number),
                        title: p.title,
                        snippet: p.abstract_text,
                        link: format!("https://patents.google.com/patent/{}", p.patent_number),
                        source: "local_db".into(),
                    });
                    hits_for_q += 1;
                }
            }
            if hits_for_q >= 5 {
                break;
            }
        }
    }

    (hits, seed_ids, seed_numbers)
}

/// 将一阶段种子专利送入二阶段引文/同族/同IPC三视角语义补漏引擎，返回去重后的增量对比专利（零 API 成本、零磁盘膨胀）。
fn collect_stage2_snowball_hits_sync(
    db: &Database,
    title: &str,
    queries: &[String],
    seed_ids: &[String],
    seed_numbers: &[String],
    limit: usize,
) -> (
    Vec<PipelinePatentHit>,
    crate::routes::SnowballExpansionResult,
) {
    let snowball_query = if let Some(first_q) = queries.first() {
        if title.trim().is_empty() || title.contains(first_q) {
            title.to_string()
        } else {
            format!("{} {}", title, first_q)
        }
    } else {
        title.to_string()
    };
    let snow = crate::routes::expand_citations_snowball_sync(
        db,
        &snowball_query,
        seed_ids,
        seed_numbers,
        limit,
    );
    let hits: Vec<PipelinePatentHit> = snow
        .patents
        .iter()
        .map(|hit| {
            let pn = if hit.patent_number.trim().is_empty() {
                hit.id.clone()
            } else {
                hit.patent_number.clone()
            };
            PipelinePatentHit {
                id: format!("patent_snowball_{}", pn),
                title: hit.title.clone(),
                snippet: format!("[{}] {}", hit.relation_label, hit.abstract_text),
                link: format!("https://patents.google.com/patent/{}", pn),
                source: "local_snowball".into(),
            }
        })
        .collect();
    (hits, snow)
}

/// 执行 Step 4: 专利搜索
///
/// MC1：改走 M-A 多源检索链（SourceChain），复用 SerpAPI + GooglePatentsXhr + EpoOps
/// 三源并行发起、按优先级择胜的降级链。本地 DB FTS 兜底保留，并在一阶段命中种子后自动触发
/// 二阶段引文/同族/同IPC三视角语义补漏（复用 `routes::search::expand_citations_snowball_sync`）。
pub async fn search_patents(
    ctx: &mut PipelineContext,
    serpapi_key: &str,
    db: &Arc<crate::db::Database>,
) -> Result<()> {
    let queries = select_patent_search_queries(ctx);
    if queries.is_empty() {
        return Ok(());
    }
    let is_refill = ctx.auto_refill_rounds > 0 && !ctx.patent_results.is_empty();

    let mut all_results = if is_refill {
        ctx.patent_results.clone()
    } else {
        Vec::new()
    };
    let mut seen_titles: HashSet<String> = all_results
        .iter()
        .map(|r| r.title.chars().take(20).collect::<String>().to_lowercase())
        .collect();

    // 缓存命中：初始轮直接返回；补搜轮增量合并去重，不覆盖已有结果
    if let Some(cached) = try_cache(db, &queries, "patent") {
        if is_refill {
            merge_unique_patent_hits(&mut all_results, &mut seen_titles, cached);
            ctx.patent_results = all_results;
        } else {
            ctx.patent_results = cached;
        }
        return Ok(());
    }

    let mut batch_results = Vec::new();
    let mut batch_seen: HashSet<String> = HashSet::new();

    // 一阶段本地数据库搜索（放入 spawn_blocking，防止在 30 万+ 本地库上阻塞 Tokio 工作线程；同时收集二阶段雪球种子）
    let db_local = db.clone();
    let local_queries = queries.clone();
    let (local_hits, seed_ids, mut seed_numbers) = tokio::task::spawn_blocking(move || {
        collect_local_stage1_hits_sync(&db_local, &local_queries)
    })
    .await
    .unwrap_or_default();
    merge_unique_patent_hits(&mut batch_results, &mut batch_seen, local_hits);

    // MC1: 在线多源检索链（SourceChain）
    let serpapi_opt = if !serpapi_key.is_empty() && serpapi_key != "your-serpapi-key-here" {
        Some(serpapi_key.to_string())
    } else {
        None
    };

    // EPO 凭证暂不从 pipeline 获取（pipeline 无 config 读取），仅用 SerpAPI + GooglePatentsXhr
    let providers = build_online_chain(db, serpapi_opt.clone(), None);

    if !providers.is_empty() {
        for query in queries.iter().take(2) {
            let lang = if contains_cjk(query) {
                Lang::Chinese
            } else {
                Lang::English
            };
            let search_query = SearchQuery {
                keyword: query.clone(),
                country: None,
                language: Some(lang),
                assignee: None,
                exact_assignee: false,
                date_from: None,
                date_to: None,
                limit: 20,
                page: 1,
                sort_by: None,
                search_type: Some(SearchType::Keyword),
            };

            let outcome = SourceChain::new(providers.clone()).run(search_query).await;
            let summaries = outcome.summaries();

            for summary in &summaries {
                if !summary.patent_number.trim().is_empty()
                    && seed_numbers.len() < 16
                    && !seed_numbers.contains(&summary.patent_number)
                {
                    seed_numbers.push(summary.patent_number.clone());
                }
                let title_key = summary
                    .title
                    .chars()
                    .take(20)
                    .collect::<String>()
                    .to_lowercase();
                if batch_seen.contains(&title_key) || summary.title.is_empty() {
                    continue;
                }
                batch_seen.insert(title_key);
                batch_results.push(summary_to_hit(
                    summary,
                    all_results.len() + batch_results.len(),
                ));
            }

            // 如果本批已有足够结果，不再多查
            if batch_results.len() >= 30 {
                break;
            }
        }
    }

    // 二阶段混合补漏：从本地 + 在线一阶段种子出发，沿引文网络 / 同族 / 同 IPC 语义邻域自动挖掘换表述隐蔽对比文件
    if (!seed_ids.is_empty() || !seed_numbers.is_empty()) && batch_results.len() < 30 {
        let snowball_limit = (30 - batch_results.len()).min(10);
        let db_snow = db.clone();
        let title_owned = ctx.title.clone();
        let queries_owned = queries.clone();
        if let Ok((snow_hits, snow_meta)) = tokio::task::spawn_blocking(move || {
            collect_stage2_snowball_hits_sync(
                &db_snow,
                &title_owned,
                &queries_owned,
                &seed_ids,
                &seed_numbers,
                snowball_limit,
            )
        })
        .await
        {
            let added = merge_unique_patent_hits(&mut batch_results, &mut batch_seen, snow_hits);
            if added > 0 {
                tracing::info!(
                    "[IDEA-SNOWBALL] seeds={} top_ipcs={:?} added={} (back={} fwd={} fam={} co_ipc={})",
                    snow_meta.seed_count,
                    snow_meta.top_ipcs,
                    added,
                    snow_meta.backward_citation,
                    snow_meta.forward_citation,
                    snow_meta.patent_family,
                    snow_meta.co_ipc_semantic,
                );
            }
        }
    }

    // 写入本批查询缓存 / Persist batch to cache
    if !batch_results.is_empty() {
        save_cache(db, &queries, "patent", &batch_results);
    }

    merge_unique_patent_hits(&mut all_results, &mut seen_titles, batch_results);
    ctx.patent_results = all_results;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_patent_search_queries_uses_tail_queries_during_auto_refill() {
        let mut ctx = PipelineContext::new("id-1", "微压氧舱", "描述");
        ctx.expanded_queries = vec![
            "微压氧舱".into(),
            "高压氧舱".into(),
            "氧气密封".into(),
            "气动自紧密封唇".into(),
            "多级迷宫微通道阻尼".into(),
        ];

        assert_eq!(
            select_patent_search_queries(&ctx),
            vec!["微压氧舱", "高压氧舱", "氧气密封"]
        );

        ctx.auto_refill_rounds = 1;
        assert_eq!(
            select_patent_search_queries(&ctx),
            vec!["氧气密封", "气动自紧密封唇", "多级迷宫微通道阻尼"]
        );
    }

    #[test]
    fn select_patent_search_queries_filters_gibberish_and_stopwords() {
        let mut ctx = PipelineContext::new(
            "id-2",
            "微高压氧仓，目前市场上头部品牌有哪些，分别优缺点有哪些",
            "描述",
        );
        ctx.expanded_queries = vec![
            "有哪".into(),
            "微高压氧仓，目前市场上头部品牌有哪些，分别优缺点有哪些".into(),
            "标题A与标题B在家用/用微/微高 vs 软体/体舱维度存在矛盾，需进一步验证".into(),
            "高压氧舱 密封结构".into(),
        ];
        assert_eq!(
            select_patent_search_queries(&ctx),
            vec!["微高压氧仓", "高压氧舱 密封结构"]
        );

        // quick_mode / 未执行 ExpandQuery 时 expanded_queries 为空，必须保持返回空集
        let empty_ctx = PipelineContext::new("id-3", "相变储热散热底座", "描述");
        assert!(select_patent_search_queries(&empty_ctx).is_empty());
    }

    #[test]
    fn local_stage1_plus_stage2_snowball_recalls_non_lexical_cited_and_co_ipc_patents() {
        let db = Database::init(":memory:").expect("init in-memory db");
        let seed = crate::patent::Patent {
            id: "idea-seed-1".into(),
            patent_number: "CN990701A".into(),
            title: "便携式医用高压氧舱智能供氧稳压系统".into(),
            abstract_text: "涉及医用加压供氧舱体结构、呼吸压力闭环调控与安全泄压阀。".into(),
            ipc_codes: "[\"A61G 10/02\"]".into(),
            citations: "[\"CN990702A\"]".into(),
            country: "CN".into(),
            filing_date: "2026-02-10".into(),
            ..Default::default()
        };
        let cited = crate::patent::Patent {
            id: "idea-cited-2".into(),
            patent_number: "CN990702A".into(),
            title: "高海拔密闭舱快锁气密门机构".into(),
            abstract_text: "一种用于密闭舱体快速锁紧与气密门防漏压的机械锁定结构。".into(),
            ipc_codes: "[\"E05B 65/00\"]".into(),
            country: "CN".into(),
            filing_date: "2024-06-15".into(),
            ..Default::default()
        };
        let co_ipc = crate::patent::Patent {
            id: "idea-coipc-3".into(),
            patent_number: "CN990703A".into(),
            title: "医用加压供氧治疗室双级泄压安全阀".into(),
            abstract_text: "针对医用加压供氧治疗舱内呼吸压力闭环调控设计的双级安全阀。".into(),
            ipc_codes: "[\"A61G 10/02\"]".into(),
            country: "CN".into(),
            filing_date: "2025-10-20".into(),
            ..Default::default()
        };
        let noise = crate::patent::Patent {
            id: "idea-noise-4".into(),
            patent_number: "CN990704A".into(),
            title: "折叠屏终端双轴阻尼铰链".into(),
            abstract_text: "移动终端柔性屏幕折叠转轴机械限位机构。".into(),
            ipc_codes: "[\"H04M 1/02\"]".into(),
            country: "CN".into(),
            filing_date: "2025-08-01".into(),
            ..Default::default()
        };
        for p in [&seed, &cited, &co_ipc, &noise] {
            db.insert_patent(p).expect("insert fixture");
        }

        let queries = vec!["高压氧舱".to_string()];
        let (stage1_hits, seed_ids, seed_numbers) = collect_local_stage1_hits_sync(&db, &queries);
        assert_eq!(stage1_hits.len(), 1, "一阶段词面检索应仅命中种子专利本身");
        assert_eq!(stage1_hits[0].id, "patent_local_CN990701A");

        let mut merged = Vec::new();
        let mut seen = HashSet::new();
        merge_unique_patent_hits(&mut merged, &mut seen, stage1_hits);

        let (snow_hits, snow_meta) = collect_stage2_snowball_hits_sync(
            &db,
            "便携式医用高压氧舱智能供氧",
            &queries,
            &seed_ids,
            &seed_numbers,
            10,
        );
        let added = merge_unique_patent_hits(&mut merged, &mut seen, snow_hits);
        assert_eq!(added, 2, "二阶段补漏应补充引证对比文件 + 同IPC语义邻居");
        assert_eq!(snow_meta.backward_citation, 1);
        assert_eq!(snow_meta.co_ipc_semantic, 1);

        let ids: Vec<&str> = merged.iter().map(|h| h.id.as_str()).collect();
        assert!(ids.contains(&"patent_snowball_CN990702A"));
        assert!(ids.contains(&"patent_snowball_CN990703A"));
        assert!(!ids.contains(&"patent_snowball_CN990704A"));
        assert!(
            merged
                .iter()
                .filter(|h| h.source == "local_snowball")
                .all(|h| h.snippet.starts_with('[')),
            "二阶段补漏摘要应前置关系标签，供下游新颖性分析透明追溯"
        );
    }
}
