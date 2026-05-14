//! Step 3-4: SearchWeb + SearchPatents
//!
//! 类型：CODE（HTTP 请求，不调用 LLM）
//!
//! 搜索仅使用 SerpAPI，其他搜索源（Bing/搜狗/Lens.org）已全部屏蔽。

use crate::db::Database;
use crate::pipeline::context::{PipelineContext, SearchResult};
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
fn try_cache(db: &Database, queries: &[String], source: &str) -> Option<Vec<SearchResult>> {
    let combined = queries.join("|");
    let hash = query_hash(&combined, source);
    if let Ok(Some(json)) = db.get_search_cache(&hash) {
        if let Ok(results) = serde_json::from_str::<Vec<SearchResult>>(&json) {
            tracing::info!("搜索缓存命中: {} ({} 条结果)", source, results.len());
            return Some(results);
        }
    }
    None
}

/// 写入搜索缓存 / Save search results to cache
fn save_cache(db: &Database, queries: &[String], source: &str, results: &[SearchResult]) {
    let combined = queries.join("|");
    let hash = query_hash(&combined, source);
    if let Ok(json) = serde_json::to_string(results) {
        let _ = db.set_search_cache(&hash, &combined, &json, source);
    }
}

/// 执行 Step 3: 网络搜索（仅 SerpAPI，其他源已屏蔽）
pub async fn search_web(ctx: &mut PipelineContext, serpapi_key: &str, db: &Database) -> Result<()> {
    if serpapi_key.is_empty() || serpapi_key == "your-serpapi-key-here" {
        // SerpAPI 未配置，跳过网络搜索
        return Ok(());
    }

    // 缓存命中则直接返回 / Return cached results if available
    let queries: Vec<String> = ctx.expanded_queries.iter().take(3).cloned().collect();
    if let Some(cached) = try_cache(db, &queries, "web") {
        ctx.web_results = cached;
        return Ok(());
    }

    let client = Client::builder()
        .timeout(Duration::from_secs(SEARCH_UPSTREAM_TIMEOUT_SECS))
        .build()
        .unwrap_or_else(|_| Client::new());
    let mut all_results = Vec::new();
    let mut seen_urls: HashSet<String> = HashSet::new();

    // SerpAPI 搜索
    for query in ctx.expanded_queries.iter().take(3) {
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
                        all_results.push(SearchResult {
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

/// 执行 Step 4: 专利搜索（仅 SerpAPI + 本地 DB，Lens.org 已屏蔽）
pub async fn search_patents(
    ctx: &mut PipelineContext,
    serpapi_key: &str,
    db: &Arc<crate::db::Database>,
) -> Result<()> {
    // 缓存命中则直接返回 / Return cached results if available
    let queries: Vec<String> = ctx.expanded_queries.iter().take(3).cloned().collect();
    if let Some(cached) = try_cache(db, &queries, "patent") {
        ctx.patent_results = cached;
        return Ok(());
    }

    let mut all_results = Vec::new();
    let mut seen_titles: HashSet<String> = HashSet::new();

    // 本地数据库搜索
    for query in ctx.expanded_queries.iter().take(3) {
        if let Ok((local_results, _total)) = db.search_fts(query, 1, 20) {
            for p in local_results {
                let title_key = p.title.chars().take(20).collect::<String>().to_lowercase();
                if seen_titles.contains(&title_key) {
                    continue;
                }
                seen_titles.insert(title_key);
                all_results.push(SearchResult {
                    id: format!("patent_local_{}", p.patent_number),
                    title: p.title,
                    snippet: p.abstract_text,
                    link: format!("https://patents.google.com/patent/{}", p.patent_number),
                    source: "local_db".into(),
                });
            }
        }
    }

    // SerpAPI Google Patents 搜索
    if !serpapi_key.is_empty() && serpapi_key != "your-serpapi-key-here" {
        let client = Client::builder()
            .timeout(Duration::from_secs(SEARCH_UPSTREAM_TIMEOUT_SECS))
            .build()
            .unwrap_or_else(|_| Client::new());
        for query in ctx.expanded_queries.iter().take(2) {
            let mut params = vec![
                ("engine", "google_patents".to_string()),
                ("q", query.clone()),
                ("api_key", serpapi_key.to_string()),
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
                            let title = r["title"].as_str().unwrap_or("").to_string();
                            let title_key =
                                title.chars().take(20).collect::<String>().to_lowercase();
                            if seen_titles.contains(&title_key) {
                                continue;
                            }
                            seen_titles.insert(title_key);
                            all_results.push(SearchResult {
                                id: format!("patent_online_{}", all_results.len()),
                                title,
                                snippet: r["snippet"].as_str().unwrap_or("").to_string(),
                                link: r["patent_id"]
                                    .as_str()
                                    .map(|id| format!("https://patents.google.com/patent/{}", id))
                                    .unwrap_or_default(),
                                source: "google_patents".into(),
                            });
                        }
                    }
                }
            }
        }
    }

    // 写入缓存 / Persist to cache
    if !all_results.is_empty() {
        save_cache(db, &queries, "patent", &all_results);
    }

    ctx.patent_results = all_results;
    Ok(())
}
