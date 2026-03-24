use axum::{extract::State, extract::Path, Json};
use serde_json::{json, Value};

use super::AppState;

/// IPC Section names (top level A-H)
fn ipc_section_name(code: &str) -> &'static str {
    match code {
        "A" => "Human Necessities / 人类生活必需",
        "B" => "Operations & Transport / 作业与运输",
        "C" => "Chemistry & Metallurgy / 化学与冶金",
        "D" => "Textiles & Paper / 纺织与造纸",
        "E" => "Fixed Constructions / 固定建筑物",
        "F" => "Mechanical Engineering / 机械工程",
        "G" => "Physics / 物理",
        "H" => "Electricity / 电学",
        _ => "Unknown",
    }
}

/// GET /api/ipc/tree — build IPC tree from patents in database
pub async fn api_ipc_tree(State(s): State<AppState>) -> Json<Value> {
    let all_codes = match s.db.get_all_ipc_codes() {
        Ok(v) => v,
        Err(e) => return Json(json!({"status":"error","message":e.to_string()})),
    };

    let mut section_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    let mut class_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    let mut subclass_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();

    for raw in &all_codes {
        for code in raw.split(',') {
            let code = code.trim();
            if code.is_empty() {
                continue;
            }
            let section = code.chars().next().unwrap_or('?').to_uppercase().to_string();
            *section_counts.entry(section.clone()).or_insert(0) += 1;

            if code.len() >= 3 {
                let class = code[..3].to_uppercase();
                *class_counts.entry(class).or_insert(0) += 1;
            }

            if code.len() >= 4 {
                let subclass = code[..4].to_uppercase();
                *subclass_counts.entry(subclass).or_insert(0) += 1;
            }
        }
    }

    let mut section_keys: Vec<String> = section_counts.keys().cloned().collect();
    section_keys.sort();

    let sections: Vec<Value> = section_keys
        .iter()
        .map(|section_code| {
            let count = section_counts[section_code];
            let name = ipc_section_name(section_code);

            let mut class_keys: Vec<String> = class_counts
                .keys()
                .filter(|k| k.starts_with(section_code.as_str()))
                .cloned()
                .collect();
            class_keys.sort();

            let classes: Vec<Value> = class_keys
                .iter()
                .map(|class_code| {
                    let mut sub_keys: Vec<String> = subclass_counts
                        .keys()
                        .filter(|k| k.starts_with(class_code.as_str()))
                        .cloned()
                        .collect();
                    sub_keys.sort();

                    let subclasses: Vec<Value> = sub_keys
                        .iter()
                        .map(|sub_code| {
                            json!({
                                "code": sub_code,
                                "count": subclass_counts[sub_code],
                            })
                        })
                        .collect();

                    json!({
                        "code": class_code,
                        "count": class_counts[class_code],
                        "subclasses": subclasses,
                    })
                })
                .collect();

            json!({
                "code": section_code,
                "name": name,
                "count": count,
                "classes": classes,
            })
        })
        .collect();

    Json(json!({
        "status": "ok",
        "sections": sections,
        "total_codes": section_counts.values().sum::<usize>(),
    }))
}

/// GET /api/ipc/:code/patents — get patents matching an IPC code prefix
pub async fn api_ipc_patents(
    State(s): State<AppState>,
    Path(code): Path<String>,
) -> Json<Value> {
    let code = code.trim().to_uppercase();
    match s.db.search_by_ipc(&code) {
        Ok(patents) => Json(json!({
            "status": "ok",
            "code": code,
            "count": patents.len(),
            "patents": patents,
        })),
        Err(e) => Json(json!({"status":"error","message":e.to_string()})),
    }
}
