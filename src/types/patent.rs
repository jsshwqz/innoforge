//! 专利核心类型 / Patent core domain types
//!
//! 自 `src/patent.rs` 迁出（T1.1 缩水版，types-migration-map.md §1「专利核心与检索族」）。
//! 字段、serde 属性与默认值函数逐字保持，仅更换定义位置。

use serde::{Deserialize, Serialize};

/// 专利数据 / Patent data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Patent {
    #[serde(default = "gen_id")]
    pub id: String,
    pub patent_number: String,
    pub title: String,
    #[serde(default)]
    pub abstract_text: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub claims: String,
    #[serde(default)]
    pub applicant: String,
    #[serde(default)]
    pub inventor: String,
    #[serde(default)]
    pub filing_date: String,
    #[serde(default)]
    pub publication_date: String,
    pub grant_date: Option<String>,
    #[serde(default)]
    pub ipc_codes: String,
    #[serde(default)]
    pub cpc_codes: String,
    #[serde(default)]
    pub priority_date: String,
    #[serde(default)]
    pub country: String,
    #[serde(default)]
    pub kind_code: String,
    pub family_id: Option<String>,
    #[serde(default)]
    pub legal_status: String,
    #[serde(default)]
    pub citations: String,
    #[serde(default)]
    pub cited_by: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub raw_json: String,
    #[serde(default = "now_str")]
    pub created_at: String,
    /// JSON array of image URLs (patent drawings)
    #[serde(default)]
    pub images: String,
    /// PDF download URL
    #[serde(default)]
    pub pdf_url: String,
}

fn gen_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn now_str() -> String {
    chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

/// Canonical patent key for deduplication.
/// Keeps `country_prefix + main_digits` when present, strips spaces/dots/kind code.
pub fn canonical_patent_key(raw: &str) -> String {
    let upper = raw.trim().to_uppercase();
    if upper.is_empty() {
        return String::new();
    }
    let clean: String = upper
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    if clean.is_empty() {
        return String::new();
    }

    let bytes = clean.as_bytes();
    let mut i = 0usize;
    while i + 2 < bytes.len() {
        if bytes[i].is_ascii_alphabetic() && bytes[i + 1].is_ascii_alphabetic() {
            let mut j = i + 2;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            if j - (i + 2) >= 6 {
                return clean[i..j].to_string();
            }
        }
        i += 1;
    }

    let digits: String = clean.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() >= 8 {
        return digits;
    }
    clean
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FetchPatentRequest {
    pub patent_number: String,
    pub source: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ImportRequest {
    pub patents: Vec<Patent>,
}

// ── Legal Status 法律状态 ────────────────────────────────────────────────────

/// 专利法律状态查询结果 / Patent legal status result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegalStatusResult {
    pub patent_number: String,
    /// "有效" | "无效" | "审查中" | "公开" | "驳回" | "撤回" | "未知"
    pub current_status: String,
    pub events: Vec<LegalEvent>,
    /// "google_patents" | "lens" | "cnipa_gazette" | "sogou"
    pub source: String,
    pub updated_at: String,
}

/// 单条法律状态事件 / A single legal status event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegalEvent {
    pub date: String,
    /// "公开" | "实审" | "授权" | "缴费" | "驳回" | "无效" | "转让" 等
    pub title: String,
    pub description: String,
}

#[cfg(test)]
mod tests {
    use super::canonical_patent_key;

    #[test]
    fn canonical_key_cn_with_spaces_and_kind() {
        assert_eq!(canonical_patent_key("CN 116401354 A"), "CN116401354");
    }

    #[test]
    fn canonical_key_us_with_kind_digit() {
        assert_eq!(canonical_patent_key("US1234567B2"), "US1234567");
    }

    #[test]
    fn canonical_key_google_patent_url_tail() {
        assert_eq!(
            canonical_patent_key("https://patents.google.com/patent/CN109876543A/zh"),
            "CN109876543"
        );
    }

    #[test]
    fn canonical_key_digits_fallback() {
        assert_eq!(canonical_patent_key("202310123456.7"), "2023101234567");
    }
}
