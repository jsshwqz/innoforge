//! T1.1 迁移不变式验证：serde 逐字段快照
//!
//! 这些断言的期望值是**在迁移之前**对着 `src/patent.rs` / `src/db/chat.rs` 的原始定义
//! 写就并跑绿的，迁移后原样复用即证明：类型搬家没有改变任何一个字段的名称、可省性或取值。
//!
//! 三重锁：
//! 1. **编译期**——构造字面量列出全部字段，任何字段增删改名都会直接编译失败；
//! 2. **键集合**——`json_field_names` 比对序列化后的字段名全集（含 `skip_serializing_if` 效果）；
//! 3. **字符串级**——`assert_exact_json` 对面向前端的响应类型逐字节比对（含键顺序与 null/缺省差异）。
//!
//! 说明：`serde_json::to_value` 会把对象落成 `BTreeMap`（本项目未开 `preserve_order`），
//! 键顺序证据只能从 `to_string` 取，故第 3 重锁用字符串比对。
//!
//! 路径一律走 `crate::patent` 公共门面（AGENTS.md §2.2），因此同一份测试在迁移前后都可编译。

use crate::patent::{
    AiChatRequest, AiResponse, CategoryGroup, ChatMessage, Evidence, FetchPatentRequest, Idea,
    IdeaChatRequest, IdeaSubmitRequest, IdeaSummary, ImportRequest, LegalEvent, LegalStatusResult,
    Patent, PatentSummary, ReferenceChunk, ResearchState, SearchRequest, SearchResult, SearchType,
    TextAttachment,
};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::BTreeSet;

/// 序列化后的顶层字段名全集（顺序无关，见模块注释）。
fn json_field_names<T: Serialize>(value: &T) -> BTreeSet<String> {
    match serde_json::to_value(value) {
        Ok(Value::Object(map)) => map.keys().cloned().collect(),
        Ok(other) => panic!("snapshot expected a JSON object, got {other}"),
        Err(e) => panic!("snapshot must serialize: {e}"),
    }
}

/// 期望字段名集合（写成声明顺序便于阅读，比较时排序）。
fn expected_fields(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(|s| (*s).to_string()).collect()
}

/// 反序列化 → 再序列化必须逐字节相同（证明 default / skip_serializing_if 未被动过）。
fn assert_roundtrip_stable<T: Serialize + serde::de::DeserializeOwned>(value: &T) {
    let before = serde_json::to_string(value).expect("snapshot must serialize");
    let parsed: T = serde_json::from_str(&before).expect("snapshot must round-trip deserialize");
    let after = serde_json::to_string(&parsed).expect("reserialized snapshot must serialize");
    assert_eq!(before, after, "serde round-trip changed the payload");
}

/// 逐字节钉住序列化结果（键顺序 + null vs 缺省 + 数值写法）。
fn assert_exact_json<T: Serialize>(value: &T, expected: &str) {
    let actual = serde_json::to_string(value).expect("snapshot must serialize");
    assert_eq!(expected, actual, "serialized payload changed");
}

fn sample_summary() -> PatentSummary {
    PatentSummary {
        id: "pid-1".into(),
        patent_number: "CN116401354A".into(),
        title: "一种固态电池".into(),
        abstract_text: "摘要正文".into(),
        applicant: "某科技有限公司".into(),
        inventor: "张三".into(),
        filing_date: "2023-01-02".into(),
        country: "CN".into(),
        relevance_score: Some(88.5),
        score_source: Some("hybrid(pos:95+content:84)".into()),
    }
}

fn sample_patent() -> Patent {
    Patent {
        id: "pt-1".into(),
        patent_number: "CN116401354A".into(),
        title: "一种固态电池".into(),
        abstract_text: "摘要".into(),
        description: "说明书".into(),
        claims: "权利要求 1".into(),
        applicant: "某科技有限公司".into(),
        inventor: "张三".into(),
        filing_date: "2023-01-02".into(),
        publication_date: "2023-06-09".into(),
        grant_date: Some("2024-02-01".into()),
        ipc_codes: "H01M10/02".into(),
        cpc_codes: "Y02E60/10".into(),
        priority_date: "2023-01-02".into(),
        country: "CN".into(),
        kind_code: "A".into(),
        family_id: Some("fam-1".into()),
        legal_status: "有效".into(),
        citations: "[]".into(),
        cited_by: "[]".into(),
        source: "serpapi".into(),
        raw_json: "{\"k\":1}".into(),
        created_at: "2026-01-01 00:00:00".into(),
        images: "[]".into(),
        pdf_url: "https://example.com/a.pdf".into(),
    }
}

// ── 检索族 ───────────────────────────────────────────────────────────────────

#[test]
fn patent_summary_field_snapshot_is_unchanged() {
    let s = sample_summary();
    assert_eq!(
        expected_fields(&[
            "id",
            "patent_number",
            "title",
            "abstract_text",
            "applicant",
            "inventor",
            "filing_date",
            "country",
            "relevance_score",
            "score_source",
        ]),
        json_field_names(&s)
    );
    // 前端逐字段读取这条记录：顺序与取值一并钉住
    assert_exact_json(
        &s,
        r#"{"id":"pid-1","patent_number":"CN116401354A","title":"一种固态电池","abstract_text":"摘要正文","applicant":"某科技有限公司","inventor":"张三","filing_date":"2023-01-02","country":"CN","relevance_score":88.5,"score_source":"hybrid(pos:95+content:84)"}"#,
    );
    assert_roundtrip_stable(&s);
}

#[test]
fn patent_summary_optional_scores_stay_serialized_as_null() {
    let mut s = sample_summary();
    s.relevance_score = None;
    s.score_source = None;
    // `#[serde(default)]` 而非 `skip_serializing_if`：None 仍出现在 JSON 中（值为 null）
    assert_exact_json(
        &s,
        r#"{"id":"pid-1","patent_number":"CN116401354A","title":"一种固态电池","abstract_text":"摘要正文","applicant":"某科技有限公司","inventor":"张三","filing_date":"2023-01-02","country":"CN","relevance_score":null,"score_source":null}"#,
    );
    assert_roundtrip_stable(&s);
}

#[test]
fn search_request_field_snapshot_is_unchanged() {
    let req = SearchRequest {
        query: "固态电池".into(),
        page: 2,
        page_size: 20,
        country: Some("CN".into()),
        date_from: Some("2023-01-01".into()),
        date_to: Some("2024-01-01".into()),
        search_type: Some("keyword".into()),
        sort_by: Some("new".into()),
        ipc: Some("H01M".into()),
        cpc: Some("Y02E".into()),
        region: Some("cn".into()),
    };
    assert_eq!(
        expected_fields(&[
            "query",
            "page",
            "page_size",
            "country",
            "date_from",
            "date_to",
            "search_type",
            "sort_by",
            "ipc",
            "cpc",
            "region",
        ]),
        json_field_names(&req)
    );
    assert_roundtrip_stable(&req);
}

#[test]
fn search_request_keeps_page_defaults_when_fields_absent() {
    // 迁移前 `page`/`page_size` 走 d1()/d20() 默认值，缺字段时必须仍为 1/20
    let req: SearchRequest = serde_json::from_str(r#"{"query":"x"}"#).expect("minimal request");
    assert_eq!(1, req.page);
    assert_eq!(20, req.page_size);
    // ipc/cpc/region 带 #[serde(default)]：缺字段解析为 None 而非报错
    assert_eq!(None, req.ipc);
    assert_eq!(None, req.cpc);
    assert_eq!(None, req.region);
}

#[test]
fn search_result_field_snapshot_is_unchanged_with_and_without_categories() {
    let with = SearchResult {
        patents: vec![sample_summary()],
        total: 12,
        page: 1,
        page_size: 20,
        search_type: Some("keyword".into()),
        categories: Some(vec![CategoryGroup {
            label: "申请人: 某科技有限公司".into(),
            count: 4,
        }]),
        dedup_removed: 3,
    };
    assert_eq!(
        expected_fields(&[
            "patents",
            "total",
            "page",
            "page_size",
            "search_type",
            "categories",
            "dedup_removed",
        ]),
        json_field_names(&with)
    );
    let without = SearchResult {
        patents: vec![],
        total: 0,
        page: 1,
        page_size: 20,
        search_type: Some("mixed".into()),
        categories: None,
        dedup_removed: 0,
    };
    // `categories` 带 skip_serializing_if：None 时键必须消失
    assert_eq!(
        expected_fields(&[
            "patents",
            "total",
            "page",
            "page_size",
            "search_type",
            "dedup_removed"
        ]),
        json_field_names(&without)
    );
    assert_exact_json(
        &without,
        r#"{"patents":[],"total":0,"page":1,"page_size":20,"search_type":"mixed","dedup_removed":0}"#,
    );
    assert_roundtrip_stable(&with);
    assert_roundtrip_stable(&without);
}

#[test]
fn category_group_field_snapshot_is_unchanged() {
    let g = CategoryGroup {
        label: "国家: CN".into(),
        count: 7,
    };
    assert_eq!(expected_fields(&["label", "count"]), json_field_names(&g));
    assert_exact_json(&g, r#"{"label":"国家: CN","count":7}"#);
}

#[test]
fn search_type_serializes_with_variant_names_unchanged() {
    // 无 rename_all：迁移前是 PascalCase 变体名，迁移后必须仍是
    assert_eq!(
        serde_json::to_value(SearchType::PatentNumber).unwrap(),
        json!("PatentNumber")
    );
    for name in ["Applicant", "Inventor", "PatentNumber", "Keyword", "Mixed"] {
        let parsed: SearchType =
            serde_json::from_str(&json!(name).to_string()).expect("legacy SearchType json");
        assert_eq!(
            serde_json::to_value(&parsed).unwrap(),
            json!(name),
            "SearchType variant `{name}` changed shape"
        );
    }
}

// ── 专利核心族 ───────────────────────────────────────────────────────────────

#[test]
fn patent_field_snapshot_is_unchanged() {
    let p = sample_patent();
    assert_eq!(
        expected_fields(&[
            "id",
            "patent_number",
            "title",
            "abstract_text",
            "description",
            "claims",
            "applicant",
            "inventor",
            "filing_date",
            "publication_date",
            "grant_date",
            "ipc_codes",
            "cpc_codes",
            "priority_date",
            "country",
            "kind_code",
            "family_id",
            "legal_status",
            "citations",
            "cited_by",
            "source",
            "raw_json",
            "created_at",
            "images",
            "pdf_url",
        ]),
        json_field_names(&p)
    );
    assert_roundtrip_stable(&p);
}

#[test]
fn patent_optional_and_defaulted_fields_survive_minimal_json() {
    // 迁移前：id/created_at 有 default 函数，其余非 Option 字段有 #[serde(default)]
    let p: Patent =
        serde_json::from_str(r#"{"patent_number":"CN1A","title":"T"}"#).expect("minimal patent");
    assert!(!p.id.is_empty(), "gen_id() default must still fill id");
    assert!(
        !p.created_at.is_empty(),
        "now_str() default must still fill created_at"
    );
    assert!(p.grant_date.is_none());
    assert!(p.family_id.is_none());
    // String::default() —— 若有人把 #[serde(default)] 改成 default="[]"，此处即红
    assert_eq!("", p.citations);
    assert_eq!("", p.images);
    assert_eq!("", p.pdf_url);
}

#[test]
fn legal_status_and_fetch_import_request_snapshots_are_unchanged() {
    let legal = LegalStatusResult {
        patent_number: "CN116401354A".into(),
        current_status: "有效".into(),
        events: vec![LegalEvent {
            date: "2024-02-01".into(),
            title: "授权".into(),
            description: "专利授权".into(),
        }],
        source: "google_patents".into(),
        updated_at: "2026-01-01".into(),
    };
    assert_eq!(
        expected_fields(&[
            "patent_number",
            "current_status",
            "events",
            "source",
            "updated_at"
        ]),
        json_field_names(&legal)
    );
    assert_eq!(
        expected_fields(&["date", "title", "description"]),
        json_field_names(&legal.events[0])
    );
    assert_eq!(
        expected_fields(&["patent_number", "source"]),
        json_field_names(&FetchPatentRequest {
            patent_number: "CN1A".into(),
            source: Some("epo".into()),
        })
    );
    assert_eq!(
        expected_fields(&["patents"]),
        json_field_names(&ImportRequest {
            patents: vec![sample_patent()],
        })
    );
    assert_roundtrip_stable(&legal);
}

// ── 对话族 ───────────────────────────────────────────────────────────────────

#[test]
fn ai_chat_request_field_snapshot_is_unchanged() {
    let req = AiChatRequest {
        message: "帮我看看这条权利要求".into(),
        patent_id: Some("pt-1".into()),
        history: vec![("user".into(), "hi".into())],
        web_search: true,
        images: vec!["AA".into()],
        system_prompt: Some("oa_expert".into()),
        preset_mode: true,
    };
    assert_eq!(
        expected_fields(&[
            "message",
            "patent_id",
            "history",
            "web_search",
            "images",
            "system_prompt",
            "preset_mode",
        ]),
        json_field_names(&req)
    );
    // history 是 Vec<(String,String)> —— 必须仍序列化成二元数组的数组
    assert_eq!(
        serde_json::to_value(&req).unwrap()["history"],
        json!([["user", "hi"]])
    );
    assert_roundtrip_stable(&req);
    let resolved = req
        .effective_system_prompt()
        .expect("oa_expert preset must resolve");
    assert_eq!(
        "你是一位资深中国专",
        resolved.chars().take(9).collect::<String>(),
        "preset 解析结果不能因迁移而改变"
    );
    assert!(resolved.contains("必须分析组合动机"));
}

#[test]
fn ai_response_field_snapshot_is_unchanged() {
    let resp = AiResponse {
        content: "ok".to_string(),
    };
    assert_eq!(expected_fields(&["content"]), json_field_names(&resp));
    assert_exact_json(&resp, r#"{"content":"ok"}"#);
    assert_roundtrip_stable(&resp);
}

#[test]
fn chat_message_field_snapshot_is_unchanged() {
    let msg = ChatMessage {
        id: 7,
        role: "assistant".into(),
        content: "答复".into(),
        created_at: "2026-01-01 00:00:00".into(),
    };
    assert_eq!(
        expected_fields(&["id", "role", "content", "created_at"]),
        json_field_names(&msg)
    );
    assert_exact_json(
        &msg,
        r#"{"id":7,"role":"assistant","content":"答复","created_at":"2026-01-01 00:00:00"}"#,
    );
    assert_roundtrip_stable(&msg);
}

// ── 创意族 ───────────────────────────────────────────────────────────────────

#[test]
fn pipeline_owned_domain_types_field_snapshots_are_unchanged() {
    // 这三个类型原居 pipeline/context.rs，按 types-migration-map.md §2 归属检索/创意域
    let chunk = ReferenceChunk {
        id: "rc-1".into(),
        patent_id: "pt-1".into(),
        chunk_index: 3,
        source_type: "description".into(),
        content: "说明书片段".into(),
        relevance_score: 0.83_f32,
    };
    assert_eq!(
        expected_fields(&[
            "id",
            "patent_id",
            "chunk_index",
            "source_type",
            "content",
            "relevance_score",
        ]),
        json_field_names(&chunk)
    );
    let evidence = Evidence {
        id: "e-1".into(),
        idea_id: "i-1".into(),
        claim: "结论".into(),
        source_type: "patent".into(),
        source_id: "pt-1".into(),
        source_title: "一种固态电池".into(),
        source_url: "https://x".into(),
        claim_number: Some("1".into()),
        excerpt: "原文".into(),
        relation: "supports".into(),
        confidence: 0.9,
        produced_by: "ScoreNovelty".into(),
        created_at: "2026-01-01".into(),
    };
    assert_eq!(
        expected_fields(&[
            "id",
            "idea_id",
            "claim",
            "source_type",
            "source_id",
            "source_title",
            "source_url",
            "claim_number",
            "excerpt",
            "relation",
            "confidence",
            "produced_by",
            "created_at",
        ]),
        json_field_names(&evidence)
    );
    // claim_number 带 skip_serializing_if：None 时键必须消失
    let mut sparse = evidence.clone();
    sparse.claim_number = None;
    assert_eq!(
        expected_fields(&[
            "id",
            "idea_id",
            "claim",
            "source_type",
            "source_id",
            "source_title",
            "source_url",
            "excerpt",
            "relation",
            "confidence",
            "produced_by",
            "created_at",
        ]),
        json_field_names(&sparse)
    );
    let state = ResearchState {
        current_hypothesis: "h".into(),
        excluded_paths: vec!["p1".into()],
        open_questions: vec!["q1".into()],
        verified_claims: vec!["c1".into()],
    };
    assert_eq!(
        expected_fields(&[
            "current_hypothesis",
            "excluded_paths",
            "open_questions",
            "verified_claims",
        ]),
        json_field_names(&state)
    );
    assert_roundtrip_stable(&chunk);
    assert_roundtrip_stable(&evidence);
    assert_roundtrip_stable(&state);
}

#[test]
fn idea_family_field_snapshots_are_unchanged() {
    let idea = Idea {
        id: "i-1".into(),
        title: "固态电池封装".into(),
        description: "d".into(),
        input_type: "text".into(),
        status: "done".into(),
        analysis: "a".into(),
        web_results: "w".into(),
        patent_results: "p".into(),
        novelty_score: Some(0.72),
        created_at: "2026-01-01".into(),
        updated_at: "2026-01-02".into(),
        discussion_summary: "s".into(),
    };
    assert_eq!(
        expected_fields(&[
            "id",
            "title",
            "description",
            "input_type",
            "status",
            "analysis",
            "web_results",
            "patent_results",
            "novelty_score",
            "created_at",
            "updated_at",
            "discussion_summary",
        ]),
        json_field_names(&idea)
    );
    assert_eq!(
        expected_fields(&["title", "description", "input_type"]),
        json_field_names(&IdeaSubmitRequest {
            title: "t".into(),
            description: "d".into(),
            input_type: "text".into(),
        })
    );
    assert_eq!(
        expected_fields(&["name", "content"]),
        json_field_names(&TextAttachment {
            name: "n".into(),
            content: "c".into(),
        })
    );
    assert_eq!(
        expected_fields(&["message", "depth", "images", "attachments"]),
        json_field_names(&IdeaChatRequest {
            message: "m".into(),
            depth: "deep".into(),
            images: vec!["AA".into()],
            attachments: vec![TextAttachment {
                name: "n".into(),
                content: "c".into(),
            }],
        })
    );
    assert_eq!(
        expected_fields(&[
            "id",
            "title",
            "status",
            "novelty_score",
            "created_at",
            "description",
            "message_count",
        ]),
        json_field_names(&IdeaSummary {
            id: "i-1".into(),
            title: "t".into(),
            status: "done".into(),
            novelty_score: Some(0.5),
            created_at: "2026-01-01".into(),
            description: "d".into(),
            message_count: 3,
        })
    );
    // IdeaSubmitRequest.input_type 的 default_text() 仍生效
    let minimal: IdeaSubmitRequest =
        serde_json::from_str(r#"{"title":"t","description":"d"}"#).expect("minimal submit request");
    assert_eq!("text", minimal.input_type);
    assert_roundtrip_stable(&idea);
}

#[test]
fn feature_card_and_claim_tree_snapshots_are_unchanged() {
    use crate::patent::{
        ClaimNode, ClaimType, CreateFeatureCardRequest, FeatureCard, TechnicalFeature,
    };
    let card = FeatureCard {
        id: "f-1".into(),
        idea_id: "i-1".into(),
        title: "t".into(),
        description: "d".into(),
        novelty_score: Some(0.6),
        created_at: "2026-01-01".into(),
        technical_problem: "tp".into(),
        core_structure: "cs".into(),
        key_relations: "kr".into(),
        process_steps: "ps".into(),
        application_scenarios: "as".into(),
    };
    assert_eq!(
        expected_fields(&[
            "id",
            "idea_id",
            "title",
            "description",
            "novelty_score",
            "created_at",
            "technical_problem",
            "core_structure",
            "key_relations",
            "process_steps",
            "application_scenarios",
        ]),
        json_field_names(&card)
    );
    assert_eq!(
        expected_fields(&[
            "id",
            "idea_id",
            "claim_number",
            "claim_type",
            "parent_claim_id",
            "content",
            "features",
            "created_at",
        ]),
        json_field_names(&ClaimNode {
            id: "c-1".into(),
            idea_id: "i-1".into(),
            claim_number: 1,
            claim_type: ClaimType::Independent,
            parent_claim_id: None,
            content: "一种…".into(),
            features: vec![TechnicalFeature {
                id: "tf-1".into(),
                claim_id: "c-1".into(),
                description: "特征".into(),
                novelty_flag: true,
                evidence_ids: vec!["e-1".into()],
            }],
            created_at: "2026-01-01".into(),
        })
    );
    assert_eq!(
        expected_fields(&[
            "id",
            "claim_id",
            "description",
            "novelty_flag",
            "evidence_ids"
        ]),
        json_field_names(&TechnicalFeature {
            id: "tf-1".into(),
            claim_id: "c-1".into(),
            description: "特征".into(),
            novelty_flag: false,
            evidence_ids: vec![],
        })
    );
    assert_eq!(
        serde_json::to_value(ClaimType::Dependent).unwrap(),
        json!("Dependent")
    );
    // CreateFeatureCardRequest 迁移前就只派生 Deserialize（无 Serialize），形状不得顺手扩
    let req: CreateFeatureCardRequest = serde_json::from_str(
        r#"{"title":"t","description":"d","novelty_score":0.5,"technical_problem":"tp",
             "core_structure":"cs","key_relations":"kr","process_steps":"ps",
             "application_scenarios":"as"}"#,
    )
    .expect("feature card request json");
    assert_eq!("t", req.title);
    assert_eq!(Some(0.5), req.novelty_score);
    assert_eq!("as", req.application_scenarios);
}
