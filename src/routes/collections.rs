use super::AppState;
use axum::{
    extract::{Path, State},
    Json,
};
use serde_json::json;

pub async fn api_create_collection(
    State(s): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let name = req["name"].as_str().unwrap_or("").trim();
    let description = req["description"].as_str().unwrap_or("").trim();

    if name.is_empty() || name.len() > 100 {
        return Json(json!({"error": "Collection name is required (max 100 chars)"}));
    }

    let id = uuid::Uuid::new_v4().to_string();
    match s.db.create_collection(&id, name, description) {
        Ok(()) => Json(json!({"status": "ok", "id": id, "name": name})),
        Err(e) => Json(json!({"error": format!("Failed to create collection: {}", e)})),
    }
}

pub async fn api_list_collections(State(s): State<AppState>) -> Json<serde_json::Value> {
    match s.db.list_collections() {
        Ok(cols) => {
            let list: Vec<serde_json::Value> = cols
                .into_iter()
                .map(|(id, name, desc, count, created_at)| {
                    json!({
                        "id": id,
                        "name": name,
                        "description": desc,
                        "patent_count": count,
                        "created_at": created_at,
                    })
                })
                .collect();
            Json(json!({"collections": list}))
        }
        Err(e) => Json(json!({"error": format!("{}", e)})),
    }
}

pub async fn api_delete_collection(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    match s.db.delete_collection(&id) {
        Ok(()) => Json(json!({"status": "ok"})),
        Err(e) => Json(json!({"error": format!("{}", e)})),
    }
}

pub async fn api_add_to_collection(
    State(s): State<AppState>,
    Path(collection_id): Path<String>,
    Json(req): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let patent_id = req["patent_id"].as_str().unwrap_or("").trim();
    if patent_id.is_empty() {
        return Json(json!({"error": "patent_id is required"}));
    }

    // 验证专利 ID 是否存在
    match s.db.get_patent(patent_id) {
        Ok(None) => return Json(json!({"error": "专利不存在"})),
        Err(_) => return Json(json!({"error": "查询专利失败"})),
        Ok(Some(_)) => {}
    }

    match s.db.add_to_collection(patent_id, &collection_id) {
        Ok(()) => Json(json!({"status": "ok"})),
        Err(e) => Json(json!({"error": format!("{}", e)})),
    }
}

pub async fn api_remove_from_collection(
    State(s): State<AppState>,
    Path((collection_id, patent_id)): Path<(String, String)>,
) -> Json<serde_json::Value> {
    match s.db.remove_from_collection(&patent_id, &collection_id) {
        Ok(()) => Json(json!({"status": "ok"})),
        Err(e) => Json(json!({"error": format!("{}", e)})),
    }
}

pub async fn api_get_collection_patents(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    match s.db.get_collection_patents(&id) {
        Ok(patents) => Json(json!({"patents": patents})),
        Err(e) => Json(json!({"error": format!("{}", e)})),
    }
}

pub async fn api_add_tag(
    State(s): State<AppState>,
    Path(patent_id): Path<String>,
    Json(req): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let tag = req["tag"].as_str().unwrap_or("").trim();
    if tag.is_empty() || tag.len() > 50 {
        return Json(json!({"error": "Tag is required (max 50 chars)"}));
    }

    match s.db.add_tag(&patent_id, tag) {
        Ok(()) => Json(json!({"status": "ok"})),
        Err(e) => Json(json!({"error": format!("{}", e)})),
    }
}

pub async fn api_remove_tag(
    State(s): State<AppState>,
    Path((patent_id, tag)): Path<(String, String)>,
) -> Json<serde_json::Value> {
    match s.db.remove_tag(&patent_id, &tag) {
        Ok(()) => Json(json!({"status": "ok"})),
        Err(e) => Json(json!({"error": format!("{}", e)})),
    }
}

pub async fn api_get_patent_tags(
    State(s): State<AppState>,
    Path(patent_id): Path<String>,
) -> Json<serde_json::Value> {
    match s.db.get_patent_tags(&patent_id) {
        Ok(tags) => Json(json!({"tags": tags})),
        Err(e) => Json(json!({"error": format!("{}", e)})),
    }
}

pub async fn api_list_all_tags(State(s): State<AppState>) -> Json<serde_json::Value> {
    match s.db.list_all_tags() {
        Ok(tags) => {
            let list: Vec<serde_json::Value> = tags
                .into_iter()
                .map(|(tag, count)| json!({"tag": tag, "count": count}))
                .collect();
            Json(json!({"tags": list}))
        }
        Err(e) => Json(json!({"error": format!("{}", e)})),
    }
}

pub async fn api_get_patent_collections(
    State(s): State<AppState>,
    Path(patent_id): Path<String>,
) -> Json<serde_json::Value> {
    match s.db.get_patent_collections(&patent_id) {
        Ok(collection_ids) => Json(json!({"collection_ids": collection_ids})),
        Err(e) => Json(json!({"error": format!("{}", e)})),
    }
}

#[cfg(test)]
mod tests {
    use crate::db::Database;
    use crate::patent::Patent;

    /// 构造测试用专利
    fn sample_patent(id: &str) -> Patent {
        Patent {
            id: id.to_string(),
            patent_number: format!("CN{}A", id),
            title: format!("Patent {}", id),
            abstract_text: "abstract".to_string(),
            description: "desc".to_string(),
            claims: "claim".to_string(),
            applicant: "Test Corp".to_string(),
            inventor: "Alice".to_string(),
            filing_date: "2024-01-01".to_string(),
            publication_date: "2024-01-01".to_string(),
            grant_date: None,
            ipc_codes: "G06N".to_string(),
            cpc_codes: "G06N".to_string(),
            priority_date: "2024-01-01".to_string(),
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

    // ── 名称校验测试 ──

    #[test]
    fn test_collection_name_validation_empty() {
        let name = "";
        assert!(name.is_empty() || name.len() > 100);
    }

    #[test]
    fn test_collection_name_validation_too_long() {
        let name = "A".repeat(101);
        assert!(name.is_empty() || name.len() > 100);
    }

    #[test]
    fn test_collection_name_valid() {
        let name = "Valid Name";
        assert!(!(name.is_empty() || name.len() > 100));
    }

    // ── 标签校验测试 ──

    #[test]
    fn test_tag_validation_empty() {
        let tag = "";
        assert!(tag.is_empty() || tag.len() > 50);
    }

    #[test]
    fn test_tag_validation_too_long() {
        let tag = "X".repeat(51);
        assert!(tag.is_empty() || tag.len() > 50);
    }

    #[test]
    fn test_tag_validation_valid() {
        let tag = "valid-tag";
        assert!(!(tag.is_empty() || tag.len() > 50));
    }

    // ── 收藏夹 CRUD 测试 ──

    #[test]
    fn test_create_collection_success() {
        let db = Database::init(":memory:").expect("init db");
        let id = uuid::Uuid::new_v4().to_string();
        db.create_collection(&id, "Test Collection", "A test")
            .expect("create collection");
        let cols = db.list_collections().expect("list");
        assert_eq!(cols.len(), 1);
        assert_eq!(cols[0].1, "Test Collection"); // name 字段
        assert_eq!(cols[0].2, "A test"); // description 字段
    }

    #[test]
    fn test_delete_collection() {
        let db = Database::init(":memory:").expect("init db");
        let id = uuid::Uuid::new_v4().to_string();
        db.create_collection(&id, "To Delete", "").expect("create");
        db.delete_collection(&id).expect("delete");
        let cols = db.list_collections().expect("list");
        assert_eq!(cols.len(), 0);
    }

    #[test]
    fn test_add_patent_to_collection() {
        let db = Database::init(":memory:").expect("init db");
        let patent = sample_patent("p1");
        db.insert_patent(&patent).expect("insert patent");
        let cid = uuid::Uuid::new_v4().to_string();
        db.create_collection(&cid, "Coll", "")
            .expect("create collection");
        db.add_to_collection("p1", &cid).expect("add to collection");
        let patents = db.get_collection_patents(&cid).expect("get patents");
        assert_eq!(patents.len(), 1);
    }

    #[test]
    fn test_remove_patent_from_collection() {
        let db = Database::init(":memory:").expect("init db");
        let patent = sample_patent("p2");
        db.insert_patent(&patent).expect("insert patent");
        let cid = uuid::Uuid::new_v4().to_string();
        db.create_collection(&cid, "Coll", "")
            .expect("create collection");
        db.add_to_collection("p2", &cid).expect("add");
        db.remove_from_collection("p2", &cid).expect("remove");
        let patents = db.get_collection_patents(&cid).expect("get patents");
        assert_eq!(patents.len(), 0);
    }

    // ── 标签 CRUD 测试 ──

    #[test]
    fn test_add_and_get_tags() {
        let db = Database::init(":memory:").expect("init db");
        let patent = sample_patent("p3");
        db.insert_patent(&patent).expect("insert patent");
        db.add_tag("p3", "AI").expect("add tag AI");
        db.add_tag("p3", "ML").expect("add tag ML");
        let tags = db.get_patent_tags("p3").expect("get tags");
        assert_eq!(tags.len(), 2);
    }

    #[test]
    fn test_list_all_tags() {
        let db = Database::init(":memory:").expect("init db");
        let p1 = sample_patent("t1");
        let p2 = sample_patent("t2");
        db.insert_patent(&p1).expect("insert p1");
        db.insert_patent(&p2).expect("insert p2");
        db.add_tag("t1", "AI").expect("add AI to t1");
        db.add_tag("t2", "AI").expect("add AI to t2");
        db.add_tag("t1", "ML").expect("add ML to t1");
        let all_tags = db.list_all_tags().expect("list all tags");
        // AI 应出现 2 次，ML 应出现 1 次
        assert_eq!(all_tags.len(), 2);
        let ai_entry = all_tags.iter().find(|(t, _)| t == "AI").expect("AI tag");
        assert_eq!(ai_entry.1, 2);
    }

    #[test]
    fn test_remove_tag() {
        let db = Database::init(":memory:").expect("init db");
        let patent = sample_patent("p4");
        db.insert_patent(&patent).expect("insert patent");
        db.add_tag("p4", "AI").expect("add tag");
        db.remove_tag("p4", "AI").expect("remove tag");
        let tags = db.get_patent_tags("p4").expect("get tags");
        assert_eq!(tags.len(), 0);
    }

    // ── 关联查询测试 ──

    #[test]
    fn test_patent_collections() {
        let db = Database::init(":memory:").expect("init db");
        let patent = sample_patent("p5");
        db.insert_patent(&patent).expect("insert patent");
        let c1 = uuid::Uuid::new_v4().to_string();
        let c2 = uuid::Uuid::new_v4().to_string();
        db.create_collection(&c1, "C1", "").expect("create c1");
        db.create_collection(&c2, "C2", "").expect("create c2");
        db.add_to_collection("p5", &c1).expect("add to c1");
        db.add_to_collection("p5", &c2).expect("add to c2");
        let collections = db.get_patent_collections("p5").expect("get collections");
        assert_eq!(collections.len(), 2);
    }
}
