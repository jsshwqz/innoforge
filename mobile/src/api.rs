use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::sync::LazyLock;

static SERVER_URL: LazyLock<Mutex<String>> = LazyLock::new(|| {
    Mutex::new("http://192.168.1.100:3000".to_string())
});

pub fn get_server_url() -> String {
    SERVER_URL.lock().unwrap().clone()
}

pub fn set_server_url(url: &str) {
    let mut s = SERVER_URL.lock().unwrap();
    *s = url.trim_end_matches('/').to_string();
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PatentSummary {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub patent_number: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub applicant: String,
    #[serde(default)]
    pub inventor: String,
    #[serde(default)]
    pub filing_date: String,
    #[serde(default)]
    pub country: String,
    #[serde(default)]
    pub abstract_text: String,
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    #[serde(default)]
    patents: Vec<PatentSummary>,
    #[serde(default)]
    total: usize,
}

pub async fn search_patents(
    server: &str,
    query: &str,
    page: u32,
    page_size: u32,
) -> Result<(Vec<PatentSummary>, usize), String> {
    let url = format!("{}/api/search", server);
    let body = serde_json::json!({
        "query": query,
        "search_type": "online",
        "page": page,
        "page_size": page_size,
    });

    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("网络错误: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("服务器错误: {}", resp.status()));
    }

    let data: SearchResponse = resp
        .json()
        .await
        .map_err(|e| format!("解析错误: {e}"))?;

    Ok((data.patents, data.total))
}
