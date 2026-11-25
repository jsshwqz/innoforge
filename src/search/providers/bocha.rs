//! 博查国内直连 Web Search BYOK 源（`POST https://api.bochaai.com/v1/web-search`）
//!
//! ## 定位
//! - **零 VPN 国内直连**：当用户在设置页或环境变量中配置了 `BOCHA_API_KEY` 时启用，未配置时零外联请求；
//! - **诚实展示与专利号抽取**：
//!   - 在 `/api/search/online` 链路专利源 0 命中时作为国内直连 `web_results` 回退源；
//!   - 在 `/api/search/omni` 全域混搜中并行补充最新中文专利公开线索与中文技术情报。

use crate::search::model::{report_excerpt, FailKind, Lang, SearchQuery};
use crate::types::search::PatentSummary;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::OnceLock;
use std::time::Duration;

pub const BOCHA_SEARCH_ENDPOINT: &str = "https://api.bochaai.com/v1/web-search";
pub const BOCHA_RESULT_LIMIT: usize = 8;
const BOCHA_TIMEOUT_SECS: u64 = 12;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BochaWebHit {
    pub title: String,
    pub snippet: String,
    pub url: String,
    pub published_date: String,
    pub patent_number: Option<String>,
}

fn patent_num_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)\b((?:CN|US|EP|WO|JP|KR)\s*\d{6,13}\s*[A-Z]\d?)\b")
            .unwrap_or_else(|_| Regex::new("CN\\d+[A-Z]").unwrap())
    })
}

pub fn extract_patent_number_from_text(text: &str) -> Option<String> {
    patent_num_regex().captures(text).and_then(|caps| {
        caps.get(1).map(|m| {
            m.as_str()
                .chars()
                .filter(|c| !c.is_whitespace())
                .collect::<String>()
                .to_uppercase()
        })
    })
}

pub fn bocha_query_for(query: &SearchQuery) -> String {
    let kw = query.keyword.trim();
    if kw.is_empty() {
        return String::new();
    }
    match query.language {
        Some(Lang::English) => format!("{kw} patent"),
        _ => format!("{kw} 专利"),
    }
}

pub fn parse_bocha_response(body: &str) -> Result<Vec<BochaWebHit>, String> {
    let parsed: Value =
        serde_json::from_str(body).map_err(|e| format!("博查 JSON 解析失败: {e}"))?;
    // 兼容 {"code": 200, "data": {"webPages": {"value": [...]}}} 与顶层 {"webPages": {"value": [...]}}
    if let Some(code) = parsed.get("code").and_then(|v| v.as_i64()) {
        if code != 200 && code != 0 {
            let msg = parsed
                .get("msg")
                .or_else(|| parsed.get("message"))
                .and_then(|v| v.as_str())
                .unwrap_or("博查 API 返回错误");
            return Err(format!("Bocha code={code}: {msg}"));
        }
    }
    let items = parsed
        .pointer("/data/webPages/value")
        .or_else(|| parsed.pointer("/webPages/value"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut hits = Vec::new();
    for item in items {
        let title = item
            .get("name")
            .or_else(|| item.get("title"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let url = item
            .get("url")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if title.is_empty() || url.is_empty() {
            continue;
        }
        let summary = item
            .get("summary")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        let snippet_raw = item
            .get("snippet")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        let snippet = if summary.len() > snippet_raw.len() {
            summary.to_string()
        } else {
            snippet_raw.to_string()
        };
        let published_date = item
            .get("datePublished")
            .or_else(|| item.get("dateLastCrawled"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .chars()
            .take(10)
            .collect::<String>();
        let combined = format!("{title} {snippet} {url}");
        let patent_number = extract_patent_number_from_text(&combined);
        hits.push(BochaWebHit {
            title,
            snippet,
            url,
            published_date,
            patent_number,
        });
    }
    Ok(hits)
}

impl BochaWebHit {
    pub fn to_patent_summary(&self) -> PatentSummary {
        let pn = self.patent_number.clone().unwrap_or_default();
        let country = if pn.len() >= 2 {
            pn.chars().take(2).collect::<String>()
        } else {
            "CN".to_string()
        };
        PatentSummary {
            id: self.url.clone(),
            patent_number: pn,
            title: self.title.clone(),
            abstract_text: self.snippet.clone(),
            applicant: "博查国内联网检索".to_string(),
            inventor: String::new(),
            filing_date: self.published_date.clone(),
            country,
            relevance_score: Some(68.0),
            score_source: Some("bocha_web".to_string()),
            matched_fields: Some(vec!["bocha_web".to_string()]),
        }
    }
}

pub struct BochaSearchProvider {
    api_key: String,
    client: reqwest::Client,
}

impl BochaSearchProvider {
    pub fn new(api_key: String) -> Result<Self, String> {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(BOCHA_TIMEOUT_SECS))
            .build()
            .map_err(|e| format!("博查 HTTP 客户端初始化失败: {e}"))?;
        Ok(Self { api_key, client })
    }

    pub async fn web_search(
        &self,
        query: &SearchQuery,
    ) -> Result<Vec<BochaWebHit>, (FailKind, String)> {
        let q = bocha_query_for(query);
        if q.is_empty() {
            return Ok(Vec::new());
        }
        let limit = query.limit.clamp(1, BOCHA_RESULT_LIMIT);
        let payload = serde_json::json!({
            "query": q,
            "summary": true,
            "count": limit
        });

        let resp = self
            .client
            .post(BOCHA_SEARCH_ENDPOINT)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await
            .map_err(|e| (FailKind::Network, report_excerpt(&e.to_string())))?;

        let status = resp.status().as_u16();
        let body = resp
            .text()
            .await
            .map_err(|e| (FailKind::Network, report_excerpt(&e.to_string())))?;

        match status {
            200..=299 => parse_bocha_response(&body).map_err(|msg| (FailKind::Parse, msg)),
            401 | 403 => Err((
                FailKind::Auth,
                format!("博查 API Key 鉴权失败 (HTTP {status})"),
            )),
            429 => Err((
                FailKind::Quota,
                "博查 API 配额或频率受限 (HTTP 429)".to_string(),
            )),
            other => Err((
                FailKind::Network,
                format!("博查上游返回 HTTP {other}: {}", report_excerpt(&body)),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_bocha_response_extracts_cn_patent_number_and_fields() {
        let raw = r#"{
            "code": 200,
            "data": {
                "webPages": {
                    "value": [
                        {
                            "name": "CN118200456A 一种双面液冷SiC功率模块封装结构",
                            "url": "https://patents.example.cn/CN118200456A",
                            "snippet": "公开号 CN118200456A，公开了一种采用微通道铜冷板的封装方案。",
                            "summary": "公开号 CN118200456A，本发明公开了一种采用双面微通道铜冷板的800V碳化硅功率模块封装结构，显著降低热阻。",
                            "datePublished": "2026-09-12T00:00:00Z"
                        },
                        {
                            "name": "碳化硅封装散热技术行业综述",
                            "url": "https://tech.example.cn/sic-cooling",
                            "snippet": "对比银烧结与直接水冷路线。",
                            "datePublished": "2026-10-01"
                        }
                    ]
                }
            }
        }"#;
        let hits = parse_bocha_response(raw).expect("parse bocha ok");
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].patent_number.as_deref(), Some("CN118200456A"));
        assert_eq!(hits[0].published_date, "2026-09-12");
        assert!(hits[0].snippet.contains("800V碳化硅"));
        assert_eq!(hits[1].patent_number, None);

        let summary = hits[0].to_patent_summary();
        assert_eq!(summary.patent_number, "CN118200456A");
        assert_eq!(summary.country, "CN");
        assert_eq!(summary.score_source.as_deref(), Some("bocha_web"));
    }

    #[test]
    fn parse_bocha_error_code_returns_err() {
        let raw = r#"{"code": 401, "msg": "Invalid API Key"}"#;
        let err = parse_bocha_response(raw).expect_err("should fail on code 401");
        assert!(err.contains("401"));
    }
}
