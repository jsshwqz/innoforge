use super::Database;
use crate::patent::{Patent, SearchType};

fn sample_patent(id: &str, title: &str, filing_date: &str) -> Patent {
    Patent {
        id: id.to_string(),
        patent_number: format!("CN{}A", id.to_uppercase()),
        title: title.to_string(),
        abstract_text: format!("{title} abstract"),
        description: "description".to_string(),
        claims: "claim".to_string(),
        applicant: "Acme Corp".to_string(),
        inventor: "Alice Zhang".to_string(),
        filing_date: filing_date.to_string(),
        publication_date: filing_date.to_string(),
        grant_date: None,
        ipc_codes: "G06N".to_string(),
        cpc_codes: "G06N".to_string(),
        priority_date: filing_date.to_string(),
        country: "CN".to_string(),
        kind_code: "A".to_string(),
        family_id: None,
        legal_status: "pending".to_string(),
        citations: "[]".to_string(),
        cited_by: "[]".to_string(),
        source: "test".to_string(),
        raw_json: "{}".to_string(),
        created_at: "2026-03-07T00:00:00Z".to_string(),
        images: "[]".to_string(),
        pdf_url: String::new(),
    }
}

#[test]
fn keyword_search_without_filters_uses_fts_path() {
    let db = Database::init(":memory:").expect("init db");
    db.insert_patent(&sample_patent(
        "fts1",
        "Vector database patent",
        "2024-01-10",
    ))
    .expect("insert patent");

    let (rows, total, detected) = db
        .search_smart(
            "Vector",
            Some(&SearchType::Keyword),
            None,
            None,
            None,
            1,
            10,
        )
        .expect("search succeeds");

    assert_eq!(detected, SearchType::Keyword);
    assert_eq!(total, 1);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].title, "Vector database patent");
    // FTS results now have BM25-based relevance scores (30-100 range)
    assert!(rows[0].relevance_score.is_some());
    let score = rows[0].relevance_score.unwrap();
    assert!(
        (30.0..=100.0).contains(&score),
        "FTS score {} out of range",
        score
    );
}

#[test]
fn keyword_search_with_date_filter_uses_filtered_search() {
    let db = Database::init(":memory:").expect("init db");
    db.insert_patent(&sample_patent(
        "old1",
        "Vector database patent old",
        "2023-01-10",
    ))
    .expect("insert old patent");
    db.insert_patent(&sample_patent(
        "new1",
        "Vector database patent new",
        "2024-01-10",
    ))
    .expect("insert new patent");

    let (rows, total, detected) = db
        .search_smart(
            "Vector",
            Some(&SearchType::Keyword),
            None,
            Some("2024-01-01"),
            Some("2024-12-31"),
            1,
            10,
        )
        .expect("search succeeds");

    assert_eq!(detected, SearchType::Keyword);
    assert_eq!(total, 1);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].title, "Vector database patent new");
    assert!(rows[0].relevance_score.is_some());
}

#[test]
fn mixed_search_routes_patent_like_query_to_patent_number_search() {
    let db = Database::init(":memory:").expect("init db");
    let mut patent = sample_patent(
        "123456789",
        "Foldable hinge dustproof structure",
        "2026-01-10",
    );
    patent.patent_number = "CN 123456789 A".to_string();
    db.insert_patent(&patent).expect("insert patent");

    let (rows, total, detected) = db
        .search_smart(
            "CN123456789A",
            Some(&SearchType::Mixed),
            None,
            None,
            None,
            1,
            10,
        )
        .expect("search succeeds");

    assert_eq!(detected, SearchType::PatentNumber);
    assert_eq!(total, 1);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].patent_number, "CN 123456789 A");
}
