//! T-203 四维全域情报检索与真实 DOI 核验内核集成测试
//!
//! 使用 `tokio::net::TcpListener::bind("127.0.0.1:0")` 启动本地轻量 mock HTTP 服务，
//! 覆盖四维并发检索、OpenAlex/Crossref 同 DOI 归一化去重、伪造 DOI 拦截、以及
//! 连续失败 3 次触发熔断降级且不拖垮其他圈层。

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde_json::json;

use innoforge::core::omni_search::{
    normalize_and_verify_doi, ArxivAdapter, CrossrefAdapter, OmniSearchService, OpenAlexAdapter,
    OpenSourceCodeAdapter, QueryPlanner, SemanticScholarAdapter, SerpApiPatentAdapter,
    TechStandardAdapter,
};
use innoforge::core::vault::models::{DocKind, SearchRealm};

#[derive(Clone)]
struct MockServerState {
    failing_hits: Arc<AtomicUsize>,
}

async fn start_mock_server() -> (String, MockServerState) {
    let state = MockServerState {
        failing_hits: Arc::new(AtomicUsize::new(0)),
    };
    let failing_counter = state.failing_hits.clone();

    let app = Router::new()
        .route(
            "/patent/search.json",
            get(|| async {
                Json(json!({
                    "organic_results": [
                        {
                            "publication_number": "CN117654321A",
                            "title": "一种固态电池复合电解质热管理系统",
                            "snippet": "公开了一种基于相变材料的固态电池热管理结构。",
                            "assignee": "某新能源科技有限公司",
                            "publication_date": "2024-03-01",
                            "patent_link": "https://patents.google.com/patent/CN117654321A"
                        }
                    ]
                }))
            }),
        )
        .route(
            "/openalex/works",
            get(|| async {
                Json(json!({
                    "results": [
                        {
                            "id": "https://openalex.org/W4390000001",
                            "doi": "https://doi.org/10.1038/S41586-024-07123-4",
                            "title": "Solid-state electrolyte thermal stability",
                            "publication_year": 2024,
                            "publication_date": "2024-05-15",
                            "type": "article",
                            "primary_location": {
                                "source": { "display_name": "Nature" }
                            },
                            "authorships": [
                                { "author": { "display_name": "Alice Zhang" } },
                                { "author": { "display_name": "Bob Li" } }
                            ],
                            "abstract_inverted_index": {
                                "Solid-state": [0],
                                "electrolyte": [1],
                                "improves": [2],
                                "thermal": [3],
                                "safety.": [4]
                            }
                        },
                        {
                            "id": "https://openalex.org/W4390000002",
                            "doi": "fake-doi-without-prefix",
                            "title": "Unverified DOI Paper Sample",
                            "publication_year": 2024,
                            "type": "article",
                            "authorships": []
                        }
                    ]
                }))
            }),
        )
        .route(
            "/crossref/works",
            get(|| async {
                Json(json!({
                    "message": {
                        "items": [
                            {
                                "DOI": "doi:10.1038/s41586-024-07123-4",
                                "title": ["Solid-state electrolyte thermal stability"],
                                "author": [
                                    { "given": "Alice", "family": "Zhang" },
                                    { "given": "Bob", "family": "Li" }
                                ],
                                "container-title": ["Nature"],
                                "published": { "date-parts": [[2024, 5, 15]] },
                                "type": "journal-article",
                                "abstract": "<jats:p>Comprehensive experimental validation of solid-state battery thermal management and phase change composite electrolyte.</jats:p>",
                                "URL": "https://doi.org/10.1038/s41586-024-07123-4"
                            }
                        ]
                    }
                }))
            }),
        )
        .route(
            "/arxiv/api/query",
            get(|| async {
                r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom" xmlns:arxiv="http://arxiv.org/schemas/atom">
  <entry>
    <id>http://arxiv.org/abs/2406.01234v1</id>
    <published>2024-06-03T00:00:00Z</published>
    <title>Thermal Modeling of Solid-State Battery Packs</title>
    <summary>We propose a coupled electro-thermal model for solid-state cells.</summary>
    <author><name>Carol Wang</name></author>
  </entry>
</feed>"#
            }),
        )
        .route(
            "/github/search/repositories",
            get(|| async {
                Json(json!({
                    "items": [
                        {
                            "full_name": "open-battery/solid-state-sim",
                            "description": "Open-source solid-state battery thermal simulator",
                            "html_url": "https://github.com/open-battery/solid-state-sim",
                            "stargazers_count": 420,
                            "updated_at": "2024-08-10T12:00:00Z",
                            "language": "Rust",
                            "license": { "spdx_id": "GPL-3.0" }
                        }
                    ]
                }))
            }),
        )
        .route(
            "/std/standards/search",
            get(|| async {
                Json(json!({
                    "items": [
                        {
                            "standard_id": "GB/T 31485-2015",
                            "title": "电动汽车用动力蓄电池安全要求及试验方法",
                            "abstract": "规定了动力蓄电池单体和模组的安全要求与热失控试验方法。",
                            "organization": "SAC",
                            "published_date": "2015-05-15",
                            "url": "https://std.samr.gov.cn/"
                        }
                    ]
                }))
            }),
        )
        .route(
            "/failing/{*rest}",
            get(move || {
                let counter = failing_counter.clone();
                async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                    (StatusCode::INTERNAL_SERVER_ERROR, "upstream error")
                }
            }),
        );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mock listener");
    let addr = listener.local_addr().expect("mock local addr");
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    (format!("http://{addr}"), state)
}

#[test]
fn test_normalize_and_verify_doi_accepts_valid_and_rejects_forged() {
    assert_eq!(
        normalize_and_verify_doi("https://doi.org/10.1038/S41586-024-07123-4"),
        Some("10.1038/s41586-024-07123-4".to_string())
    );
    assert_eq!(
        normalize_and_verify_doi("  DOI:10.1145/3544548.3581388  "),
        Some("10.1145/3544548.3581388".to_string())
    );
    assert_eq!(
        normalize_and_verify_doi("http://dx.doi.org/10.1109/TPAMI.2023.123456"),
        Some("10.1109/tpami.2023.123456".to_string())
    );

    // 伪造/非法 DOI 必须被拒绝
    assert_eq!(normalize_and_verify_doi(""), None);
    assert_eq!(normalize_and_verify_doi("not-a-doi"), None);
    assert_eq!(
        normalize_and_verify_doi("10.123/too-short-registrant"),
        None
    );
    assert_eq!(normalize_and_verify_doi("10.1234/contains space"), None);
    assert_eq!(normalize_and_verify_doi("10.1234/ctrl\nchar"), None);
    assert_eq!(normalize_and_verify_doi("10.1234/"), None);
}

#[tokio::test]
async fn test_four_realms_concurrent_dedup_and_circuit_breaker() {
    let (base, state) = start_mock_server().await;

    let patent_adapter = Arc::new(
        SerpApiPatentAdapter::with_base_url(format!("{base}/patent"), None)
            .expect("patent adapter"),
    );
    let openalex_adapter = Arc::new(
        OpenAlexAdapter::with_base_url(format!("{base}/openalex")).expect("openalex adapter"),
    );
    let crossref_adapter = Arc::new(
        CrossrefAdapter::with_base_url(format!("{base}/crossref")).expect("crossref adapter"),
    );
    let arxiv_adapter =
        Arc::new(ArxivAdapter::with_base_url(format!("{base}/arxiv")).expect("arxiv adapter"));
    let code_adapter = Arc::new(
        OpenSourceCodeAdapter::with_base_url(format!("{base}/github")).expect("github adapter"),
    );
    let std_adapter =
        Arc::new(TechStandardAdapter::with_base_url(format!("{base}/std")).expect("std adapter"));
    let failing_adapter = Arc::new(
        SemanticScholarAdapter::with_base_url(format!("{base}/failing")).expect("failing adapter"),
    );

    let service = OmniSearchService::with_breaker_config(
        vec![
            patent_adapter,
            openalex_adapter,
            crossref_adapter,
            arxiv_adapter,
            code_adapter,
            std_adapter,
            failing_adapter,
        ],
        3,
        Duration::from_secs(60),
    );

    let plan = QueryPlanner::plan_with_optional_rewrite(
        "固态电池 电解质 热管理",
        Some(|_p| async { Err(anyhow::anyhow!("simulated rewrite error")) }),
    )
    .await;
    assert!(!plan.used_ai_rewrite);
    assert!(!plan.patent_boolean_query.is_empty());
    assert!(!plan.scholar_query.is_empty());

    let all_realms = [
        SearchRealm::Patent,
        SearchRealm::Scholar,
        SearchRealm::CodeRepo,
        SearchRealm::Standard,
    ];

    // 第 1 次全圈层并发检索
    let outcome = service.search_across_realms(&plan, &all_realms, 10).await;

    // ① 验证四大圈层（含专利、期刊论文、预印本、开源工程、行业标准）一次性全部返回
    assert!(outcome
        .hits
        .iter()
        .any(|h| h.doc.doc_kind == DocKind::Patent));
    assert!(outcome
        .hits
        .iter()
        .any(|h| h.doc.doc_kind == DocKind::JournalPaper));
    assert!(outcome
        .hits
        .iter()
        .any(|h| h.doc.doc_kind == DocKind::Preprint));
    assert!(outcome
        .hits
        .iter()
        .any(|h| h.doc.doc_kind == DocKind::CodeRepo));
    assert!(outcome
        .hits
        .iter()
        .any(|h| h.doc.doc_kind == DocKind::Standard));

    // ② 验证 OpenAlex 与 Crossref 命中的同一 DOI 精准去重为 1 条，且固化了真实 DOI 与 BibTeX
    let merged_doi_hits: Vec<_> = outcome
        .hits
        .iter()
        .filter(|h| h.doc.external_id == "10.1038/s41586-024-07123-4")
        .collect();
    assert_eq!(merged_doi_hits.len(), 1);
    let merged = merged_doi_hits[0];
    assert!(merged.doc.doi_verified);
    assert!(merged.doc.bibtex_raw.contains("@article{"));
    assert!(merged.source_provider.contains("openalex"));
    assert!(merged.source_provider.contains("crossref"));
    // 保留 Crossref 更长摘要
    assert!(merged
        .doc
        .abstract_text
        .contains("Comprehensive experimental"));

    // ③ 验证非法 DOI 条目保持 doi_verified == false 且不生成 BibTeX
    let fake_hits: Vec<_> = outcome
        .hits
        .iter()
        .filter(|h| h.doc.title_en == "Unverified DOI Paper Sample")
        .collect();
    assert_eq!(fake_hits.len(), 1);
    assert!(!fake_hits[0].doc.doi_verified);
    assert!(fake_hits[0].doc.bibtex_raw.is_empty());

    // 验证开源仓库 GPL 传染性风险提示
    let code_hit = outcome
        .hits
        .iter()
        .find(|h| h.doc.doc_kind == DocKind::CodeRepo)
        .expect("code hit");
    assert!(code_hit.match_reason.contains("GPL"));

    // ④ 验证连续失败 3 次触发熔断器：第 2、3 次打到故障源，第 4 次被熔断拦截（不再发起 HTTP 请求）
    let _ = service.search_across_realms(&plan, &all_realms, 5).await;
    let _ = service.search_across_realms(&plan, &all_realms, 5).await;
    assert_eq!(state.failing_hits.load(Ordering::SeqCst), 3);

    let outcome_4 = service.search_across_realms(&plan, &all_realms, 5).await;
    // 第 4 次未再请求故障服务器，且其余圈层依然正常返回
    assert_eq!(state.failing_hits.load(Ordering::SeqCst), 3);
    assert!(outcome_4
        .diagnostics
        .iter()
        .any(|d| d.status == "circuit_open"));
    assert!(outcome_4
        .hits
        .iter()
        .any(|h| h.doc.doc_kind == DocKind::Patent));
    assert!(outcome_4
        .hits
        .iter()
        .any(|h| h.doc.doc_kind == DocKind::JournalPaper));
}
