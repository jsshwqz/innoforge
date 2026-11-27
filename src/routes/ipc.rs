use axum::{extract::Path, extract::State, Json};
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

/// 解析 `patents.ipc_codes` 字段（兼容 JSON 数组 `["F23D1/00","F23C7/00"]` 与逗号/分号分隔纯文本），
/// 返回归一化大写的有效 IPC 分类号列表（过滤掉 `[`、`"` 等语法噪音与非法部类）。
pub fn parse_ipc_code_list(raw: &str) -> Vec<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed == "[]" {
        return Vec::new();
    }
    let items: Vec<String> = if trimmed.starts_with('[') {
        serde_json::from_str::<Vec<String>>(trimmed).unwrap_or_else(|_| {
            trimmed
                .trim_matches(['[', ']'])
                .split([',', ';', '；', '|'])
                .map(str::to_string)
                .collect()
        })
    } else {
        trimmed
            .split([',', ';', '；', '|'])
            .map(str::to_string)
            .collect()
    };

    let mut out = Vec::new();
    for item in items {
        let cleaned = item
            .trim()
            .trim_matches(['"', '\'', '[', ']', ' '])
            .replace(' ', "")
            .to_uppercase();
        if let Some(first) = cleaned.chars().next() {
            if matches!(first, 'A'..='H') && !out.contains(&cleaned) {
                out.push(cleaned);
            }
        }
    }
    out
}

/// 将 `(ipc_codes_raw, row_count)` 聚合构建为 IPC 三级树 JSON（使用安全 Unicode 字符迭代，禁用裸字节切片）。
fn build_ipc_tree_payload(code_counts: &[(String, usize)]) -> Value {
    let mut section_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    let mut class_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    let mut subclass_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();

    for (raw, weight) in code_counts {
        let w = *weight;
        if w == 0 {
            continue;
        }
        for code in parse_ipc_code_list(raw) {
            let chars: Vec<char> = code.chars().collect();
            let Some(&first) = chars.first() else {
                continue;
            };
            let section = first.to_string();
            *section_counts.entry(section).or_insert(0) += w;

            if chars.len() >= 3 {
                let class: String = chars.iter().take(3).collect();
                *class_counts.entry(class).or_insert(0) += w;
            }
            if chars.len() >= 4 {
                let subclass: String = chars.iter().take(4).collect();
                *subclass_counts.entry(subclass).or_insert(0) += w;
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

    json!({
        "status": "ok",
        "sections": sections,
        "total_codes": section_counts.values().sum::<usize>(),
    })
}

/// GET /api/ipc/tree — build IPC tree from patents in database
pub async fn api_ipc_tree(State(s): State<AppState>) -> Json<Value> {
    let db = s.db.clone();
    match tokio::task::spawn_blocking(move || db.get_ipc_code_counts()).await {
        Ok(Ok(counts)) => Json(build_ipc_tree_payload(&counts)),
        Ok(Err(e)) => Json(json!({"status":"error","message":e.to_string()})),
        Err(e) => Json(json!({"status":"error","message":format!("task join error: {e}")})),
    }
}

/// GET /api/ipc/:code/patents — get patents matching an IPC code prefix
pub async fn api_ipc_patents(State(s): State<AppState>, Path(code): Path<String>) -> Json<Value> {
    let code = code.trim().to_uppercase();
    let db = s.db.clone();
    let query_code = code.clone();
    match tokio::task::spawn_blocking(move || db.search_by_ipc(&query_code)).await {
        Ok(Ok(patents)) => Json(json!({
            "status": "ok",
            "code": code,
            "count": patents.len(),
            "patents": patents,
        })),
        Ok(Err(e)) => Json(json!({"status":"error","message":e.to_string()})),
        Err(e) => Json(json!({"status":"error","message":format!("task join error: {e}")})),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::patent::Patent;

    #[test]
    fn parse_ipc_code_list_handles_json_arrays_and_delimited_strings() {
        assert_eq!(
            parse_ipc_code_list(r#"["F23D 1/00", "F23C7/00", "f23d1/00"]"#),
            vec!["F23D1/00", "F23C7/00"]
        );
        assert_eq!(
            parse_ipc_code_list("A61G10/02; H01M10/0525"),
            vec!["A61G10/02", "H01M10/0525"]
        );
        assert!(parse_ipc_code_list("[]").is_empty());
        assert!(parse_ipc_code_list("中文噪声").is_empty());
    }

    #[test]
    fn build_ipc_tree_classifies_json_arrays_into_valid_sections_not_brackets() {
        let payload = build_ipc_tree_payload(&[
            (r#"["F23D1/00","A61G10/02"]"#.to_string(), 3),
            ("F23C7/00".to_string(), 2),
        ]);
        let sections = payload["sections"].as_array().expect("sections array");
        let codes: Vec<&str> = sections.iter().filter_map(|s| s["code"].as_str()).collect();
        assert_eq!(codes, vec!["A", "F"]);
        assert_eq!(payload["total_codes"], 8);
        assert_eq!(sections[1]["count"], 5);
    }

    #[test]
    fn ipc_prefix_filter_section_f_does_not_false_match_g06f() {
        let db = Database::init(":memory:").expect("init db");
        let p_f = Patent {
            id: "ipc-f-1".into(),
            patent_number: "CN990301A".into(),
            title: "旋流燃烧器".into(),
            ipc_codes: r#"["F23D1/00"]"#.into(),
            ..Default::default()
        };
        let p_g = Patent {
            id: "ipc-g-1".into(),
            patent_number: "CN990302A".into(),
            title: "计算机数据处理".into(),
            ipc_codes: r#"["G06F16/00"]"#.into(),
            ..Default::default()
        };
        db.insert_patent(&p_f).expect("insert f");
        db.insert_patent(&p_g).expect("insert g");

        let hits = db.search_by_ipc("F").expect("search F");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0]["patent_number"], "CN990301A");
    }
}
