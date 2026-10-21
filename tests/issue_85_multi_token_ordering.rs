//! Regression for issue #85: multi-token OR search must order by hit-token count,
//! not by filing_date DESC.
//!
//! Before fix: `search_like` built OR LIKE across title/abstract_text/applicant/
//! inventor/patent_number but ordered by `filing_date DESC` — a 1-token hit with
//! a newer date ranked above a 2-token hit with an older date, contradicting the
//! documented "relevance" ordering.
//!
//! After fix: ORDER BY (sum of per-token CASE WHEN hit THEN 1 ELSE 0 END) DESC,
//! filing_date DESC. The hit-count expression reuses the SAME `FIELDS` constant
//! as the WHERE clause, so the sort column set and filter column set can never
//! diverge.
//!
//! All fixtures use synthetic 99/90-style numbers (CN9900xxxxxA). No real case
//! data appears anywhere in this file.

use innoforge::db::Database;
use innoforge::patent::Patent;
use innoforge::types::search::PatentSummary;

/// Build a synthetic patent with full control over text fields.
fn synth_patent(
    id: &str,
    patent_number: &str,
    title: &str,
    abstract_text: &str,
    applicant: &str,
    inventor: &str,
    filing_date: &str,
) -> Patent {
    Patent {
        id: id.to_string(),
        patent_number: patent_number.to_string(),
        title: title.to_string(),
        abstract_text: abstract_text.to_string(),
        description: String::new(),
        claims: String::new(),
        applicant: applicant.to_string(),
        inventor: inventor.to_string(),
        filing_date: filing_date.to_string(),
        publication_date: filing_date.to_string(),
        grant_date: None,
        ipc_codes: String::new(),
        cpc_codes: String::new(),
        priority_date: filing_date.to_string(),
        country: "CN".to_string(),
        kind_code: "A".to_string(),
        family_id: None,
        legal_status: "pending".to_string(),
        citations: "[]".to_string(),
        cited_by: "[]".to_string(),
        source: "test".to_string(),
        raw_json: "{}".to_string(),
        created_at: "2026-01-01T00:00:00Z".to_string(),
        images: "[]".to_string(),
        pdf_url: String::new(),
    }
}

/// Count how many query tokens hit at least one field of a result row.
///
/// This mirrors the SQL `CASE WHEN (field LIKE ?N OR ...) THEN 1 ELSE 0 END` sum
/// that `search_like` uses for ORDER BY. It is the **verification oracle** for
/// ordering assertions — we compute the expected hit count independently and
/// compare it against the actual result order.
fn token_hit_count(query: &str, row: &PatentSummary) -> usize {
    let tokens: Vec<&str> = query.split_whitespace().filter(|s| !s.is_empty()).collect();
    let fields = [
        &row.title,
        &row.abstract_text,
        &row.applicant,
        &row.inventor,
        &row.patent_number,
    ];
    tokens
        .iter()
        .filter(|token| {
            let tok_lower = token.to_lowercase();
            fields
                .iter()
                .any(|field| field.to_lowercase().contains(&tok_lower))
        })
        .count()
}

// ────────────────────────────────────────────────────────────────────────────
//  Positive: 2-token hit must rank above 1-token hit despite older date
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn issue85_two_token_hit_ranks_above_one_token_hit() {
    let db = Database::init(":memory:").expect("init db");

    // OLD patent: title contains BOTH "alpha" and "beta" → 2 token hits.
    // Filing date is OLDER to prove ordering is by hit count, not date.
    let old_2hit = synth_patent(
        "old2hit",
        "CN990001001A",
        "alpha beta marker",
        "abstract gamma",
        "Gamma Corp",
        "Gamma Inventor",
        "2024-01-10",
    );
    // NEW patent: title contains only "alpha" → 1 token hit.
    // Filing date is NEWER — under old `filing_date DESC` this would rank first.
    let new_1hit = synth_patent(
        "new1hit",
        "CN990002002A",
        "alpha delta marker",
        "abstract epsilon",
        "Epsilon Corp",
        "Epsilon Inventor",
        "2026-01-20",
    );
    db.insert_patent(&old_2hit).expect("insert old 2-hit");
    db.insert_patent(&new_1hit).expect("insert new 1-hit");

    let query = "alpha beta";
    let (rows, total) = db
        .search_like(query, None, None, None, 1, 10)
        .expect("search succeeds");

    assert_eq!(total, 2, "both patents must match with OR semantics");
    assert_eq!(rows.len(), 2);

    // The assertion that nails ordering: row[0] must be the 2-hit patent,
    // row[1] must be the 1-hit patent. If ORDER BY were bare filing_date DESC,
    // the newer 1-hit patent would come first and this would fail.
    let hits_0 = token_hit_count(query, &rows[0]);
    let hits_1 = token_hit_count(query, &rows[1]);
    assert!(
        hits_0 > hits_1,
        "first result must have more token hits than second: \
         got hits[0]={hits_0} (id={}), hits[1]={hits_1} (id={}) — \
         ordering is not by hit count",
        rows[0].id,
        rows[1].id,
    );
    assert_eq!(
        rows[0].id, "old2hit",
        "the 2-token-hit patent must rank first despite older filing_date"
    );
    assert_eq!(
        rows[1].id, "new1hit",
        "the 1-token-hit patent must rank second despite newer filing_date"
    );
}

// ────────────────────────────────────────────────────────────────────────────
//  Cross-page: global ordering, not page-internal reordering
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn issue85_cross_page_global_ordering() {
    let db = Database::init(":memory:").expect("init db");

    // Create 4 patents with descending hit counts and ascending filing dates.
    // If ordering were page-internal only, page 2's first item could have a
    // higher hit count than page 1's last item.
    //
    //   patent  hits  filing_date
    //   p3      3     2024-01-01   ← page 1, slot 1
    //   p2      2     2024-01-02   ← page 1, slot 2
    //   p1      1     2024-01-03   ← page 2, slot 1
    //   p0      1     2024-01-04   ← page 2, slot 2  (same hits as p1, newer date)
    //
    // Query: "alpha beta gamma" (3 tokens)
    // p3 matches all 3, p2 matches 2, p1 and p0 each match 1.

    let p3 = synth_patent(
        "p3",
        "CN990003003A",
        "alpha beta gamma",
        "abstract p3",
        "Applicant P3",
        "Inventor P3",
        "2024-01-01",
    );
    let p2 = synth_patent(
        "p2",
        "CN990002002A",
        "alpha beta delta",
        "abstract p2",
        "Applicant P2",
        "Inventor P2",
        "2024-01-02",
    );
    let p1 = synth_patent(
        "p1",
        "CN990001001A",
        "alpha epsilon zeta",
        "abstract p1",
        "Applicant P1",
        "Inventor P1",
        "2024-01-03",
    );
    let p0 = synth_patent(
        "p0",
        "CN990004004A",
        "alpha eta theta",
        "abstract p0",
        "Applicant P0",
        "Inventor P0",
        "2024-01-04",
    );
    db.insert_patent(&p3).expect("insert p3");
    db.insert_patent(&p2).expect("insert p2");
    db.insert_patent(&p1).expect("insert p1");
    db.insert_patent(&p0).expect("insert p0");

    let query = "alpha beta gamma";
    let page_size = 2usize;

    let (page1, total) = db
        .search_like(query, None, None, None, 1, page_size)
        .expect("page 1 search");
    let (page2, _) = db
        .search_like(query, None, None, None, 2, page_size)
        .expect("page 2 search");

    assert_eq!(total, 4, "all 4 patents must match");
    assert_eq!(page1.len(), 2, "page 1 must have 2 results");
    assert_eq!(page2.len(), 2, "page 2 must have 2 results");

    // Cross-page invariant: page 2's first item must NOT have more token hits
    // than page 1's last item. This proves the sort is global (SQL ORDER BY +
    // LIMIT/OFFSET), not a per-page reordering.
    let page1_last_hits = token_hit_count(query, &page1[1]);
    let page2_first_hits = token_hit_count(query, &page2[0]);
    assert!(
        page2_first_hits <= page1_last_hits,
        "cross-page ordering violated: page1 last hit_count={page1_last_hits} \
         (id={}), page2 first hit_count={page2_first_hits} (id={}) — \
         sort may be page-internal, not global",
        page1[1].id,
        page2[0].id,
    );

    // Verify exact expected order: p3(3 hits) -> p2(2 hits) -> p0(1 hit, newer date) -> p1(1 hit, older date)
    assert_eq!(page1[0].id, "p3", "page 1 slot 1 must be p3 (3 hits)");
    assert_eq!(page1[1].id, "p2", "page 1 slot 2 must be p2 (2 hits)");
    assert_eq!(
        page2[0].id, "p0",
        "page 2 slot 1 must be p0 (1 hit, newer date -> tiebreaker)"
    );
    assert_eq!(
        page2[1].id, "p1",
        "page 2 slot 2 must be p1 (1 hit, older date -> tiebreaker)"
    );
}

// ────────────────────────────────────────────────────────────────────────────
//  Column-set consistency: patent_number-only match must be counted in ORDER BY
// ────────────────────────────────────────────────────────────────────────────

/// If ORDER BY's hit-count expression used a different (smaller) column set than
/// WHERE's OR LIKE, a patent matching ONLY in a dropped column would be in the
/// result set (WHERE includes it) but get 0 hit count in ORDER BY — sorting it
/// last regardless of filing_date.
///
/// This test constructs exactly that scenario for `patent_number` (the last
/// field in FIELDS, most likely to be accidentally dropped from a hardcoded
/// ORDER BY): Patent A matches the search token ONLY in patent_number, Patent B
/// matches ONLY in title. Both have 1 hit. Patent A has a NEWER filing_date.
/// With correct ORDER BY (both 1 hit), A ranks first. If ORDER BY misses
/// patent_number, A gets 0 hits and ranks after B — the assertion fails.
#[test]
fn issue85_order_by_column_set_matches_where_column_set() {
    let db = Database::init(":memory:").expect("init db");

    // Patent A: "990077001" appears ONLY in patent_number, nowhere else.
    // Newer filing_date so that with equal hit count it ranks first.
    let patent_a = synth_patent(
        "patent_a",
        "CN990077001A",
        "marker gamma",
        "abstract gamma",
        "Gamma Corp",
        "Gamma Inventor",
        "2026-01-20",
    );
    // Patent B: "990077001" appears ONLY in title, nowhere else.
    // Older filing_date so that with equal hit count it ranks second.
    let patent_b = synth_patent(
        "patent_b",
        "CN990088002A",
        "990077001 delta",
        "abstract delta",
        "Delta Corp",
        "Delta Inventor",
        "2024-01-10",
    );
    db.insert_patent(&patent_a).expect("insert patent_a");
    db.insert_patent(&patent_b).expect("insert patent_b");

    let query = "990077001";
    let (rows, total) = db
        .search_like(query, None, None, None, 1, 10)
        .expect("search succeeds");

    assert_eq!(
        total, 2,
        "both patents must match (WHERE includes both fields)"
    );
    assert_eq!(rows.len(), 2);

    // Both should have exactly 1 token hit.
    let hits_a = token_hit_count(query, &rows[0]);
    let hits_b = token_hit_count(query, &rows[1]);
    assert_eq!(
        hits_a, 1,
        "first result must have 1 hit (patent_number-only match counts)"
    );
    assert_eq!(hits_b, 1, "second result must have 1 hit");

    // With correct ORDER BY: both have 1 hit, so filing_date DESC breaks the tie.
    // Patent A (2026-01-20, newer) must come first.
    // If ORDER BY misses patent_number: A gets 0 hits, B gets 1 hit, B comes first.
    assert_eq!(
        rows[0].id, "patent_a",
        "patent matching only in patent_number must be counted in ORDER BY \
         (same column set as WHERE). If this fails, ORDER BY is missing \
         patent_number from its hit-count expression."
    );
    assert_eq!(
        rows[1].id, "patent_b",
        "title-only match must rank second (older date, same hit count)"
    );
}

/// Companion to the patent_number test: verifies `abstract_text` is also counted
/// in ORDER BY. A patent matching ONLY in abstract_text must get a non-zero hit
/// count in the sort expression.
#[test]
fn issue85_abstract_text_only_match_counted_in_order_by() {
    let db = Database::init(":memory:").expect("init db");

    let patent_a = synth_patent(
        "patent_a",
        "CN990003003A",
        "marker gamma",
        "990077003 abstract",
        "Gamma Corp",
        "Gamma Inventor",
        "2026-01-20",
    );
    let patent_b = synth_patent(
        "patent_b",
        "CN990088004A",
        "990077003 delta",
        "abstract delta",
        "Delta Corp",
        "Delta Inventor",
        "2024-01-10",
    );
    db.insert_patent(&patent_a).expect("insert patent_a");
    db.insert_patent(&patent_b).expect("insert patent_b");

    let query = "990077003";
    let (rows, total) = db
        .search_like(query, None, None, None, 1, 10)
        .expect("search succeeds");

    assert_eq!(total, 2);
    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows[0].id, "patent_a",
        "abstract_text-only match must be counted in ORDER BY. \
         If this fails, ORDER BY is missing abstract_text."
    );
}

// ────────────────────────────────────────────────────────────────────────────
//  Tiebreaker: equal hit counts fall back to filing_date DESC
// ────────────────────────────────────────────────────────────────────────────

#[test]
fn issue85_equal_hit_count_tiebreaker_is_filing_date_desc() {
    let db = Database::init(":memory:").expect("init db");

    // Both patents match "alpha" in title (1 hit each), but have different dates.
    let older = synth_patent(
        "older",
        "CN990001001A",
        "alpha marker one",
        "abstract one",
        "Applicant One",
        "Inventor One",
        "2024-01-10",
    );
    let newer = synth_patent(
        "newer",
        "CN990002002A",
        "alpha marker two",
        "abstract two",
        "Applicant Two",
        "Inventor Two",
        "2026-01-20",
    );
    db.insert_patent(&older).expect("insert older");
    db.insert_patent(&newer).expect("insert newer");

    let query = "alpha";
    let (rows, total) = db
        .search_like(query, None, None, None, 1, 10)
        .expect("search succeeds");

    assert_eq!(total, 2);
    assert_eq!(rows.len(), 2);
    // Equal hit count (1 each) → tiebreaker filing_date DESC → newer first.
    assert_eq!(
        rows[0].id, "newer",
        "equal hit counts must fall back to filing_date DESC"
    );
    assert_eq!(rows[1].id, "older");
}
