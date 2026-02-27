use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Patent {
    pub id: String, pub patent_number: String, pub title: String,
    pub abstract_text: String, pub description: String, pub claims: String,
    pub applicant: String, pub inventor: String, pub filing_date: String,
    pub publication_date: String, pub grant_date: Option<String>,
    pub ipc_codes: String, pub cpc_codes: String, pub priority_date: String,
    pub country: String, pub kind_code: String, pub family_id: Option<String>,
    pub legal_status: String, pub citations: String, pub cited_by: String,
    pub source: String, pub raw_json: String, pub created_at: String,
}

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
    #[serde(default = "d1")] pub page: usize,
    #[serde(default = "d20")] pub page_size: usize,
    pub country: Option<String>,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub search_type: Option<String>, // "applicant", "inventor", "keyword", "mixed"
    pub sort_by: Option<String>, // "relevance", "new", "old"
}

fn d1() -> usize { 1 }
fn d20() -> usize { 20 }

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchResult {
    pub patents: Vec<PatentSummary>,
    pub total: usize,
    pub page: usize,
    pub page_size: usize,
    pub search_type: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
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
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AiChatRequest { pub message: String, pub patent_id: Option<String> }

#[derive(Debug, Serialize, Deserialize)]
pub struct AiResponse { pub content: String }

#[derive(Debug, Serialize, Deserialize)]
pub struct FetchPatentRequest { pub patent_number: String, pub source: Option<String> }

#[derive(Debug, Serialize, Deserialize)]
pub struct ImportRequest { pub patents: Vec<Patent> }