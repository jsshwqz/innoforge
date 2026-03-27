//! Step 3-4: SearchWeb + SearchPatents
//!
//! 类型：CODE（HTTP 请求，不调用 LLM）

use crate::pipeline::context::{PipelineContext, SearchResult};
use anyhow::Result;
use reqwest::Client;
use std::collections::HashSet;
use std::sync::Arc;

/// 执行 Step 3: 网络搜索
pub async fn search_web(ctx: &mut PipelineContext, serpapi_key: &str) -> Result<()> {
    if serpapi_key.is_empty() || serpapi_key == "your-serpapi-key-here" {
        // SerpAPI 未配置，跳过网络搜索
        return Ok(());
    }

    let client = Client::new();
    let mut all_results = Vec::new();
    let mut seen_urls: HashSet<String> = HashSet::new();

    // 用前 3 个扩展查询搜索
    for query in ctx.expanded_queries.iter().take(3) {
        let resp = client
            .get("https://serpapi.com/search.json")
            .query(&[
                ("q", format!("{} site:patents.google.com OR technology OR patent", query)),
                ("api_key", serpapi_key.to_string()),
                ("num", "10".to_string()),
            ])
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

    ctx.web_results = all_results;
    Ok(())
}

/// 执行 Step 4: 专利搜索
pub async fn search_patents(
    ctx: &mut PipelineContext,
    serpapi_key: &str,
    db: &Arc<crate::db::Database>,
) -> Result<()> {
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
        let client = Client::new();
        for query in ctx.expanded_queries.iter().take(2) {
            let resp = client
                .get("https://serpapi.com/search.json")
                .query(&[
                    ("engine", "google_patents".to_string()),
                    ("q", query.clone()),
                    ("api_key", serpapi_key.to_string()),
                ])
                .send()
                .await;

            if let Ok(resp) = resp {
                if let Ok(json) = resp.json::<serde_json::Value>().await {
                    if let Some(results) = json["organic_results"].as_array() {
                        for r in results {
                            let title = r["title"].as_str().unwrap_or("").to_string();
                            let title_key = title.chars().take(20).collect::<String>().to_lowercase();
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

    ctx.patent_results = all_results;
    Ok(())
}
