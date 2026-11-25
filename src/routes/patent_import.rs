//! 专利表格批量导入与本地/网盘同步目录扫描模块
//!
//! 支持直接解析国知局（CNIPA）、Google Patents、智慧芽（PatSnap）、incoPat 等导出的
//! `.xlsx` / `.csv` / `.tsv` / `.json` / `.jsonl` 轻量著录表格，批量幂等入库本地 SQLite
//! （并严格保留已有已富化全文/权利要求/附图，绝不覆写为空）。
//! 同时支持用户配置私人网盘/本地同步目录，一键增量扫描新增或变更表格。

use crate::patent::Patent;
use crate::routes::AppState;
use axum::extract::{Multipart, State};
use axum::response::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

const SETTING_SYNC_FOLDER: &str = "patent_sync_folder";
const SETTING_SYNC_IPC_PREFIX: &str = "patent_sync_ipc_prefix";
const SETTING_SYNC_LIGHTWEIGHT: &str = "patent_sync_lightweight";
const SETTING_SYNC_LAST_AT: &str = "patent_sync_last_at";
const SETTING_SYNC_LAST_SUMMARY: &str = "patent_sync_last_summary";
const SETTING_SYNC_MANIFEST: &str = "patent_sync_file_manifest";

const MAX_TABLE_BYTES: usize = 30 * 1024 * 1024; // 30 MB
const MAX_SYNC_FILES: usize = 200;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PatentTableImportOptions {
    #[serde(default)]
    pub ipc_prefix: String,
    #[serde(default)]
    pub lightweight: bool,
    #[serde(default)]
    pub source_tag: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedTableStats {
    pub total_rows: usize,
    pub matched_patents: Vec<Patent>,
    pub filtered_out: usize,
    pub skipped_empty: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncFolderRequest {
    pub folder_path: String,
    #[serde(default)]
    pub ipc_prefix: String,
    #[serde(default)]
    pub lightweight: bool,
    #[serde(default)]
    pub save_only: bool,
    #[serde(default)]
    pub force: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FileSyncResult {
    filename: String,
    status: String,
    imported: usize,
    filtered_out: usize,
    chunks: usize,
    message: String,
}

fn is_supported_table_ext(ext_lower: &str) -> bool {
    matches!(ext_lower, "xlsx" | "csv" | "tsv" | "json" | "jsonl")
}

fn normalize_header_key(raw: &str) -> String {
    raw.trim()
        .trim_start_matches('\u{feff}')
        .to_lowercase()
        .chars()
        .filter(|c| {
            !c.is_whitespace()
                && !matches!(
                    *c,
                    '_' | '-' | '/' | '\\' | '（' | '）' | '(' | ')' | ':' | '：' | '.' | '·'
                )
        })
        .collect()
}

fn decode_xml_entities(s: &str) -> String {
    let mut out = s
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#10;", "\n")
        .replace("&#13;", "\r")
        .replace("&#9;", "\t");
    // 兼容常见十进制/十六进制数字字符引用（如 &#x000A; / &#39;）
    if out.contains("&#") {
        let mut decoded = String::with_capacity(out.len());
        let mut chars = out.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '&' && chars.peek() == Some(&'#') {
                let mut buf = String::new();
                let mut found_semi = false;
                let mut clone = chars.clone();
                let _ = clone.next(); // '#'
                for c in clone.by_ref() {
                    if c == ';' {
                        found_semi = true;
                        break;
                    }
                    if buf.len() > 10 {
                        break;
                    }
                    buf.push(c);
                }
                if found_semi {
                    let parsed = if let Some(hex) =
                        buf.strip_prefix('x').or_else(|| buf.strip_prefix('X'))
                    {
                        u32::from_str_radix(hex, 16).ok()
                    } else {
                        buf.parse::<u32>().ok()
                    };
                    if let Some(cp) = parsed.and_then(char::from_u32) {
                        decoded.push(cp);
                        chars = clone;
                        continue;
                    }
                }
            }
            decoded.push(ch);
        }
        out = decoded;
    }
    out
}

fn extract_xml_tag_texts(xml: &str, tag_name: &str) -> String {
    let open_prefix = format!("<{}", tag_name);
    let close_tag = format!("</{}>", tag_name);
    let mut out = String::new();
    let mut rest = xml;
    while let Some(start) = rest.find(&open_prefix) {
        let after_open = rest.get(start + open_prefix.len()..).unwrap_or("");
        let first_ch = after_open.chars().next();
        if !matches!(
            first_ch,
            Some('>') | Some(' ') | Some('\t') | Some('\n') | Some('\r')
        ) {
            rest = after_open;
            continue;
        }
        let Some(gt_idx) = after_open.find('>') else {
            break;
        };
        let header_part = after_open.get(..gt_idx).unwrap_or("");
        if header_part.trim_end().ends_with('/') {
            rest = after_open.get(gt_idx + 1..).unwrap_or("");
            continue;
        }
        let body_start = after_open.get(gt_idx + 1..).unwrap_or("");
        let Some(close_idx) = body_start.find(&close_tag) else {
            break;
        };
        let text = body_start.get(..close_idx).unwrap_or("");
        out.push_str(&decode_xml_entities(text));
        rest = body_start.get(close_idx + close_tag.len()..).unwrap_or("");
    }
    out
}

fn parse_cell_col_index(cell_header: &str) -> Option<usize> {
    let r_pos = cell_header
        .find("r=\"")
        .or_else(|| cell_header.find("r='"))?;
    let after = cell_header.get(r_pos + 3..)?;
    let mut col: usize = 0;
    let mut has_alpha = false;
    for ch in after.chars() {
        if ch.is_ascii_alphabetic() {
            has_alpha = true;
            let upper = ch.to_ascii_uppercase() as usize;
            col = col
                .checked_mul(26)?
                .checked_add(upper - ('A' as usize) + 1)?;
        } else {
            break;
        }
    }
    if has_alpha && col > 0 {
        Some(col - 1)
    } else {
        None
    }
}

/// 解析 XLSX 首张工作表为保留精确列索引的二维字符串矩阵（空单元格绝不导致列错位）。
fn parse_xlsx_rows(data: &[u8]) -> Result<Vec<Vec<String>>, String> {
    let mut archive =
        zip::ZipArchive::new(Cursor::new(data)).map_err(|e| format!("非有效 XLSX 文件: {e}"))?;

    let mut shared_strings: Vec<String> = Vec::new();
    if let Ok(mut ss_file) = archive.by_name("xl/sharedStrings.xml") {
        let mut xml = String::new();
        let _ = ss_file.read_to_string(&mut xml);
        for si in xml.split("<si").skip(1) {
            let Some(first) = si.chars().next() else {
                continue;
            };
            if first != '>' && !first.is_whitespace() {
                continue;
            }
            let end = si.find("</si>").unwrap_or(si.len());
            let chunk = si.get(..end).unwrap_or(si);
            shared_strings.push(extract_xml_tag_texts(chunk, "t"));
        }
    }

    let mut sheet_names: Vec<(usize, String)> = Vec::new();
    for i in 0..archive.len() {
        if let Ok(file) = archive.by_index(i) {
            let name = file.name().to_string();
            if name.starts_with("xl/worksheets/sheet") && name.ends_with(".xml") {
                let num = name
                    .trim_start_matches("xl/worksheets/sheet")
                    .trim_end_matches(".xml")
                    .parse::<usize>()
                    .unwrap_or(usize::MAX);
                sheet_names.push((num, name));
            }
        }
    }
    sheet_names.sort_by_key(|(n, _)| *n);
    let Some((_, first_sheet)) = sheet_names.into_iter().next() else {
        return Err("XLSX 中未找到工作表（xl/worksheets/sheet*.xml）".into());
    };

    let mut xml = String::new();
    let mut sheet_file = archive
        .by_name(&first_sheet)
        .map_err(|e| format!("读取工作表失败: {e}"))?;
    sheet_file
        .read_to_string(&mut xml)
        .map_err(|e| format!("解码工作表 XML 失败: {e}"))?;

    let mut matrix: Vec<Vec<String>> = Vec::new();
    for row_chunk in xml.split("<row").skip(1) {
        let Some(first) = row_chunk.chars().next() else {
            continue;
        };
        if first != '>' && !first.is_whitespace() {
            continue;
        }
        let row_end = row_chunk.find("</row>").unwrap_or(row_chunk.len());
        let row_slice = row_chunk.get(..row_end).unwrap_or(row_chunk);

        let mut row_cells: Vec<String> = Vec::new();
        let mut cursor_col = 0usize;
        for cell_chunk in row_slice.split("<c").skip(1) {
            let Some(c_first) = cell_chunk.chars().next() else {
                continue;
            };
            if c_first != '>' && !c_first.is_whitespace() {
                continue;
            }
            let cell_end = cell_chunk.find("</c>").unwrap_or(cell_chunk.len());
            let cell_slice = cell_chunk.get(..cell_end).unwrap_or(cell_chunk);
            let tag_close = cell_slice.find('>').unwrap_or(0);
            let header = cell_slice.get(..tag_close).unwrap_or("");
            let col_idx = parse_cell_col_index(header).unwrap_or(cursor_col);
            cursor_col = col_idx + 1;

            // 自闭合空单元格 <c r="B2"/>
            if header.trim_end().ends_with('/') {
                if row_cells.len() <= col_idx {
                    row_cells.resize(col_idx + 1, String::new());
                }
                continue;
            }

            let is_shared = header.contains("t=\"s\"") || header.contains("t='s'");
            let is_inline = header.contains("t=\"inlineStr\"") || header.contains("t='inlineStr'");
            let val = if is_inline {
                extract_xml_tag_texts(cell_slice, "t")
            } else {
                let raw_v = extract_xml_tag_texts(cell_slice, "v");
                if is_shared {
                    raw_v
                        .trim()
                        .parse::<usize>()
                        .ok()
                        .and_then(|i| shared_strings.get(i).cloned())
                        .unwrap_or(raw_v)
                } else {
                    raw_v
                }
            };
            if row_cells.len() <= col_idx {
                row_cells.resize(col_idx + 1, String::new());
            }
            if let Some(slot) = row_cells.get_mut(col_idx) {
                *slot = val.trim().to_string();
            }
        }
        if row_cells.iter().any(|c| !c.trim().is_empty()) {
            matrix.push(row_cells);
        }
    }
    Ok(matrix)
}

/// 解码 UTF-8（含 BOM）或 Windows 国知局常见的 GB18030/GBK 文本。
fn decode_text_bytes(data: &[u8]) -> String {
    let stripped = data.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(data);
    if let Ok(s) = std::str::from_utf8(stripped) {
        return s.to_string();
    }
    let (cow, _, _) = encoding_rs::GB18030.decode(stripped);
    cow.into_owned()
}

/// 解析 RFC-4180 CSV / TSV（支持引号包裹逗号、引号内换行与双引号转义 `""`）。
fn parse_delimited_rows(text: &str, force_tab: bool) -> Vec<Vec<String>> {
    let delimiter = if force_tab {
        '\t'
    } else {
        let first_line = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
        let tabs = first_line.matches('\t').count();
        let commas = first_line.matches(',').count();
        let semis = first_line.matches(';').count();
        if tabs > commas && tabs >= 2 {
            '\t'
        } else if commas == 0 && semis >= 2 {
            ';'
        } else {
            ','
        }
    };

    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        if in_quotes {
            if ch == '"' {
                if chars.peek() == Some(&'"') {
                    let _ = chars.next();
                    field.push('"');
                } else {
                    in_quotes = false;
                }
            } else {
                field.push(ch);
            }
        } else if ch == '"' && field.is_empty() {
            in_quotes = true;
        } else if ch == delimiter {
            row.push(field.trim().to_string());
            field.clear();
        } else if ch == '\r' {
            if chars.peek() == Some(&'\n') {
                let _ = chars.next();
            }
            row.push(field.trim().to_string());
            field.clear();
            if row.iter().any(|c| !c.is_empty()) {
                rows.push(std::mem::take(&mut row));
            } else {
                row.clear();
            }
        } else if ch == '\n' {
            row.push(field.trim().to_string());
            field.clear();
            if row.iter().any(|c| !c.is_empty()) {
                rows.push(std::mem::take(&mut row));
            } else {
                row.clear();
            }
        } else {
            field.push(ch);
        }
    }

    if !field.is_empty() || !row.is_empty() {
        row.push(field.trim().to_string());
        if row.iter().any(|c| !c.is_empty()) {
            rows.push(row);
        }
    }
    rows
}

fn excel_serial_to_date(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.contains('-') || trimmed.contains('/') || trimmed.contains('.') {
        return None;
    }
    let val: f64 = trimmed.parse().ok()?;
    if !(20_000.0..=70_000.0).contains(&val) {
        return None;
    }
    let days = val.floor() as i64;
    // Excel 1900 序列基准：1899-12-30 + days
    let base = chrono::NaiveDate::from_ymd_opt(1899, 12, 30)?;
    let date = base.checked_add_signed(chrono::Duration::days(days))?;
    Some(date.format("%Y-%m-%d").to_string())
}

fn normalize_date_str(raw: &str) -> String {
    let t = raw.trim();
    if t.is_empty() {
        return String::new();
    }
    if let Some(excel) = excel_serial_to_date(t) {
        return excel;
    }
    // 兼容 2024.05.12 / 2024/05/12 / 20240512
    let replaced = t.replace(['.', '/'], "-");
    let digits: String = t.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() == 8 && !replaced.contains('-') {
        let y: String = digits.chars().take(4).collect();
        let m: String = digits.chars().skip(4).take(2).collect();
        let d: String = digits.chars().skip(6).take(2).collect();
        return format!("{y}-{m}-{d}");
    }
    replaced
}

fn normalize_codes_json(raw: &str) -> String {
    let t = raw.trim();
    if t.is_empty() {
        return "[]".to_string();
    }
    if t.starts_with('[') && serde_json::from_str::<Vec<String>>(t).is_ok() {
        return t.to_string();
    }
    let parts: Vec<String> = t
        .split([';', '；', ',', '，', '|', '\n'])
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if parts.is_empty() {
        "[]".to_string()
    } else {
        serde_json::to_string(&parts).unwrap_or_else(|_| "[]".to_string())
    }
}

fn infer_country_code(patent_number: &str, explicit: &str) -> String {
    let exp = explicit.trim().to_uppercase();
    if !exp.is_empty() {
        return exp;
    }
    let pn: String = patent_number
        .trim()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_uppercase();
    let prefix: String = pn.chars().take(2).collect();
    if prefix.len() == 2 && prefix.chars().all(|c| c.is_ascii_alphabetic()) {
        return prefix;
    }
    "CN".to_string()
}

fn safe_truncate_chars(s: &str, max_chars: usize) -> String {
    let count = s.chars().count();
    if count <= max_chars {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max_chars).collect();
        out.push('…');
        out
    }
}

fn pick_alias(map: &HashMap<String, String>, aliases: &[&str]) -> String {
    for alias in aliases {
        let key = normalize_header_key(alias);
        if let Some(v) = map.get(&key) {
            if !v.trim().is_empty() {
                return v.trim().to_string();
            }
        }
    }
    String::new()
}

fn row_map_to_patent(
    row: &HashMap<String, String>,
    options: &PatentTableImportOptions,
) -> Option<Patent> {
    let mut pn = pick_alias(
        row,
        &[
            "公开（公告）号",
            "公开公告号",
            "公开号",
            "公告号",
            "文献号",
            "专利号",
            "publication_number",
            "pub_number",
            "patent_number",
            "publication",
            "doc_number",
            "id",
        ],
    );
    if pn.is_empty() {
        pn = pick_alias(row, &["申请号", "application_number"]);
    }
    let title = pick_alias(
        row,
        &[
            "发明名称",
            "专利名称",
            "名称",
            "标题",
            "title",
            "invention_title",
        ],
    );
    if pn.is_empty() && title.is_empty() {
        return None;
    }

    let mut abstract_text = pick_alias(row, &["摘要", "文摘", "abstract", "abstract_text"]);
    let mut claims = pick_alias(
        row,
        &["权利要求书", "权利要求", "主权项", "独立权利要求", "claims"],
    );
    let mut description = pick_alias(row, &["说明书", "description"]);

    if options.lightweight {
        abstract_text = safe_truncate_chars(&abstract_text, 400);
        claims.clear();
        description.clear();
    }

    let applicant = pick_alias(
        row,
        &[
            "申请（专利权）人",
            "申请人",
            "专利权人",
            "当前权利人",
            "applicant",
            "assignee",
            "assignee_original",
        ],
    );
    let inventor = pick_alias(
        row,
        &[
            "发明（设计）人",
            "发明人",
            "设计人",
            "inventor",
            "inventor_original",
        ],
    );
    let filing_date = normalize_date_str(&pick_alias(
        row,
        &["申请日", "filing_date", "application_date"],
    ));
    let publication_date = normalize_date_str(&pick_alias(
        row,
        &[
            "公开（公告）日",
            "公开公告日",
            "公开日",
            "公告日",
            "publication_date",
            "pub_date",
        ],
    ));
    let grant_raw = normalize_date_str(&pick_alias(row, &["授权公告日", "授权日", "grant_date"]));
    let grant_date = if grant_raw.is_empty() {
        None
    } else {
        Some(grant_raw)
    };
    let ipc_raw = pick_alias(
        row,
        &["ipc分类号", "主分类号", "分类号", "ipc", "ipc_codes"],
    );
    let cpc_raw = pick_alias(row, &["cpc分类号", "cpc", "cpc_codes"]);
    let ipc_codes = normalize_codes_json(&ipc_raw);
    let cpc_codes = normalize_codes_json(&cpc_raw);
    let country = infer_country_code(
        &pn,
        &pick_alias(
            row,
            &["国家", "国家/地区", "国别", "country", "jurisdiction"],
        ),
    );
    let legal_status = pick_alias(
        row,
        &[
            "法律状态",
            "案件状态",
            "当前法律状态",
            "legal_status",
            "status",
        ],
    );
    let pdf_url = pick_alias(row, &["pdf链接", "pdf_url", "result_link", "url"]);
    let source = if options.source_tag.trim().is_empty() {
        "table_import".to_string()
    } else {
        options.source_tag.trim().to_string()
    };

    Some(Patent {
        id: uuid::Uuid::new_v4().to_string(),
        patent_number: pn.trim().to_uppercase(),
        title: if title.is_empty() {
            "(无标题)".to_string()
        } else {
            title
        },
        abstract_text,
        description,
        claims,
        applicant,
        inventor,
        filing_date,
        publication_date,
        grant_date,
        ipc_codes,
        cpc_codes,
        priority_date: String::new(),
        country,
        kind_code: String::new(),
        family_id: None,
        legal_status,
        citations: "[]".to_string(),
        cited_by: "[]".to_string(),
        source,
        raw_json: String::new(),
        images: "[]".to_string(),
        pdf_url,
        created_at: String::new(),
    })
}

fn matches_ipc_prefixes(patent: &Patent, prefixes: &[String]) -> bool {
    if prefixes.is_empty() {
        return true;
    }
    let hay = format!("{} {}", patent.ipc_codes, patent.cpc_codes).to_uppercase();
    prefixes.iter().any(|p| hay.contains(p))
}

fn parse_ipc_prefixes(raw: &str) -> Vec<String> {
    raw.split([',', '，', ';', '；', ' '])
        .map(|s| s.trim().to_uppercase())
        .filter(|s| !s.is_empty())
        .collect()
}

fn matrix_to_row_maps(matrix: Vec<Vec<String>>) -> Vec<HashMap<String, String>> {
    // 跳过国知局/智慧芽可能带有的前 1-3 行说明标题，自动探测包含专利号或发明名称的真实表头行
    let header_idx = matrix
        .iter()
        .take(5)
        .position(|row| {
            row.iter().any(|cell| {
                let k = normalize_header_key(cell);
                matches!(
                    k.as_str(),
                    "公开公告号"
                        | "公开号"
                        | "公告号"
                        | "申请号"
                        | "专利号"
                        | "publicationnumber"
                        | "patentnumber"
                        | "id"
                        | "发明名称"
                        | "title"
                )
            })
        })
        .unwrap_or(0);

    let Some(header_row) = matrix.get(header_idx) else {
        return Vec::new();
    };
    let headers: Vec<String> = header_row.iter().map(|h| normalize_header_key(h)).collect();

    let mut out = Vec::new();
    for row in matrix.into_iter().skip(header_idx + 1) {
        let mut map = HashMap::new();
        for (i, val) in row.into_iter().enumerate() {
            if let Some(h) = headers.get(i) {
                if !h.is_empty() && !val.trim().is_empty() {
                    map.entry(h.clone()).or_insert(val);
                }
            }
        }
        if !map.is_empty() {
            out.push(map);
        }
    }
    out
}

fn json_value_to_row_map(val: &Value) -> Option<HashMap<String, String>> {
    let obj = val.as_object()?;
    let mut map = HashMap::new();
    for (k, v) in obj {
        let norm_k = normalize_header_key(k);
        if norm_k.is_empty() {
            continue;
        }
        let s = match v {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            Value::Array(arr) => {
                let items: Vec<String> = arr
                    .iter()
                    .filter_map(|x| x.as_str().map(|s| s.to_string()))
                    .collect();
                serde_json::to_string(&items).unwrap_or_default()
            }
            Value::Null => String::new(),
            other => other.to_string(),
        };
        if !s.trim().is_empty() {
            map.insert(norm_k, s);
        }
    }
    if map.is_empty() {
        None
    } else {
        Some(map)
    }
}

pub fn parse_patent_table_bytes(
    filename: &str,
    data: &[u8],
    options: &PatentTableImportOptions,
) -> Result<ParsedTableStats, String> {
    let ext = Path::new(filename)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    if !is_supported_table_ext(&ext) {
        return Err(format!(
            "不支持的文件格式 `.{ext}`，请上传 .xlsx / .csv / .tsv / .json / .jsonl"
        ));
    }

    let row_maps: Vec<HashMap<String, String>> = match ext.as_str() {
        "xlsx" => matrix_to_row_maps(parse_xlsx_rows(data)?),
        "csv" => matrix_to_row_maps(parse_delimited_rows(&decode_text_bytes(data), false)),
        "tsv" => matrix_to_row_maps(parse_delimited_rows(&decode_text_bytes(data), true)),
        "json" => {
            let text = decode_text_bytes(data);
            let parsed: Value =
                serde_json::from_str(&text).map_err(|e| format!("JSON 解析失败: {e}"))?;
            let items: Vec<Value> = if let Some(arr) = parsed.as_array() {
                arr.clone()
            } else if let Some(arr) = parsed.get("patents").and_then(|v| v.as_array()) {
                arr.clone()
            } else if let Some(arr) = parsed.get("results").and_then(|v| v.as_array()) {
                arr.clone()
            } else if parsed.is_object() {
                vec![parsed]
            } else {
                Vec::new()
            };
            items.iter().filter_map(json_value_to_row_map).collect()
        }
        "jsonl" => {
            let text = decode_text_bytes(data);
            text.lines()
                .filter(|l| !l.trim().is_empty())
                .filter_map(|line| serde_json::from_str::<Value>(line).ok())
                .filter_map(|v| json_value_to_row_map(&v))
                .collect()
        }
        _ => Vec::new(),
    };

    let prefixes = parse_ipc_prefixes(&options.ipc_prefix);
    let total_rows = row_maps.len();
    let mut matched_patents = Vec::new();
    let mut filtered_out = 0usize;
    let mut skipped_empty = 0usize;

    for row in row_maps {
        match row_map_to_patent(&row, options) {
            Some(p) => {
                if matches_ipc_prefixes(&p, &prefixes) {
                    matched_patents.push(p);
                } else {
                    filtered_out += 1;
                }
            }
            None => skipped_empty += 1,
        }
    }

    Ok(ParsedTableStats {
        total_rows,
        matched_patents,
        filtered_out,
        skipped_empty,
    })
}

/// POST `/api/patents/import-table` — 网页一键上传 `.xlsx / .csv / .tsv / .json / .jsonl` 批量入库专利
pub async fn api_import_patent_table(
    State(s): State<AppState>,
    mut multipart: Multipart,
) -> Json<Value> {
    let mut filename = String::new();
    let mut file_bytes: Vec<u8> = Vec::new();
    let mut ipc_prefix = String::new();
    let mut lightweight = false;

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        if name == "file" {
            filename = field.file_name().unwrap_or("patents.csv").to_string();
            match field.bytes().await {
                Ok(b) => {
                    if b.len() > MAX_TABLE_BYTES {
                        return Json(json!({
                            "status": "error",
                            "message": "文件超过 30 MB 上限，请按年份或 IPC 拆分后导入"
                        }));
                    }
                    file_bytes = b.to_vec();
                }
                Err(e) => {
                    return Json(json!({
                        "status": "error",
                        "message": format!("读取上传文件失败: {e}")
                    }));
                }
            }
        } else if name == "ipc_prefix" {
            if let Ok(v) = field.text().await {
                ipc_prefix = v.trim().to_string();
            }
        } else if name == "lightweight" {
            if let Ok(v) = field.text().await {
                lightweight = matches!(v.trim().to_lowercase().as_str(), "1" | "true" | "yes");
            }
        }
    }

    if file_bytes.is_empty() {
        return Json(json!({
            "status": "error",
            "message": "未接收到专利表格文件，请选择 .xlsx / .csv / .json / .jsonl"
        }));
    }

    let options = PatentTableImportOptions {
        ipc_prefix,
        lightweight,
        source_tag: "table_import".to_string(),
    };
    let parse_filename = filename.clone();
    let parsed_res = tokio::task::spawn_blocking(move || {
        parse_patent_table_bytes(&parse_filename, &file_bytes, &options)
    })
    .await;

    let stats = match parsed_res {
        Ok(Ok(st)) => st,
        Ok(Err(msg)) => {
            return Json(json!({
                "status": "error",
                "message": msg
            }));
        }
        Err(e) => {
            return Json(json!({
                "status": "error",
                "message": format!("表格解析任务异常: {e}")
            }));
        }
    };

    if stats.matched_patents.is_empty() {
        return Json(json!({
            "status": "ok",
            "filename": filename,
            "total_rows": stats.total_rows,
            "imported": 0,
            "filtered_out": stats.filtered_out,
            "skipped_empty": stats.skipped_empty,
            "chunks": 0,
            "sample_numbers": [],
            "message": "表格未解析出符合条件的专利行（请检查表头是否包含公开号/专利号/发明名称或 IPC 过滤条件）"
        }));
    }

    let sample_numbers: Vec<String> = stats
        .matched_patents
        .iter()
        .take(5)
        .map(|p| {
            if p.patent_number.is_empty() {
                p.title.clone()
            } else {
                p.patent_number.clone()
            }
        })
        .collect();

    let db = s.db.clone();
    let patents = stats.matched_patents;
    let import_res = tokio::task::spawn_blocking(move || {
        db.batch_import_patents_with_chunks(&patents, "tfidf-local")
    })
    .await;

    match import_res {
        Ok(Ok((imported, chunks))) => Json(json!({
            "status": "ok",
            "filename": filename,
            "total_rows": stats.total_rows,
            "imported": imported,
            "filtered_out": stats.filtered_out,
            "skipped_empty": stats.skipped_empty,
            "chunks": chunks,
            "sample_numbers": sample_numbers
        })),
        Ok(Err(e)) => Json(json!({
            "status": "error",
            "message": format!("写入本地专利库失败: {e}")
        })),
        Err(e) => Json(json!({
            "status": "error",
            "message": format!("批量写入任务异常: {e}")
        })),
    }
}

fn list_syncable_table_files(dir: &Path) -> Result<Vec<(String, PathBuf, String)>, String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("无法读取目录: {e}"))?;
    let mut files = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        // 跳过 Excel 临时锁文件（~$xxx.xlsx）与隐藏文件
        if name.starts_with("~$") || name.starts_with('.') {
            continue;
        }
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        if !is_supported_table_ext(&ext) {
            continue;
        }
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        if meta.len() as usize > MAX_TABLE_BYTES {
            continue;
        }
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let sig = format!("{}:{}", meta.len(), mtime);
        files.push((name.to_string(), path, sig));
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    files.truncate(MAX_SYNC_FILES);
    Ok(files)
}

/// GET `/api/patents/sync-folder` — 获取当前已配置的本地/网盘专利同步目录状态
pub async fn api_get_patent_sync_folder(State(s): State<AppState>) -> Json<Value> {
    let folder_path =
        s.db.get_setting(SETTING_SYNC_FOLDER)
            .ok()
            .flatten()
            .unwrap_or_default();
    let ipc_prefix =
        s.db.get_setting(SETTING_SYNC_IPC_PREFIX)
            .ok()
            .flatten()
            .unwrap_or_default();
    let lightweight =
        s.db.get_setting(SETTING_SYNC_LIGHTWEIGHT)
            .ok()
            .flatten()
            .is_some_and(|v| v == "1" || v == "true");
    let last_at =
        s.db.get_setting(SETTING_SYNC_LAST_AT)
            .ok()
            .flatten()
            .unwrap_or_default();
    let last_summary =
        s.db.get_setting(SETTING_SYNC_LAST_SUMMARY)
            .ok()
            .flatten()
            .unwrap_or_default();

    let (exists, detected_files) = if folder_path.trim().is_empty() {
        (false, 0usize)
    } else {
        let p = PathBuf::from(folder_path.trim());
        if p.is_dir() {
            let count = list_syncable_table_files(&p).map(|v| v.len()).unwrap_or(0);
            (true, count)
        } else {
            (false, 0)
        }
    };

    Json(json!({
        "status": "ok",
        "folder_path": folder_path,
        "ipc_prefix": ipc_prefix,
        "lightweight": lightweight,
        "exists": exists,
        "detected_files": detected_files,
        "last_at": last_at,
        "last_summary": last_summary
    }))
}

/// POST `/api/patents/sync-folder` — 保存并增量同步本地/私人网盘同步目录中的专利表格
pub async fn api_sync_patent_folder(
    State(s): State<AppState>,
    Json(req): Json<SyncFolderRequest>,
) -> Json<Value> {
    let folder_trimmed = req.folder_path.trim().to_string();
    if folder_trimmed.is_empty() {
        let _ = s.db.set_settings_batch(&[
            (SETTING_SYNC_FOLDER, ""),
            (SETTING_SYNC_IPC_PREFIX, req.ipc_prefix.trim()),
            (
                SETTING_SYNC_LIGHTWEIGHT,
                if req.lightweight { "1" } else { "0" },
            ),
        ]);
        return Json(json!({
            "status": "ok",
            "folder_path": "",
            "message": "已清空同步目录配置"
        }));
    }

    let dir = PathBuf::from(&folder_trimmed);
    if !dir.exists() || !dir.is_dir() {
        return Json(json!({
            "status": "error",
            "message": format!("目录不存在或不是文件夹: {}", folder_trimmed)
        }));
    }

    let lw_flag = if req.lightweight { "1" } else { "0" };
    if let Err(e) = s.db.set_settings_batch(&[
        (SETTING_SYNC_FOLDER, &folder_trimmed),
        (SETTING_SYNC_IPC_PREFIX, req.ipc_prefix.trim()),
        (SETTING_SYNC_LIGHTWEIGHT, lw_flag),
    ]) {
        return Json(json!({
            "status": "error",
            "message": format!("保存同步目录配置失败: {e}")
        }));
    }

    if req.save_only {
        let detected = list_syncable_table_files(&dir)
            .map(|v| v.len())
            .unwrap_or(0);
        return Json(json!({
            "status": "ok",
            "folder_path": folder_trimmed,
            "detected_files": detected,
            "saved_only": true
        }));
    }

    let db = s.db.clone();
    let ipc_prefix = req.ipc_prefix.trim().to_string();
    let lightweight = req.lightweight;
    let force = req.force;

    let sync_res = tokio::task::spawn_blocking(move || -> Result<Value, String> {
        let files = list_syncable_table_files(&dir)?;
        let prev_manifest_raw = db
            .get_setting(SETTING_SYNC_MANIFEST)
            .ok()
            .flatten()
            .unwrap_or_default();
        let mut manifest: HashMap<String, String> =
            serde_json::from_str(&prev_manifest_raw).unwrap_or_default();

        let mut imported_files = 0usize;
        let mut skipped_unchanged_files = 0usize;
        let mut total_imported = 0usize;
        let mut total_filtered = 0usize;
        let mut total_chunks = 0usize;
        let mut details: Vec<FileSyncResult> = Vec::new();
        let mut errors: Vec<String> = Vec::new();

        let options = PatentTableImportOptions {
            ipc_prefix: ipc_prefix.clone(),
            lightweight,
            source_tag: "folder_sync".to_string(),
        };

        for (name, path, sig) in &files {
            let manifest_key = format!("{}|{}|{}", name, ipc_prefix, lightweight);
            if !force && manifest.get(&manifest_key).is_some_and(|old| old == sig) {
                skipped_unchanged_files += 1;
                details.push(FileSyncResult {
                    filename: name.clone(),
                    status: "skipped_unchanged".to_string(),
                    imported: 0,
                    filtered_out: 0,
                    chunks: 0,
                    message: "未变更，已跳过".to_string(),
                });
                continue;
            }

            let bytes = match std::fs::read(path) {
                Ok(b) => b,
                Err(e) => {
                    let msg = format!("{name}: 读取失败 ({e})");
                    errors.push(msg.clone());
                    details.push(FileSyncResult {
                        filename: name.clone(),
                        status: "error".to_string(),
                        imported: 0,
                        filtered_out: 0,
                        chunks: 0,
                        message: msg,
                    });
                    continue;
                }
            };

            match parse_patent_table_bytes(name, &bytes, &options) {
                Ok(st) => {
                    if st.matched_patents.is_empty() {
                        manifest.insert(manifest_key, sig.clone());
                        total_filtered += st.filtered_out;
                        details.push(FileSyncResult {
                            filename: name.clone(),
                            status: "empty".to_string(),
                            imported: 0,
                            filtered_out: st.filtered_out,
                            chunks: 0,
                            message: "无匹配专利行".to_string(),
                        });
                        continue;
                    }
                    match db.batch_import_patents_with_chunks(&st.matched_patents, "tfidf-local") {
                        Ok((imp, chunks)) => {
                            manifest.insert(manifest_key, sig.clone());
                            imported_files += 1;
                            total_imported += imp;
                            total_filtered += st.filtered_out;
                            total_chunks += chunks;
                            details.push(FileSyncResult {
                                filename: name.clone(),
                                status: "imported".to_string(),
                                imported: imp,
                                filtered_out: st.filtered_out,
                                chunks,
                                message: format!("导入 {imp} 件"),
                            });
                        }
                        Err(e) => {
                            let msg = format!("{name}: 入库失败 ({e})");
                            errors.push(msg.clone());
                            details.push(FileSyncResult {
                                filename: name.clone(),
                                status: "error".to_string(),
                                imported: 0,
                                filtered_out: st.filtered_out,
                                chunks: 0,
                                message: msg,
                            });
                        }
                    }
                }
                Err(e) => {
                    let msg = format!("{name}: {e}");
                    errors.push(msg.clone());
                    details.push(FileSyncResult {
                        filename: name.clone(),
                        status: "error".to_string(),
                        imported: 0,
                        filtered_out: 0,
                        chunks: 0,
                        message: msg,
                    });
                }
            }
        }

        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let summary = format!(
            "扫描 {} 个文件：导入 {} 个（共 {} 件专利），跳过未变更 {} 个",
            files.len(),
            imported_files,
            total_imported,
            skipped_unchanged_files
        );
        let manifest_json = serde_json::to_string(&manifest).unwrap_or_else(|_| "{}".to_string());
        let _ = db.set_settings_batch(&[
            (SETTING_SYNC_MANIFEST, &manifest_json),
            (SETTING_SYNC_LAST_AT, &now),
            (SETTING_SYNC_LAST_SUMMARY, &summary),
        ]);

        Ok(json!({
            "status": "ok",
            "folder_path": folder_trimmed,
            "scanned_files": files.len(),
            "imported_files": imported_files,
            "skipped_unchanged_files": skipped_unchanged_files,
            "imported_patents": total_imported,
            "filtered_out": total_filtered,
            "chunks": total_chunks,
            "last_at": now,
            "summary": summary,
            "errors": errors,
            "details": details
        }))
    })
    .await;

    match sync_res {
        Ok(Ok(val)) => Json(val),
        Ok(Err(msg)) => Json(json!({ "status": "error", "message": msg })),
        Err(e) => Json(json!({ "status": "error", "message": format!("同步任务异常: {e}") })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    fn build_test_xlsx_bytes() -> Vec<u8> {
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut cursor);
            let opts = SimpleFileOptions::default();

            let shared_strings = r#"<?xml version="1.0" encoding="UTF-8"?>
<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="7" uniqueCount="7">
  <si><t>公开（公告）号</t></si>
  <si><t>发明名称</t></si>
  <si><t>申请人</t></si>
  <si><t>IPC分类号</t></si>
  <si><t>公开（公告）日</t></si>
  <si><t>CN118000123A</t></si>
  <si><t>一种SiC功率模块双面水冷结构 &amp; 制造方法</t></si>
  <si><t>H01L23/473; H02M7/00</t></si>
</sst>"#;
            zip.start_file("xl/sharedStrings.xml", opts)
                .expect("start sharedStrings");
            zip.write_all(shared_strings.as_bytes())
                .expect("write sharedStrings");

            // 注意：第 2 行故意省略 C2（申请人为空单元格），验证 D2(IPC) 与 E2(日期序列号 45323 = 2024-02-01) 绝不列错位！
            let sheet1 = r#"<?xml version="1.0" encoding="UTF-8"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
  <sheetData>
    <row r="1">
      <c r="A1" t="s"><v>0</v></c>
      <c r="B1" t="s"><v>1</v></c>
      <c r="C1" t="s"><v>2</v></c>
      <c r="D1" t="s"><v>3</v></c>
      <c r="E1" t="s"><v>4</v></c>
    </row>
    <row r="2">
      <c r="A2" t="s"><v>5</v></c>
      <c r="B2" t="s"><v>6</v></c>
      <c r="D2" t="s"><v>7</v></c>
      <c r="E2"><v>45323</v></c>
    </row>
  </sheetData>
</worksheet>"#;
            zip.start_file("xl/worksheets/sheet1.xml", opts)
                .expect("start sheet1");
            zip.write_all(sheet1.as_bytes()).expect("write sheet1");
            zip.finish().expect("finish zip");
        }
        cursor.into_inner()
    }

    #[test]
    fn parse_xlsx_preserves_sparse_columns_and_excel_dates() {
        let bytes = build_test_xlsx_bytes();
        let stats = parse_patent_table_bytes(
            "cnipa_export.xlsx",
            &bytes,
            &PatentTableImportOptions::default(),
        )
        .expect("parse xlsx");
        assert_eq!(stats.matched_patents.len(), 1);
        let p = &stats.matched_patents[0];
        assert_eq!(p.patent_number, "CN118000123A");
        assert_eq!(p.title, "一种SiC功率模块双面水冷结构 & 制造方法");
        assert_eq!(p.applicant, "");
        assert!(p.ipc_codes.contains("H01L23/473"));
        assert_eq!(p.publication_date, "2024-02-01");
        assert_eq!(p.country, "CN");
    }

    #[test]
    fn parse_gbk_cnipa_csv_and_ipc_filter_and_lightweight() {
        let csv_utf8 = "公开（公告）号,发明名称,摘要,主分类号,申请人,权利要求\n\
CN118100001A,固态电池硫化物电解质界面层,\"含逗号,的摘要正文\",H01M10/0562,某新能源公司,1. 一种固态电池...\n\
CN118100002A,一种农业灌溉喷头,节水喷头结构,A01G25/02,某农机公司,1. 一种喷头...\n";
        let (gbk_bytes, _, _) = encoding_rs::GB18030.encode(csv_utf8);
        let opts = PatentTableImportOptions {
            ipc_prefix: "H01M".to_string(),
            lightweight: true,
            source_tag: "cnipa_export".to_string(),
        };
        let stats = parse_patent_table_bytes("export.csv", &gbk_bytes, &opts).expect("parse gbk");
        assert_eq!(stats.total_rows, 2);
        assert_eq!(stats.filtered_out, 1);
        assert_eq!(stats.matched_patents.len(), 1);
        let p = &stats.matched_patents[0];
        assert_eq!(p.patent_number, "CN118100001A");
        assert_eq!(p.abstract_text, "含逗号,的摘要正文");
        assert!(p.claims.is_empty(), "lightweight 模式应跳过长权利要求");
    }

    #[test]
    fn batch_import_from_table_preserves_existing_enriched_fulltext() {
        let db = Database::init(":memory:").expect("init db");
        // 先存入一条已经 Enrich 过完整权利要求、说明书和附图的存量专利
        let enriched = Patent {
            id: "existing-enriched-1".to_string(),
            patent_number: "CN118000123A".to_string(),
            title: "一种SiC功率模块双面水冷结构".to_string(),
            abstract_text: "原始详细中文摘要内容".to_string(),
            description: "完整说明书实施例段落：采用双面微通道液冷板...".to_string(),
            claims: "1. 一种SiC功率模块双面水冷结构，其特征在于包括上冷板与下冷板。".to_string(),
            applicant: "原申请人公司".to_string(),
            inventor: "张三".to_string(),
            filing_date: "2023-08-01".to_string(),
            publication_date: "2024-02-01".to_string(),
            grant_date: None,
            ipc_codes: "[\"H01L23/473\"]".to_string(),
            cpc_codes: "[]".to_string(),
            priority_date: String::new(),
            country: "CN".to_string(),
            kind_code: String::new(),
            family_id: None,
            legal_status: "审中".to_string(),
            citations: "[\"US10000001B2\"]".to_string(),
            cited_by: "[]".to_string(),
            source: "google_patents".to_string(),
            raw_json: String::new(),
            images: "[\"data:image/png;base64,AAAA\"]".to_string(),
            pdf_url: "https://example.com/cn118000123a.pdf".to_string(),
            created_at: String::new(),
        };
        db.insert_patent(&enriched).expect("insert enriched");

        // 再次用轻量国知局导出表（claims/description/images 均为空）批量导入同公开号
        let xlsx_bytes = build_test_xlsx_bytes();
        let stats = parse_patent_table_bytes(
            "cnipa_export.xlsx",
            &xlsx_bytes,
            &PatentTableImportOptions::default(),
        )
        .expect("parse xlsx");
        let (imported, _chunks) = db
            .batch_import_patents_with_chunks(&stats.matched_patents, "tfidf-local")
            .expect("batch import");
        assert_eq!(imported, 1);

        let after = db
            .get_patent("existing-enriched-1")
            .expect("get patent")
            .expect("patent still exists under same id");
        assert!(
            after.claims.contains("上冷板与下冷板"),
            "批量轻量导入绝不能覆盖已有权利要求"
        );
        assert!(
            after.description.contains("双面微通道液冷板"),
            "批量轻量导入绝不能覆盖已有说明书"
        );
        assert!(
            after.images.contains("data:image/png"),
            "批量轻量导入绝不能覆盖已有附图"
        );
        assert_eq!(after.pdf_url, "https://example.com/cn118000123a.pdf");
    }
}
