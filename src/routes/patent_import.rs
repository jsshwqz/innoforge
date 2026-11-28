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
use std::io::{BufRead, BufReader, Cursor, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Instant, UNIX_EPOCH};

const SETTING_SYNC_FOLDER: &str = "patent_sync_folder";
const SETTING_SYNC_IPC_PREFIX: &str = "patent_sync_ipc_prefix";
const SETTING_SYNC_LIGHTWEIGHT: &str = "patent_sync_lightweight";
const SETTING_SYNC_LAST_AT: &str = "patent_sync_last_at";
const SETTING_SYNC_LAST_SUMMARY: &str = "patent_sync_last_summary";
const SETTING_SYNC_MANIFEST: &str = "patent_sync_file_manifest";
const SETTING_SYNC_AUTO_INTERVAL_MINS: &str = "patent_sync_auto_interval_mins";

const MAX_TABLE_BYTES: usize = 30 * 1024 * 1024; // 30 MB（用于单文件内存解析：XLSX/DOC/ZIP 等）
const MAX_STREAM_TABLE_BYTES: u64 = 250 * 1024 * 1024; // 250 MB（用于流式读取 JSONL/CSV/TSV）
const MAX_SYNC_FILES: usize = 500;
const MAX_WALK_DEPTH: usize = 6;
const MAX_ZIP_ENTRIES: usize = 100;
const MAX_ZIP_TOTAL_UNCOMPRESSED: u64 = 100 * 1024 * 1024; // 100 MB 防压缩炸弹
const STREAM_BATCH_ROWS: usize = 500;
const MIN_FREE_DISK_MB: u64 = 1024; // 低于 1 GB 自动暂停同步保护硬盘

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
    #[serde(default)]
    pub async_mode: bool,
    #[serde(default)]
    pub auto_interval_mins: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderSyncJobSnapshot {
    pub state: String,
    pub folder_path: String,
    pub resolved_path: Option<String>,
    pub current_file: String,
    pub files_done: usize,
    pub files_total: usize,
    pub deduped_pair_files: usize,
    pub table_files: usize,
    pub doc_files: usize,
    pub archive_files: usize,
    pub imported_files: usize,
    pub skipped_unchanged_files: usize,
    pub imported_patents: usize,
    pub filtered_out: usize,
    pub chunks: usize,
    pub rows_per_sec: usize,
    pub free_disk_mb: Option<u64>,
    pub elapsed_secs: u64,
    pub sample_numbers: Vec<String>,
    pub summary: String,
    pub errors: Vec<String>,
}

impl Default for FolderSyncJobSnapshot {
    fn default() -> Self {
        Self {
            state: "idle".to_string(),
            folder_path: String::new(),
            resolved_path: None,
            current_file: String::new(),
            files_done: 0,
            files_total: 0,
            deduped_pair_files: 0,
            table_files: 0,
            doc_files: 0,
            archive_files: 0,
            imported_files: 0,
            skipped_unchanged_files: 0,
            imported_patents: 0,
            filtered_out: 0,
            chunks: 0,
            rows_per_sec: 0,
            free_disk_mb: None,
            elapsed_secs: 0,
            sample_numbers: Vec::new(),
            summary: String::new(),
            errors: Vec::new(),
        }
    }
}

struct FolderSyncJobManager {
    snapshot: FolderSyncJobSnapshot,
    cancel_flag: Arc<AtomicBool>,
}

impl Default for FolderSyncJobManager {
    fn default() -> Self {
        Self {
            snapshot: FolderSyncJobSnapshot::default(),
            cancel_flag: Arc::new(AtomicBool::new(false)),
        }
    }
}

static FOLDER_SYNC_MANAGER: OnceLock<Mutex<FolderSyncJobManager>> = OnceLock::new();

fn sync_manager() -> &'static Mutex<FolderSyncJobManager> {
    FOLDER_SYNC_MANAGER.get_or_init(|| Mutex::new(FolderSyncJobManager::default()))
}

fn current_sync_snapshot() -> FolderSyncJobSnapshot {
    sync_manager()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .snapshot
        .clone()
}

fn update_sync_snapshot(mutator: impl FnOnce(&mut FolderSyncJobSnapshot)) {
    let mut guard = sync_manager().lock().unwrap_or_else(|e| e.into_inner());
    mutator(&mut guard.snapshot);
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SyncFileKind {
    Table,
    Doc,
    Archive,
}

type SyncFileEntry = (String, PathBuf, String, SyncFileKind);

pub(crate) fn table_ext_for_filename(filename: &str) -> Option<(&'static str, bool)> {
    let lower = filename.to_ascii_lowercase();
    if lower.ends_with(".jsonl.gz") {
        return Some(("jsonl", true));
    }
    if lower.ends_with(".csv.gz") {
        return Some(("csv", true));
    }
    if lower.ends_with(".tsv.gz") {
        return Some(("tsv", true));
    }
    if lower.ends_with(".json.gz") {
        return Some(("json", true));
    }
    let ext = Path::new(&lower)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    match ext {
        "xlsx" => Some(("xlsx", false)),
        "csv" => Some(("csv", false)),
        "tsv" => Some(("tsv", false)),
        "json" => Some(("json", false)),
        "jsonl" => Some(("jsonl", false)),
        _ => None,
    }
}

fn is_supported_doc_ext(ext_lower: &str) -> bool {
    matches!(
        ext_lower,
        "pdf" | "ofd" | "docx" | "txt" | "md" | "xml" | "html" | "htm" | "rtf" | "tex"
    )
}

fn is_supported_archive_ext(ext_lower: &str) -> bool {
    ext_lower == "zip"
}

fn classify_sync_ext(ext_lower: &str) -> Option<SyncFileKind> {
    if matches!(ext_lower, "xlsx" | "csv" | "tsv" | "json" | "jsonl") {
        Some(SyncFileKind::Table)
    } else if is_supported_doc_ext(ext_lower) {
        Some(SyncFileKind::Doc)
    } else if is_supported_archive_ext(ext_lower) {
        Some(SyncFileKind::Archive)
    } else {
        None
    }
}

fn classify_sync_filename(filename: &str) -> Option<SyncFileKind> {
    if table_ext_for_filename(filename).is_some() {
        return Some(SyncFileKind::Table);
    }
    let ext = Path::new(filename)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    classify_sync_ext(&ext)
}

fn is_temp_download_filename(name: &str) -> bool {
    if name.starts_with("~$") || name.starts_with('.') {
        return true;
    }
    let lower = name.to_ascii_lowercase();
    // 跳过采集器状态/清单/凭证与系统元信息 JSON（防止 STATUS_v2.json 秒级刷新反复触发自动同步，且绝不触碰凭证文件）
    if lower.ends_with(".json")
        && (lower.starts_with("status")
            || lower.starts_with("manifest")
            || lower.starts_with("ftp_accounts")
            || lower.starts_with("innoforge-feed")
            || lower.starts_with('_')
            || matches!(
                lower.as_str(),
                "package.json"
                    | "package-lock.json"
                    | "tsconfig.json"
                    | "config.json"
                    | "settings.json"
            ))
    {
        return true;
    }
    if [
        ".part",
        ".tmp",
        ".temp",
        ".crdownload",
        ".downloading",
        ".download",
        ".aria2",
        ".!qb",
        ".!ut",
        ".filepart",
    ]
    .iter()
    .any(|suffix| lower.ends_with(suffix))
    {
        return true;
    }
    if let Some(ext) = lower.rsplit('.').next() {
        let mut chars = ext.chars();
        if chars.next() == Some('z')
            && matches!(chars.next(), Some(c) if c.is_ascii_digit())
            && matches!(chars.next(), Some(c) if c.is_ascii_digit())
            && chars.next().is_none()
        {
            return true;
        }
    }
    false
}

/// 清理用户粘贴路径两侧可能附带的引号与空白（例如 Windows「复制文件地址」产生的 `"F:\down\专利数据"`）
fn clean_folder_path_input(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.len() >= 2
        && ((trimmed.starts_with('"') && trimmed.ends_with('"'))
            || (trimmed.starts_with('\'') && trimmed.ends_with('\'')))
    {
        trimmed
            .chars()
            .skip(1)
            .take(trimmed.chars().count().saturating_sub(2))
            .collect::<String>()
            .trim()
            .to_string()
    } else {
        trimmed.to_string()
    }
}

/// 将 Windows 盘符路径（`F:\down\专利数据` / `F:/down/专利数据`）或 WSL UNC 路径
/// （`\\wsl$\Ubuntu\...` / `\\wsl.localhost\Ubuntu\...`）转换为 WSL `/mnt/<drive>/...` 候选路径。
pub fn windows_path_to_wsl_candidate(raw: &str) -> Option<PathBuf> {
    let cleaned = clean_folder_path_input(raw);
    let mut chars = cleaned.chars();
    let c0 = chars.next()?;
    let c1 = chars.next()?;

    // 1. 盘符路径：F:\down\专利数据 或 F:/down/专利数据 或 F:
    if c0.is_ascii_alphabetic() && c1 == ':' {
        let rest_str: String = chars.collect();
        if !rest_str.is_empty() && !rest_str.starts_with('\\') && !rest_str.starts_with('/') {
            return None;
        }
        let drive = c0.to_ascii_lowercase();
        let norm_rest = rest_str.replace('\\', "/");
        let rel = norm_rest.trim_matches('/');
        return if rel.is_empty() {
            Some(PathBuf::from(format!("/mnt/{drive}")))
        } else {
            Some(PathBuf::from(format!("/mnt/{drive}/{rel}")))
        };
    }

    // 2. WSL UNC 路径：\\wsl$\<distro>\... 或 \\wsl.localhost\<distro>\...
    let lower = cleaned.to_ascii_lowercase();
    let unc_prefix_len = if lower.starts_with(r"\\wsl$\") {
        7
    } else if lower.starts_with(r"\\wsl.localhost\") {
        16
    } else {
        0
    };
    if unc_prefix_len > 0 {
        let after_prefix: String = cleaned.chars().skip(unc_prefix_len).collect();
        let mut parts = after_prefix.splitn(2, ['\\', '/']);
        let _distro = parts.next()?;
        let linux_rel = parts.next().unwrap_or("").replace('\\', "/");
        let rel = linux_rel.trim_start_matches('/');
        return Some(PathBuf::from(format!("/{rel}")));
    }

    None
}

/// 解析本机或网盘同步目录路径：
/// - 优先按原路径检查；
/// - 在 Linux/WSL 下若用户输入 Windows 盘符路径（如 `F:\down\专利数据`），自动映射为 `/mnt/f/down/专利数据`。
///
/// 返回 `(解析后的实际路径, 若发生了自动映射则为映射后路径文本)`。
pub fn resolve_local_folder_path(raw: &str) -> (PathBuf, Option<String>) {
    let cleaned = clean_folder_path_input(raw);
    let direct = PathBuf::from(&cleaned);
    if direct.is_dir() {
        return (direct, None);
    }
    #[cfg(unix)]
    if let Some(wsl_candidate) = windows_path_to_wsl_candidate(&cleaned) {
        let display = wsl_candidate.to_string_lossy().to_string();
        return (wsl_candidate, Some(display));
    }
    (direct, None)
}

/// 校验可选 IPC 赛道过滤栏，拦截把本地文件夹路径误填进 IPC 过滤栏（否则会静默过滤掉 100% 专利）
pub fn validate_ipc_prefix_filter(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }
    let looks_like_drive = {
        let mut chs = trimmed.chars();
        matches!((chs.next(), chs.next()), (Some(c), Some(':')) if c.is_ascii_alphabetic())
    };
    let looks_like_path = looks_like_drive
        || trimmed.contains('\\')
        || trimmed.starts_with('/')
        || trimmed.starts_with("~/")
        || trimmed.contains("://");
    if looks_like_path {
        return Err(format!(
            "左侧「关键词/领域过滤」栏误填了路径（“{trimmed}”），请清空该栏或填写中文关键词/领域，文件夹路径请填在右侧「同步目录」框中"
        ));
    }
    Ok(trimmed.to_string())
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
    let base_source = if options.source_tag.trim().is_empty() {
        "table_import"
    } else {
        options.source_tag.trim()
    };
    let source = if options.lightweight && !base_source.ends_with("_lightweight") {
        format!("{base_source}_lightweight")
    } else {
        base_source.to_string()
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

#[derive(Debug, Clone, PartialEq, Eq)]
enum ImportFilterRule {
    /// IPC / CPC 分类号前缀（如 `F`、`F23`、`A61G`、`H01L23/473`），按独立分类号前缀匹配防 `F` 误命中 `G06F`
    IpcPrefix(String),
    /// 大白话中文/英文技术关键词（如 `煤粉`、`热风炉`、`氧舱`、`SiC`），按标题/摘要/权利要求/申请人子串匹配
    Keyword(String),
}

fn is_ipc_code_token(raw_token: &str) -> bool {
    let trimmed = raw_token.trim();
    if trimmed.is_empty() || !trimmed.is_ascii() {
        return false;
    }
    let bytes = trimmed.as_bytes();
    // 单字母仅接受大写 A-H 作为 IPC 部级大类（避免普通小写单字母被误判）
    if bytes.len() == 1 {
        return matches!(bytes[0], b'A'..=b'H');
    }
    // 标准 IPC/CPC 形态：[A-H][0-9]{2}[A-Z0-9/]*
    let upper = trimmed.to_ascii_uppercase();
    let ub = upper.as_bytes();
    ub.len() >= 3
        && matches!(ub[0], b'A'..=b'H')
        && ub[1].is_ascii_digit()
        && ub[2].is_ascii_digit()
        && ub.iter().all(|b| b.is_ascii_alphanumeric() || *b == b'/')
}

fn parse_ipc_prefixes(raw: &str) -> Vec<ImportFilterRule> {
    raw.split([',', '，', ';', '；', '、', ' ', '\t', '\n', '\r'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|token| {
            if is_ipc_code_token(token) {
                ImportFilterRule::IpcPrefix(token.to_ascii_uppercase())
            } else {
                ImportFilterRule::Keyword(token.to_lowercase())
            }
        })
        .collect()
}

fn extract_code_tokens(raw_codes: &str) -> Vec<String> {
    raw_codes
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '/')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_ascii_uppercase())
        .collect()
}

fn matches_ipc_prefixes(patent: &Patent, rules: &[ImportFilterRule]) -> bool {
    if rules.is_empty() {
        return true;
    }
    let has_ipc_rule = rules
        .iter()
        .any(|r| matches!(r, ImportFilterRule::IpcPrefix(_)));
    let has_kw_rule = rules
        .iter()
        .any(|r| matches!(r, ImportFilterRule::Keyword(_)));

    let code_tokens = if has_ipc_rule {
        let mut codes = extract_code_tokens(&patent.ipc_codes);
        codes.extend(extract_code_tokens(&patent.cpc_codes));
        codes
    } else {
        Vec::new()
    };

    let text_hay = if has_kw_rule {
        format!(
            "{} {} {} {}",
            patent.title, patent.abstract_text, patent.claims, patent.applicant
        )
        .to_lowercase()
    } else {
        String::new()
    };

    rules.iter().any(|rule| match rule {
        ImportFilterRule::IpcPrefix(prefix) => code_tokens
            .iter()
            .any(|code| code.starts_with(prefix.as_str())),
        ImportFilterRule::Keyword(kw) => text_hay.contains(kw.as_str()),
    })
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
    let Some((base_ext, is_gz)) = table_ext_for_filename(filename) else {
        let ext = Path::new(filename)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        return Err(format!(
            "不支持的文件格式 `.{ext}`，请上传 .xlsx / .csv / .tsv / .json / .jsonl（支持 .gz 压缩）"
        ));
    };

    let decompressed_owned: Vec<u8>;
    let payload: &[u8] = if is_gz {
        let mut decoder = flate2::read::GzDecoder::new(Cursor::new(data));
        let mut buf = Vec::new();
        decoder
            .by_ref()
            .take(MAX_ZIP_TOTAL_UNCOMPRESSED + 1)
            .read_to_end(&mut buf)
            .map_err(|e| format!("Gzip 解压失败: {e}"))?;
        if buf.len() as u64 > MAX_ZIP_TOTAL_UNCOMPRESSED {
            return Err("Gzip 解压后体积超过 100 MB 单文件安全上限，请改用目录流式同步".into());
        }
        decompressed_owned = buf;
        &decompressed_owned
    } else {
        data
    };

    let row_maps: Vec<HashMap<String, String>> = match base_ext {
        "xlsx" => matrix_to_row_maps(parse_xlsx_rows(payload)?),
        "csv" => matrix_to_row_maps(parse_delimited_rows(&decode_text_bytes(payload), false)),
        "tsv" => matrix_to_row_maps(parse_delimited_rows(&decode_text_bytes(payload), true)),
        "json" => {
            let text = decode_text_bytes(payload);
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
            let text = decode_text_bytes(payload);
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

/// 统一解析任意受支持的专利/文献文件字节（表格 `.xlsx/.csv/.tsv/.json/.jsonl/.jsonl.gz/.csv.gz`、全文文档 `.pdf/.ofd/.docx/.txt/.md/.xml/.html/.rtf/.tex`、压缩包 `.zip`）。
pub fn parse_any_patent_bytes(
    filename: &str,
    data: &[u8],
    options: &PatentTableImportOptions,
) -> Result<ParsedTableStats, String> {
    match classify_sync_filename(filename) {
        Some(SyncFileKind::Table) => parse_patent_table_bytes(filename, data, options),
        Some(SyncFileKind::Doc) => parse_single_doc_bytes(filename, data, options),
        Some(SyncFileKind::Archive) => parse_zip_archive_bytes(filename, data, options),
        None => {
            let ext = Path::new(filename)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            Err(format!(
                "不支持的文件格式 `.{ext}`，支持表格(.xlsx/.csv/.tsv/.json/.jsonl/.gz)、全文文档(.pdf/.ofd/.docx/.txt/.md/.xml)与压缩包(.zip)"
            ))
        }
    }
}

fn parse_single_doc_bytes(
    filename: &str,
    data: &[u8],
    options: &PatentTableImportOptions,
) -> Result<ParsedTableStats, String> {
    let text = crate::routes::upload::extract_fast_doc_text_sync(filename, data)?;
    let (mut patent, _doc_kind, _kw) =
        crate::routes::upload::build_local_patent_record(filename, &text);
    if !options.source_tag.is_empty() && patent.source.starts_with("local_") {
        patent.source = format!("{}_{}", options.source_tag, patent.source);
    }
    let prefixes = parse_ipc_prefixes(&options.ipc_prefix);
    if !matches_ipc_prefixes(&patent, &prefixes) {
        return Ok(ParsedTableStats {
            total_rows: 1,
            matched_patents: Vec::new(),
            filtered_out: 1,
            skipped_empty: 0,
        });
    }
    Ok(ParsedTableStats {
        total_rows: 1,
        matched_patents: vec![patent],
        filtered_out: 0,
        skipped_empty: 0,
    })
}

fn parse_zip_archive_bytes(
    _archive_name: &str,
    data: &[u8],
    options: &PatentTableImportOptions,
) -> Result<ParsedTableStats, String> {
    let mut archive =
        zip::ZipArchive::new(Cursor::new(data)).map_err(|e| format!("无法打开 ZIP 压缩包: {e}"))?;

    let mut total_rows = 0usize;
    let mut matched_patents = Vec::new();
    let mut filtered_out = 0usize;
    let mut skipped_empty = 0usize;
    let mut total_uncompressed: u64 = 0;
    let mut processed_entries = 0usize;
    let mut first_err: Option<String> = None;

    for i in 0..archive.len() {
        if processed_entries >= MAX_ZIP_ENTRIES {
            break;
        }
        let mut entry = match archive.by_index(i) {
            Ok(f) => f,
            Err(zip::result::ZipError::UnsupportedArchive(msg))
                if msg.to_lowercase().contains("encrypt")
                    || msg.to_lowercase().contains("password") =>
            {
                return Err("检测到加密 ZIP（含密码保护），暂不支持直接解压导入".into());
            }
            Err(e) => {
                if first_err.is_none() {
                    first_err = Some(format!("ZIP 条目读取失败: {e}"));
                }
                continue;
            }
        };
        if entry.is_dir() {
            continue;
        }
        let entry_name = entry.name().to_string();
        if entry_name.starts_with("__MACOSX/")
            || entry_name.contains("/__MACOSX/")
            || entry_name
                .rsplit(['/', '\\'])
                .next()
                .is_some_and(|base| base.starts_with('.') || base.starts_with("~$"))
        {
            continue;
        }
        let ext = Path::new(&entry_name)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        // 禁止递归解压嵌套 ZIP 防炸弹；支持内嵌表格与专利/论文全文文档
        let Some(kind) = classify_sync_ext(&ext) else {
            continue;
        };
        if kind == SyncFileKind::Archive {
            continue;
        }
        if entry.size() > MAX_TABLE_BYTES as u64 {
            continue;
        }
        total_uncompressed = total_uncompressed.saturating_add(entry.size());
        if total_uncompressed > MAX_ZIP_TOTAL_UNCOMPRESSED {
            break;
        }
        let mut buf = Vec::new();
        if entry
            .by_ref()
            .take((MAX_TABLE_BYTES as u64) + 1)
            .read_to_end(&mut buf)
            .is_err()
        {
            continue;
        }
        if buf.len() > MAX_TABLE_BYTES || buf.is_empty() {
            continue;
        }
        processed_entries += 1;
        let res = match kind {
            SyncFileKind::Table => parse_patent_table_bytes(&entry_name, &buf, options),
            SyncFileKind::Doc => parse_single_doc_bytes(&entry_name, &buf, options),
            SyncFileKind::Archive => continue,
        };
        match res {
            Ok(st) => {
                total_rows += st.total_rows;
                matched_patents.extend(st.matched_patents);
                filtered_out += st.filtered_out;
                skipped_empty += st.skipped_empty;
            }
            Err(e) => {
                if first_err.is_none() {
                    first_err = Some(format!("{entry_name}: {e}"));
                }
            }
        }
    }

    if processed_entries == 0 {
        if let Some(err) = first_err {
            return Err(err);
        }
        return Err("ZIP 压缩包内未找到可识别的专利表或 PDF/OFD/DOCX/TXT 文档".into());
    }
    if matched_patents.is_empty() && filtered_out == 0 {
        if let Some(err) = first_err {
            return Err(err);
        }
    }

    Ok(ParsedTableStats {
        total_rows,
        matched_patents,
        filtered_out,
        skipped_empty,
    })
}

/// POST `/api/patents/import-table` — 网页一键上传单个或多个专利表 / PDF / OFD / DOCX / ZIP（或整个本地文件夹分批上传）批量入库
pub async fn api_import_patent_table(
    State(s): State<AppState>,
    mut multipart: Multipart,
) -> Json<Value> {
    let mut uploaded_files: Vec<(String, Vec<u8>)> = Vec::new();
    let mut ipc_prefix = String::new();
    let mut lightweight = false;

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        if name == "file" || name == "files" {
            let filename = field.file_name().unwrap_or("patents.csv").to_string();
            match field.bytes().await {
                Ok(b) => {
                    if b.len() > MAX_TABLE_BYTES {
                        return Json(json!({
                            "status": "error",
                            "message": format!("文件 {filename} 超过 30 MB 上限，请拆分后导入")
                        }));
                    }
                    if !b.is_empty() {
                        uploaded_files.push((filename, b.to_vec()));
                    }
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

    let validated_ipc = match validate_ipc_prefix_filter(&ipc_prefix) {
        Ok(v) => v,
        Err(msg) => {
            return Json(json!({
                "status": "error",
                "message": msg
            }));
        }
    };

    if uploaded_files.is_empty() {
        return Json(json!({
            "status": "error",
            "message": "未接收到可导入文件，请选择 .xlsx / .csv / .json / .pdf / .ofd / .docx / .zip"
        }));
    }

    let first_filename = uploaded_files
        .first()
        .map(|(n, _)| n.clone())
        .unwrap_or_default();
    let files_count = uploaded_files.len();
    let options = PatentTableImportOptions {
        ipc_prefix: validated_ipc,
        lightweight,
        source_tag: "table_import".to_string(),
    };

    let parsed_res = tokio::task::spawn_blocking(move || {
        let mut total_rows = 0usize;
        let mut matched_patents: Vec<Patent> = Vec::new();
        let mut filtered_out = 0usize;
        let mut skipped_empty = 0usize;
        let mut errors: Vec<String> = Vec::new();
        let mut files_ok = 0usize;

        for (fname, bytes) in uploaded_files {
            match parse_any_patent_bytes(&fname, &bytes, &options) {
                Ok(st) => {
                    files_ok += 1;
                    total_rows += st.total_rows;
                    matched_patents.extend(st.matched_patents);
                    filtered_out += st.filtered_out;
                    skipped_empty += st.skipped_empty;
                }
                Err(msg) => {
                    errors.push(format!("{fname}: {msg}"));
                }
            }
        }
        (
            ParsedTableStats {
                total_rows,
                matched_patents,
                filtered_out,
                skipped_empty,
            },
            files_ok,
            errors,
        )
    })
    .await;

    let (stats, files_ok, errors) = match parsed_res {
        Ok(res) => res,
        Err(e) => {
            return Json(json!({
                "status": "error",
                "message": format!("批量解析任务异常: {e}")
            }));
        }
    };

    if files_ok == 0 && !errors.is_empty() {
        return Json(json!({
            "status": "error",
            "message": errors[0],
            "errors": errors
        }));
    }

    if stats.matched_patents.is_empty() {
        return Json(json!({
            "status": "ok",
            "filename": first_filename,
            "files_processed": files_count,
            "total_rows": stats.total_rows,
            "imported": 0,
            "filtered_out": stats.filtered_out,
            "skipped_empty": stats.skipped_empty,
            "chunks": 0,
            "sample_numbers": [],
            "errors": errors,
            "message": "未解析出符合条件的专利/文献（请检查文件格式或 IPC 过滤条件）"
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
        db.batch_import_patents_chunked(&patents, lightweight, "char-tfidf-v2", None)
    })
    .await;

    match import_res {
        Ok(Ok((imported, chunks, _))) => Json(json!({
            "status": "ok",
            "filename": first_filename,
            "files_processed": files_count,
            "total_rows": stats.total_rows,
            "imported": imported,
            "filtered_out": stats.filtered_out,
            "skipped_empty": stats.skipped_empty,
            "chunks": chunks,
            "sample_numbers": sample_numbers,
            "errors": errors
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

/// 查询目标路径所在磁盘的可用空间（MB）；用于 GB 级同步前与同步中实时护栏，防撑爆硬盘。
pub fn free_disk_mb_for_path(path: &Path) -> Option<u64> {
    let check_dir = if path.is_dir() {
        path
    } else {
        path.parent().unwrap_or(path)
    };
    #[cfg(unix)]
    {
        let out = std::process::Command::new("df")
            .arg("-Pk")
            .arg(check_dir)
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        let stdout = String::from_utf8_lossy(&out.stdout);
        let data_line = stdout.lines().nth(1)?;
        let cols: Vec<&str> = data_line.split_whitespace().collect();
        // POSIX `df -Pk` 列序：Filesystem 1024-blocks Used Available Capacity Mounted
        let avail_kb: u64 = cols.get(3)?.parse().ok()?;
        Some(avail_kb / 1024)
    }
    #[cfg(not(unix))]
    {
        let _ = check_dir;
        None
    }
}

fn free_disk_mb_for_db(db: &crate::db::Database) -> Option<u64> {
    let db_path = db.main_db_file_path()?;
    free_disk_mb_for_path(&db_path)
}

pub(crate) fn should_skip_sync_dir(dir_name: &str) -> bool {
    let lower = dir_name.to_ascii_lowercase();
    dir_name.starts_with('.')
        || lower.starts_with("work_")
        || matches!(
            lower.as_str(),
            "__macosx"
                | "node_modules"
                | "$recycle.bin"
                | "system volume information"
                | "__pycache__"
                | "tmp"
                | "_tmp"
                | "temp"
        )
}

fn max_sync_file_bytes_for_filename(filename: &str) -> u64 {
    if matches!(
        table_ext_for_filename(filename),
        Some(("jsonl" | "csv" | "tsv", _))
    ) {
        MAX_STREAM_TABLE_BYTES
    } else {
        MAX_TABLE_BYTES as u64
    }
}

fn walk_syncable_files(
    root: &Path,
    dir: &Path,
    depth: usize,
    out: &mut Vec<SyncFileEntry>,
) -> Result<(), String> {
    if depth > MAX_WALK_DEPTH || out.len() >= MAX_SYNC_FILES {
        return Ok(());
    }
    let entries = std::fs::read_dir(dir).map_err(|e| format!("无法读取目录: {e}"))?;
    let mut subdirs: Vec<PathBuf> = Vec::new();
    for entry in entries.flatten() {
        if out.len() >= MAX_SYNC_FILES {
            break;
        }
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if path.is_dir() {
            if depth < MAX_WALK_DEPTH && !should_skip_sync_dir(name) {
                subdirs.push(path);
            }
            continue;
        }
        if !path.is_file() {
            continue;
        }
        // 跳过 Excel 临时锁文件、隐藏文件与下载中未完结临时分卷（.part/.tmp/.crdownload/.z01 等）
        if is_temp_download_filename(name) {
            continue;
        }
        let Some(kind) = classify_sync_filename(name) else {
            continue;
        };
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        if meta.len() == 0 || meta.len() > max_sync_file_bytes_for_filename(name) {
            continue;
        }
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let sig = format!("{}:{}", meta.len(), mtime);
        let rel = path
            .strip_prefix(root)
            .ok()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_else(|| name.to_string());
        out.push((rel, path, sig, kind));
    }
    subdirs.sort();
    for sub in subdirs {
        if out.len() >= MAX_SYNC_FILES {
            break;
        }
        let _ = walk_syncable_files(root, &sub, depth + 1, out);
    }
    Ok(())
}

/// 解析同前缀表格变体的词干与优先级：`.jsonl.gz` (4) > `.jsonl` (3) > `.csv.gz` (2) > `.csv` (1)。
pub(crate) fn table_dedup_stem_and_rank(rel_path: &str) -> Option<(String, u8)> {
    let lower = rel_path.to_ascii_lowercase();
    if let Some(stem) = lower.strip_suffix(".jsonl.gz") {
        Some((stem.to_string(), 4))
    } else if let Some(stem) = lower.strip_suffix(".jsonl") {
        Some((stem.to_string(), 3))
    } else if let Some(stem) = lower.strip_suffix(".csv.gz") {
        Some((stem.to_string(), 2))
    } else {
        lower.strip_suffix(".csv").map(|stem| (stem.to_string(), 1))
    }
}

/// 同目录下若同时存在同前缀的 `.jsonl.gz` / `.jsonl` / `.csv.gz` / `.csv`（如国知局成对导出或压缩归档），
/// 自动保留最高优先级的一份（`.jsonl.gz` > `.jsonl` > `.csv.gz` > `.csv`），立省 50%+ I/O 与写入耗时。
fn dedup_sibling_table_pairs(files: Vec<SyncFileEntry>) -> (Vec<SyncFileEntry>, usize) {
    let mut best_rank: HashMap<String, u8> = HashMap::new();
    for (rel, _, _, kind) in &files {
        if *kind == SyncFileKind::Table {
            if let Some((stem, rank)) = table_dedup_stem_and_rank(rel) {
                let entry = best_rank.entry(stem).or_insert(0);
                if rank > *entry {
                    *entry = rank;
                }
            }
        }
    }
    if best_rank.is_empty() {
        return (files, 0);
    }
    let mut kept = Vec::with_capacity(files.len());
    let mut deduped = 0usize;
    for item in files {
        if item.3 == SyncFileKind::Table {
            if let Some((stem, rank)) = table_dedup_stem_and_rank(&item.0) {
                if best_rank.get(&stem).is_some_and(|best| rank < *best) {
                    deduped += 1;
                    continue;
                }
            }
        }
        kept.push(item);
    }
    (kept, deduped)
}

fn list_syncable_files_recursive_dedup(dir: &Path) -> Result<(Vec<SyncFileEntry>, usize), String> {
    let mut files = Vec::new();
    walk_syncable_files(dir, dir, 0, &mut files)?;
    files.sort_by(|a, b| a.0.cmp(&b.0));
    let (mut deduped_files, deduped_pairs) = dedup_sibling_table_pairs(files);
    deduped_files.truncate(MAX_SYNC_FILES);
    Ok((deduped_files, deduped_pairs))
}

#[cfg(test)]
fn list_syncable_files_recursive(dir: &Path) -> Result<Vec<SyncFileEntry>, String> {
    list_syncable_files_recursive_dedup(dir).map(|(files, _)| files)
}

pub(crate) struct StreamFileSyncOutcome {
    pub(crate) imported: usize,
    pub(crate) filtered_out: usize,
    pub(crate) chunks: usize,
    pub(crate) sample_numbers: Vec<String>,
    pub(crate) cancelled: bool,
    pub(crate) disk_low_mb: Option<u64>,
}

/// 通用逐行 JSONL 流式写入器（本地 `.jsonl` / `.jsonl.gz` 与远端 WebDAV / HTTP 流共享同一低内存引擎）。
pub(crate) fn sync_stream_jsonl_reader<R: BufRead>(
    db: &crate::db::Database,
    reader: R,
    options: &PatentTableImportOptions,
    cancel: &AtomicBool,
    mut on_batch: impl FnMut(usize, usize, usize, &[String]),
) -> Result<StreamFileSyncOutcome, String> {
    let prefixes = parse_ipc_prefixes(&options.ipc_prefix);

    let mut buf: Vec<Patent> = Vec::with_capacity(STREAM_BATCH_ROWS);
    let mut total_imported = 0usize;
    let mut total_filtered = 0usize;
    let mut total_chunks = 0usize;
    let mut samples: Vec<String> = Vec::new();
    let mut batch_filtered = 0usize;
    let mut batches_since_disk_check = 0usize;

    for line_res in reader.lines() {
        if cancel.load(Ordering::Relaxed) {
            return Ok(StreamFileSyncOutcome {
                imported: total_imported,
                filtered_out: total_filtered + batch_filtered,
                chunks: total_chunks,
                sample_numbers: samples,
                cancelled: true,
                disk_low_mb: None,
            });
        }
        let line = match line_res {
            Ok(l) => l,
            Err(_) => continue,
        };
        let trimmed = line.trim().trim_start_matches('\u{feff}');
        if trimmed.is_empty() {
            continue;
        }
        let Ok(val) = serde_json::from_str::<Value>(trimmed) else {
            continue;
        };
        let Some(row_map) = json_value_to_row_map(&val) else {
            continue;
        };
        let Some(patent) = row_map_to_patent(&row_map, options) else {
            continue;
        };
        if !matches_ipc_prefixes(&patent, &prefixes) {
            batch_filtered += 1;
            continue;
        }
        if samples.len() < 5 {
            let label = if !patent.patent_number.is_empty() {
                patent.patent_number.clone()
            } else {
                patent.title.clone()
            };
            if !label.is_empty() && !samples.contains(&label) {
                samples.push(label);
            }
        }
        buf.push(patent);

        if buf.len() >= STREAM_BATCH_ROWS {
            batches_since_disk_check += 1;
            if batches_since_disk_check >= 5 {
                batches_since_disk_check = 0;
                if let Some(free_mb) = free_disk_mb_for_db(db) {
                    if free_mb < MIN_FREE_DISK_MB {
                        return Ok(StreamFileSyncOutcome {
                            imported: total_imported,
                            filtered_out: total_filtered + batch_filtered,
                            chunks: total_chunks,
                            sample_numbers: samples,
                            cancelled: false,
                            disk_low_mb: Some(free_mb),
                        });
                    }
                }
            }
            let (imp, chks, was_cancelled) = db
                .batch_import_patents_chunked(
                    &buf,
                    options.lightweight,
                    "char-tfidf-v2",
                    Some(cancel),
                )
                .map_err(|e| format!("批量写入失败: {e}"))?;
            total_imported += imp;
            total_filtered += batch_filtered;
            total_chunks += chks;
            on_batch(imp, batch_filtered, chks, &samples);
            batch_filtered = 0;
            buf.clear();
            if was_cancelled {
                return Ok(StreamFileSyncOutcome {
                    imported: total_imported,
                    filtered_out: total_filtered,
                    chunks: total_chunks,
                    sample_numbers: samples,
                    cancelled: true,
                    disk_low_mb: None,
                });
            }
        }
    }

    if !buf.is_empty() {
        let (imp, chks, was_cancelled) = db
            .batch_import_patents_chunked(&buf, options.lightweight, "char-tfidf-v2", Some(cancel))
            .map_err(|e| format!("批量写入失败: {e}"))?;
        total_imported += imp;
        total_filtered += batch_filtered;
        total_chunks += chks;
        on_batch(imp, batch_filtered, chks, &samples);
        if was_cancelled {
            return Ok(StreamFileSyncOutcome {
                imported: total_imported,
                filtered_out: total_filtered,
                chunks: total_chunks,
                sample_numbers: samples,
                cancelled: true,
                disk_low_mb: None,
            });
        }
    } else if batch_filtered > 0 {
        total_filtered += batch_filtered;
        on_batch(0, batch_filtered, 0, &samples);
    }

    Ok(StreamFileSyncOutcome {
        imported: total_imported,
        filtered_out: total_filtered,
        chunks: total_chunks,
        sample_numbers: samples,
        cancelled: false,
        disk_low_mb: None,
    })
}

/// 逐行流式解析并分段提交 `.jsonl` / `.jsonl.gz` 专利公报（每 500 行分段提交，内存占用 < 10 MB，实时响应取消与磁盘守卫）。
fn sync_stream_jsonl_file(
    db: &crate::db::Database,
    path: &Path,
    options: &PatentTableImportOptions,
    cancel: &AtomicBool,
    on_batch: impl FnMut(usize, usize, usize, &[String]),
) -> Result<StreamFileSyncOutcome, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("打开文件失败: {e}"))?;
    let lower = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if lower.ends_with(".jsonl.gz") {
        let decoder = flate2::read::GzDecoder::new(file);
        let reader = BufReader::with_capacity(128 * 1024, decoder);
        sync_stream_jsonl_reader(db, reader, options, cancel, on_batch)
    } else {
        let reader = BufReader::with_capacity(128 * 1024, file);
        sync_stream_jsonl_reader(db, reader, options, cancel, on_batch)
    }
}

#[allow(clippy::too_many_arguments)]
fn execute_folder_sync_worker(
    db: Arc<crate::db::Database>,
    dir: PathBuf,
    folder_trimmed: String,
    resolved_for_resp: Option<String>,
    ipc_prefix: String,
    lightweight: bool,
    force: bool,
    cancel_flag: Arc<AtomicBool>,
    update_global_job: bool,
) -> Result<Value, String> {
    let started_at = Instant::now();
    let (files, deduped_pairs) = list_syncable_files_recursive_dedup(&dir)?;
    let table_files = files.iter().filter(|f| f.3 == SyncFileKind::Table).count();
    let doc_files = files.iter().filter(|f| f.3 == SyncFileKind::Doc).count();
    let archive_files = files
        .iter()
        .filter(|f| f.3 == SyncFileKind::Archive)
        .count();
    let initial_free_mb = free_disk_mb_for_db(&db);

    if update_global_job {
        update_sync_snapshot(|snap| {
            snap.state = "running".to_string();
            snap.folder_path = folder_trimmed.clone();
            snap.resolved_path = resolved_for_resp.clone();
            snap.files_done = 0;
            snap.files_total = files.len();
            snap.deduped_pair_files = deduped_pairs;
            snap.table_files = table_files;
            snap.doc_files = doc_files;
            snap.archive_files = archive_files;
            snap.imported_files = 0;
            snap.skipped_unchanged_files = 0;
            snap.imported_patents = 0;
            snap.filtered_out = 0;
            snap.chunks = 0;
            snap.rows_per_sec = 0;
            snap.free_disk_mb = initial_free_mb;
            snap.elapsed_secs = 0;
            snap.sample_numbers.clear();
            snap.errors.clear();
            snap.summary.clear();
        });
    }

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
    let mut sample_numbers: Vec<String> = Vec::new();
    let mut details: Vec<FileSyncResult> = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    let mut was_cancelled = false;
    let mut paused_disk_low_mb: Option<u64> = None;

    let options = PatentTableImportOptions {
        ipc_prefix: ipc_prefix.clone(),
        lightweight,
        source_tag: "folder_sync".to_string(),
    };

    for (idx, (name, path, sig, _kind)) in files.iter().enumerate() {
        if cancel_flag.load(Ordering::Relaxed) {
            was_cancelled = true;
            break;
        }
        let free_mb = free_disk_mb_for_db(&db);
        if let Some(mb) = free_mb {
            if mb < MIN_FREE_DISK_MB {
                paused_disk_low_mb = Some(mb);
                break;
            }
        }
        if update_global_job {
            update_sync_snapshot(|snap| {
                snap.current_file = name.clone();
                snap.files_done = idx;
                snap.free_disk_mb = free_mb;
                snap.elapsed_secs = started_at.elapsed().as_secs();
            });
        }

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
            if update_global_job {
                update_sync_snapshot(|snap| {
                    snap.files_done = idx + 1;
                    snap.skipped_unchanged_files = skipped_unchanged_files;
                });
            }
            continue;
        }

        let is_stream_jsonl = matches!(table_ext_for_filename(name), Some(("jsonl", _)));

        if is_stream_jsonl {
            let mut batch_imp_acc = total_imported;
            let mut batch_flt_acc = total_filtered;
            let mut batch_chk_acc = total_chunks;
            match sync_stream_jsonl_file(
                &db,
                path,
                &options,
                &cancel_flag,
                |imp, flt, chks, smps| {
                    batch_imp_acc += imp;
                    batch_flt_acc += flt;
                    batch_chk_acc += chks;
                    if update_global_job {
                        let secs = started_at.elapsed().as_secs().max(1);
                        let rate = (batch_imp_acc + batch_flt_acc) / secs as usize;
                        update_sync_snapshot(|snap| {
                            snap.imported_patents = batch_imp_acc;
                            snap.filtered_out = batch_flt_acc;
                            snap.chunks = batch_chk_acc;
                            snap.rows_per_sec = rate;
                            snap.elapsed_secs = secs;
                            for s in smps {
                                if snap.sample_numbers.len() < 5 && !snap.sample_numbers.contains(s)
                                {
                                    snap.sample_numbers.push(s.clone());
                                }
                            }
                        });
                    }
                },
            ) {
                Ok(out) => {
                    total_imported += out.imported;
                    total_filtered += out.filtered_out;
                    total_chunks += out.chunks;
                    for s in out.sample_numbers {
                        if sample_numbers.len() < 5 && !sample_numbers.contains(&s) {
                            sample_numbers.push(s);
                        }
                    }
                    if let Some(mb) = out.disk_low_mb {
                        paused_disk_low_mb = Some(mb);
                        break;
                    }
                    if out.cancelled {
                        was_cancelled = true;
                        break;
                    }
                    manifest.insert(manifest_key, sig.clone());
                    if out.imported > 0 {
                        imported_files += 1;
                    }
                    details.push(FileSyncResult {
                        filename: name.clone(),
                        status: if out.imported > 0 {
                            "imported"
                        } else {
                            "empty"
                        }
                        .to_string(),
                        imported: out.imported,
                        filtered_out: out.filtered_out,
                        chunks: out.chunks,
                        message: format!("导入 {} 件", out.imported),
                    });
                }
                Err(e) => {
                    // 未下载完的 ZIP 压缩包缺少中央目录（EOCD），不记入 manifest 且温和跳过，待下轮完结后自动补导
                    let is_incomplete_zip = name.to_ascii_lowercase().ends_with(".zip")
                        && e.contains("Could not find EOCD");
                    if is_incomplete_zip {
                        details.push(FileSyncResult {
                            filename: name.clone(),
                            status: "skipped_incomplete".to_string(),
                            imported: 0,
                            filtered_out: 0,
                            chunks: 0,
                            message: "下载中未完结 ZIP，已跳过待下次自动补导".to_string(),
                        });
                    } else {
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
        } else {
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

            match parse_any_patent_bytes(name, &bytes, &options) {
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
                            message: "无匹配专利/文献行".to_string(),
                        });
                    } else {
                        for p in &st.matched_patents {
                            if sample_numbers.len() >= 5 {
                                break;
                            }
                            let label = if !p.patent_number.is_empty() {
                                p.patent_number.clone()
                            } else {
                                p.title.clone()
                            };
                            if !label.is_empty() && !sample_numbers.contains(&label) {
                                sample_numbers.push(label);
                            }
                        }
                        match db.batch_import_patents_chunked(
                            &st.matched_patents,
                            lightweight,
                            "char-tfidf-v2",
                            Some(&cancel_flag),
                        ) {
                            Ok((imp, chunks, chunk_cancelled)) => {
                                total_imported += imp;
                                total_filtered += st.filtered_out;
                                total_chunks += chunks;
                                if chunk_cancelled {
                                    was_cancelled = true;
                                    break;
                                }
                                manifest.insert(manifest_key, sig.clone());
                                imported_files += 1;
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
                }
                Err(e) => {
                    // 未下载完的 ZIP 压缩包缺少中央目录（EOCD），不记入 manifest 且温和跳过，待下轮完结后自动补导
                    let is_incomplete_zip = name.to_ascii_lowercase().ends_with(".zip")
                        && e.contains("Could not find EOCD");
                    if is_incomplete_zip {
                        details.push(FileSyncResult {
                            filename: name.clone(),
                            status: "skipped_incomplete".to_string(),
                            imported: 0,
                            filtered_out: 0,
                            chunks: 0,
                            message: "下载中未完结 ZIP，已跳过待下次自动补导".to_string(),
                        });
                    } else {
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
        }

        // 每完成一个文件即断点落盘 manifest，保证用户随时点击「停止」后下次增量同步无缝续跑
        if let Ok(manifest_json) = serde_json::to_string(&manifest) {
            let _ = db.set_settings_batch(&[(SETTING_SYNC_MANIFEST, &manifest_json)]);
        }
        if update_global_job {
            let secs = started_at.elapsed().as_secs().max(1);
            let rate = (total_imported + total_filtered) / secs as usize;
            update_sync_snapshot(|snap| {
                snap.files_done = idx + 1;
                snap.imported_files = imported_files;
                snap.skipped_unchanged_files = skipped_unchanged_files;
                snap.imported_patents = total_imported;
                snap.filtered_out = total_filtered;
                snap.chunks = total_chunks;
                snap.rows_per_sec = rate;
                snap.elapsed_secs = secs;
                snap.sample_numbers = sample_numbers.clone();
                snap.errors = errors.clone();
            });
        }
    }

    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let dedup_note = if deduped_pairs > 0 {
        format!("，自动跳过同名重复 CSV {} 个", deduped_pairs)
    } else {
        String::new()
    };
    let final_state = if let Some(mb) = paused_disk_low_mb {
        errors.insert(
            0,
            format!(
                "数据库所在磁盘剩余空间仅 {mb} MB（低于 {MIN_FREE_DISK_MB} MB 安全红线），已自动暂停同步以保护硬盘"
            ),
        );
        "paused_disk_low"
    } else if was_cancelled {
        "cancelled"
    } else {
        "done"
    };
    let state_prefix = match final_state {
        "paused_disk_low" => "⚠️ 磁盘空间不足已安全暂停：",
        "cancelled" => "⏹ 已手动停止（已完成文件断点已保留）：",
        _ => "",
    };
    let summary = format!(
        "{state_prefix}扫描 {} 个文件（表格 {} · 文档 {} · 压缩包 {}{}）：导入 {} 个（共 {} 件专利/文献），跳过未变更 {} 个",
        files.len(),
        table_files,
        doc_files,
        archive_files,
        dedup_note,
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

    let final_free_mb = free_disk_mb_for_db(&db);
    let elapsed_secs = started_at.elapsed().as_secs();
    let rate = (total_imported + total_filtered) / elapsed_secs.max(1) as usize;
    if update_global_job {
        update_sync_snapshot(|snap| {
            snap.state = final_state.to_string();
            snap.current_file.clear();
            if final_state == "done" {
                snap.files_done = files.len();
            }
            snap.imported_files = imported_files;
            snap.skipped_unchanged_files = skipped_unchanged_files;
            snap.imported_patents = total_imported;
            snap.filtered_out = total_filtered;
            snap.chunks = total_chunks;
            snap.rows_per_sec = rate;
            snap.free_disk_mb = final_free_mb;
            snap.elapsed_secs = elapsed_secs;
            snap.sample_numbers = sample_numbers.clone();
            snap.summary = summary.clone();
            snap.errors = errors.clone();
        });
    }

    Ok(json!({
        "status": "ok",
        "state": final_state,
        "folder_path": folder_trimmed,
        "resolved_path": resolved_for_resp,
        "scanned_files": files.len(),
        "deduped_pair_files": deduped_pairs,
        "table_files": table_files,
        "doc_files": doc_files,
        "archive_files": archive_files,
        "imported_files": imported_files,
        "skipped_unchanged_files": skipped_unchanged_files,
        "imported_patents": total_imported,
        "filtered_out": total_filtered,
        "chunks": total_chunks,
        "free_disk_mb": final_free_mb,
        "elapsed_secs": elapsed_secs,
        "sample_numbers": sample_numbers,
        "last_at": now,
        "summary": summary,
        "errors": errors,
        "details": details
    }))
}

fn normalize_auto_interval_mins(raw: u64) -> u64 {
    if raw == 0 {
        0
    } else {
        raw.clamp(5, 1440)
    }
}

/// 读取采集脚本在同步目录（或其父目录，当用户指向 `headers_v2` 子目录时）生成的 `STATUS_v2.json` 脱敏进度摘要。
/// 严格只返回数值与布尔状态，绝不回传 PID、账号名、FTP 节点地址或本地私有路径。
fn read_collector_status_summary(dir: &Path) -> Option<Value> {
    let direct = dir.join("STATUS_v2.json");
    let candidate = if direct.is_file() {
        direct
    } else {
        let dir_name = dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if dir_name.starts_with("headers") {
            let parent_status = dir.parent()?.join("STATUS_v2.json");
            if parent_status.is_file() {
                parent_status
            } else {
                return None;
            }
        } else {
            return None;
        }
    };

    let meta = std::fs::metadata(&candidate).ok()?;
    if meta.len() == 0 || meta.len() > 65_536 {
        return None;
    }
    let raw = std::fs::read_to_string(&candidate).ok()?;
    let val: Value = serde_json::from_str(&raw).ok()?;
    let obj = val.as_object()?;
    let total_tasks = obj.get("total_tasks").and_then(Value::as_u64).unwrap_or(0);
    if total_tasks == 0 && obj.get("extracted_rows").is_none() {
        return None;
    }
    let active_count = obj
        .get("active")
        .and_then(Value::as_object)
        .map(|m| m.len())
        .unwrap_or(0);

    Some(json!({
        "running": obj.get("running").and_then(Value::as_bool).unwrap_or(false),
        "done": obj.get("done").and_then(Value::as_u64).unwrap_or(0),
        "total_tasks": total_tasks,
        "failed": obj.get("failed").and_then(Value::as_u64).unwrap_or(0),
        "extracted_rows": obj.get("extracted_rows").and_then(Value::as_u64).unwrap_or(0),
        "total_speed_kbps": obj.get("total_speed_kbps").and_then(Value::as_f64).unwrap_or(0.0),
        "free_disk_gb": obj.get("free_disk_gb").and_then(Value::as_f64),
        "active_count": active_count,
        "updated_at": obj.get("updated_at").and_then(Value::as_str).unwrap_or(""),
    }))
}

/// 快速比对目录中是否存在未入库或已更新的文件（仅查 metadata + manifest，无新文件时零写库开销）。
fn count_pending_sync_files(
    db: &crate::db::Database,
    dir: &Path,
    ipc_prefix: &str,
    lightweight: bool,
) -> usize {
    let Ok((files, _)) = list_syncable_files_recursive_dedup(dir) else {
        return 0;
    };
    if files.is_empty() {
        return 0;
    }
    let prev_manifest_raw = db
        .get_setting(SETTING_SYNC_MANIFEST)
        .ok()
        .flatten()
        .unwrap_or_default();
    let manifest: HashMap<String, String> =
        serde_json::from_str(&prev_manifest_raw).unwrap_or_default();
    files
        .iter()
        .filter(|(name, _, sig, _)| {
            let key = format!("{}|{}|{}", name, ipc_prefix, lightweight);
            !manifest.get(&key).is_some_and(|old| old == sig)
        })
        .count()
}

/// 启动后台定时增量同步巡检循环（仅当用户显式开启 `patent_sync_auto_interval_mins > 0` 且有新完结文件时触发）。
pub fn spawn_folder_auto_sync_loop(db: Arc<crate::db::Database>) {
    tokio::spawn(async move {
        // 服务刚启动时等待 45 秒，避免抢占首屏加载与初始化资源
        tokio::time::sleep(std::time::Duration::from_secs(45)).await;
        let mut last_attempt: Option<Instant> = None;

        loop {
            let db_check = db.clone();
            let cfg_res = tokio::task::spawn_blocking(move || {
                let interval_mins = db_check
                    .get_setting(SETTING_SYNC_AUTO_INTERVAL_MINS)
                    .ok()
                    .flatten()
                    .and_then(|v| v.trim().parse::<u64>().ok())
                    .map(normalize_auto_interval_mins)
                    .unwrap_or(0);
                let folder = db_check
                    .get_setting(SETTING_SYNC_FOLDER)
                    .ok()
                    .flatten()
                    .unwrap_or_default();
                let ipc_prefix = db_check
                    .get_setting(SETTING_SYNC_IPC_PREFIX)
                    .ok()
                    .flatten()
                    .unwrap_or_default();
                let lightweight = db_check
                    .get_setting(SETTING_SYNC_LIGHTWEIGHT)
                    .ok()
                    .flatten()
                    .is_some_and(|v| v == "1" || v == "true");
                (interval_mins, folder, ipc_prefix, lightweight)
            })
            .await;

            if let Ok((interval_mins, folder_raw, ipc_prefix, lightweight)) = cfg_res {
                let folder_trimmed = clean_folder_path_input(&folder_raw);
                let due = interval_mins > 0
                    && !folder_trimmed.is_empty()
                    && last_attempt
                        .map(|t| t.elapsed().as_secs() >= interval_mins.saturating_mul(60))
                        .unwrap_or(true);

                if due {
                    last_attempt = Some(Instant::now());
                    let db_probe = db.clone();
                    let folder_probe = folder_trimmed.clone();
                    let ipc_probe = ipc_prefix.clone();

                    let should_run = tokio::task::spawn_blocking(move || {
                        let (dir, resolved) = resolve_local_folder_path(&folder_probe);
                        if !dir.is_dir() {
                            return None;
                        }
                        if let Some(free_mb) = free_disk_mb_for_db(&db_probe) {
                            if free_mb < MIN_FREE_DISK_MB {
                                return None;
                            }
                        }
                        let pending =
                            count_pending_sync_files(&db_probe, &dir, &ipc_probe, lightweight);
                        if pending > 0 {
                            Some((dir, resolved, pending))
                        } else {
                            None
                        }
                    })
                    .await
                    .ok()
                    .flatten();

                    if let Some((dir, resolved_path, pending)) = should_run {
                        let maybe_cancel = {
                            let mut guard =
                                sync_manager().lock().unwrap_or_else(|e| e.into_inner());
                            if guard.snapshot.state == "running" {
                                None
                            } else {
                                let new_cancel = Arc::new(AtomicBool::new(false));
                                guard.cancel_flag = new_cancel.clone();
                                guard.snapshot = FolderSyncJobSnapshot {
                                    state: "running".to_string(),
                                    folder_path: folder_trimmed.clone(),
                                    resolved_path: resolved_path.clone(),
                                    free_disk_mb: free_disk_mb_for_db(&db),
                                    ..Default::default()
                                };
                                Some(new_cancel)
                            }
                        };

                        if let Some(cancel_flag) = maybe_cancel {
                            tracing::info!(
                                "自动增量同步检测到 {} 个新增/变更文件，开始后台同步: {}",
                                pending,
                                folder_trimmed
                            );
                            let db_run = db.clone();
                            let folder_bg = folder_trimmed.clone();
                            tokio::task::spawn_blocking(move || {
                                if let Err(err) = execute_folder_sync_worker(
                                    db_run,
                                    dir,
                                    folder_bg,
                                    resolved_path,
                                    ipc_prefix,
                                    lightweight,
                                    false,
                                    cancel_flag,
                                    true,
                                ) {
                                    update_sync_snapshot(|snap| {
                                        snap.state = "error".to_string();
                                        snap.current_file.clear();
                                        snap.errors.push(err.clone());
                                        snap.summary = format!("自动同步失败: {err}");
                                    });
                                }
                            });
                        }
                    }
                }
            }

            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        }
    });
}

/// GET `/api/patents/sync-folder` — 获取当前已配置的本地/网盘专利同步目录状态（含后台任务实时快照）
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
    let auto_interval_mins =
        s.db.get_setting(SETTING_SYNC_AUTO_INTERVAL_MINS)
            .ok()
            .flatten()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .map(normalize_auto_interval_mins)
            .unwrap_or(0);

    let folder_for_scan = folder_path.clone();
    let db_path_for_disk = s.db.main_db_file_path();
    let (
        exists,
        detected_files,
        deduped_pair_files,
        table_files,
        doc_files,
        archive_files,
        resolved_path,
        free_disk_mb,
        collector_status,
    ) = tokio::task::spawn_blocking(move || {
        let free_mb = db_path_for_disk.as_deref().and_then(free_disk_mb_for_path);
        if folder_for_scan.trim().is_empty() {
            return (
                false, 0usize, 0usize, 0usize, 0usize, 0usize, None, free_mb, None,
            );
        }
        let (p, mapped) = resolve_local_folder_path(&folder_for_scan);
        if p.is_dir() {
            let (list, deduped) = list_syncable_files_recursive_dedup(&p).unwrap_or_default();
            let t = list.iter().filter(|f| f.3 == SyncFileKind::Table).count();
            let d = list.iter().filter(|f| f.3 == SyncFileKind::Doc).count();
            let a = list.iter().filter(|f| f.3 == SyncFileKind::Archive).count();
            let collector = read_collector_status_summary(&p);
            (
                true,
                list.len(),
                deduped,
                t,
                d,
                a,
                mapped,
                free_mb,
                collector,
            )
        } else {
            (false, 0, 0, 0, 0, 0, mapped, free_mb, None)
        }
    })
    .await
    .unwrap_or((false, 0, 0, 0, 0, 0, None, None, None));

    Json(json!({
        "status": "ok",
        "folder_path": folder_path,
        "resolved_path": resolved_path,
        "ipc_prefix": ipc_prefix,
        "lightweight": lightweight,
        "auto_interval_mins": auto_interval_mins,
        "exists": exists,
        "detected_files": detected_files,
        "deduped_pair_files": deduped_pair_files,
        "table_files": table_files,
        "doc_files": doc_files,
        "archive_files": archive_files,
        "free_disk_mb": free_disk_mb,
        "collector_status": collector_status,
        "last_at": last_at,
        "last_summary": last_summary,
        "job": current_sync_snapshot()
    }))
}

/// GET `/api/patents/sync-folder/progress` — 查询后台 GB 级目录同步实时进度
pub async fn api_get_patent_sync_progress() -> Json<Value> {
    Json(json!({
        "status": "ok",
        "job": current_sync_snapshot()
    }))
}

/// POST `/api/patents/sync-folder/cancel` — 立即请求优雅停止正在运行的后台目录同步（保留已完成断点）
pub async fn api_cancel_patent_sync() -> Json<Value> {
    let snap = {
        let guard = sync_manager().lock().unwrap_or_else(|e| e.into_inner());
        guard.cancel_flag.store(true, Ordering::Relaxed);
        guard.snapshot.clone()
    };
    Json(json!({
        "status": "ok",
        "cancelling": snap.state == "running",
        "job": snap
    }))
}

/// POST `/api/patents/sync-folder` — 保存并递归增量同步本地/私人网盘同步目录中的专利表格、PDF/OFD/DOCX 与 ZIP 压缩包
pub async fn api_sync_patent_folder(
    State(s): State<AppState>,
    Json(req): Json<SyncFolderRequest>,
) -> Json<Value> {
    let folder_trimmed = clean_folder_path_input(&req.folder_path);
    let validated_ipc = match validate_ipc_prefix_filter(&req.ipc_prefix) {
        Ok(v) => v,
        Err(msg) => {
            return Json(json!({
                "status": "error",
                "message": msg
            }));
        }
    };

    if folder_trimmed.is_empty() {
        let _ = s.db.set_settings_batch(&[
            (SETTING_SYNC_FOLDER, ""),
            (SETTING_SYNC_IPC_PREFIX, &validated_ipc),
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

    let (dir, resolved_path) = resolve_local_folder_path(&folder_trimmed);
    if !dir.exists() || !dir.is_dir() {
        let detail = if let Some(ref mapped) = resolved_path {
            format!(
                "目录不存在或不是文件夹: {}（WSL 自动映射路径 {} 也不存在）",
                folder_trimmed, mapped
            )
        } else {
            format!("目录不存在或不是文件夹: {}", folder_trimmed)
        };
        return Json(json!({
            "status": "error",
            "message": detail
        }));
    }

    let lw_flag = if req.lightweight { "1" } else { "0" };
    let mut settings_to_save: Vec<(&str, String)> = vec![
        (SETTING_SYNC_FOLDER, folder_trimmed.clone()),
        (SETTING_SYNC_IPC_PREFIX, validated_ipc.clone()),
        (SETTING_SYNC_LIGHTWEIGHT, lw_flag.to_string()),
    ];
    let effective_auto_mins = if let Some(raw_mins) = req.auto_interval_mins {
        let norm = normalize_auto_interval_mins(raw_mins);
        settings_to_save.push((SETTING_SYNC_AUTO_INTERVAL_MINS, norm.to_string()));
        norm
    } else {
        s.db.get_setting(SETTING_SYNC_AUTO_INTERVAL_MINS)
            .ok()
            .flatten()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .map(normalize_auto_interval_mins)
            .unwrap_or(0)
    };
    let batch_refs: Vec<(&str, &str)> = settings_to_save
        .iter()
        .map(|(k, v)| (*k, v.as_str()))
        .collect();
    if let Err(e) = s.db.set_settings_batch(&batch_refs) {
        return Json(json!({
            "status": "error",
            "message": format!("保存同步目录配置失败: {e}")
        }));
    }

    if req.save_only {
        let dir_for_scan = dir.clone();
        let (list_len, deduped, collector) = tokio::task::spawn_blocking(move || {
            let (list, deduped) =
                list_syncable_files_recursive_dedup(&dir_for_scan).unwrap_or_default();
            let collector = read_collector_status_summary(&dir_for_scan);
            (list.len(), deduped, collector)
        })
        .await
        .unwrap_or((0, 0, None));
        return Json(json!({
            "status": "ok",
            "folder_path": folder_trimmed,
            "resolved_path": resolved_path,
            "auto_interval_mins": effective_auto_mins,
            "detected_files": list_len,
            "deduped_pair_files": deduped,
            "free_disk_mb": free_disk_mb_for_db(&s.db),
            "collector_status": collector,
            "saved_only": true
        }));
    }

    if let Some(free_mb) = free_disk_mb_for_db(&s.db) {
        if free_mb < MIN_FREE_DISK_MB {
            return Json(json!({
                "status": "error",
                "message": format!(
                    "数据库所在磁盘剩余空间仅 {free_mb} MB（低于 {MIN_FREE_DISK_MB} MB 安全红线），已禁止启动批量同步以保护硬盘；请清理磁盘或先填写 IPC 赛道过滤"
                )
            }));
        }
    }

    if req.async_mode {
        let cancel_flag = {
            let mut guard = sync_manager().lock().unwrap_or_else(|e| e.into_inner());
            if guard.snapshot.state == "running" {
                return Json(json!({
                    "status": "ok",
                    "already_running": true,
                    "job": guard.snapshot.clone()
                }));
            }
            let new_cancel = Arc::new(AtomicBool::new(false));
            guard.cancel_flag = new_cancel.clone();
            guard.snapshot = FolderSyncJobSnapshot {
                state: "running".to_string(),
                folder_path: folder_trimmed.clone(),
                resolved_path: resolved_path.clone(),
                free_disk_mb: free_disk_mb_for_db(&s.db),
                ..Default::default()
            };
            new_cancel
        };

        let db = s.db.clone();
        let ipc_prefix = validated_ipc;
        let lightweight = req.lightweight;
        let force = req.force;
        let folder_bg = folder_trimmed.clone();
        let resolved_bg = resolved_path.clone();

        tokio::task::spawn_blocking(move || {
            if let Err(err) = execute_folder_sync_worker(
                db,
                dir,
                folder_bg,
                resolved_bg,
                ipc_prefix,
                lightweight,
                force,
                cancel_flag,
                true,
            ) {
                update_sync_snapshot(|snap| {
                    snap.state = "error".to_string();
                    snap.current_file.clear();
                    snap.errors.push(err.clone());
                    snap.summary = format!("同步失败: {err}");
                });
            }
        });

        return Json(json!({
            "status": "ok",
            "started": true,
            "folder_path": folder_trimmed,
            "resolved_path": resolved_path,
            "job": current_sync_snapshot()
        }));
    }

    let db = s.db.clone();
    let ipc_prefix = validated_ipc;
    let lightweight = req.lightweight;
    let force = req.force;
    let resolved_for_resp = resolved_path.clone();
    let cancel_flag = Arc::new(AtomicBool::new(false));

    let sync_res = tokio::task::spawn_blocking(move || {
        execute_folder_sync_worker(
            db,
            dir,
            folder_trimmed,
            resolved_for_resp,
            ipc_prefix,
            lightweight,
            force,
            cancel_flag,
            false,
        )
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

    #[test]
    fn windows_path_to_wsl_candidate_maps_drive_and_unc_paths() {
        assert_eq!(
            windows_path_to_wsl_candidate(r#"F:\down\专利数据"#),
            Some(PathBuf::from("/mnt/f/down/专利数据"))
        );
        assert_eq!(
            windows_path_to_wsl_candidate(r#""D:/Patents/2026""#),
            Some(PathBuf::from("/mnt/d/Patents/2026"))
        );
        assert_eq!(
            windows_path_to_wsl_candidate(r"\\wsl$\Ubuntu\home\user\patents"),
            Some(PathBuf::from("/home/user/patents"))
        );
        assert_eq!(windows_path_to_wsl_candidate("/mnt/f/down/专利数据"), None);
    }

    #[test]
    fn validate_ipc_prefix_rejects_folder_paths_and_allows_valid_ipcs() {
        assert_eq!(validate_ipc_prefix_filter("").unwrap(), "");
        assert_eq!(
            validate_ipc_prefix_filter("H01L, G06F").unwrap(),
            "H01L, G06F"
        );
        assert!(validate_ipc_prefix_filter(r"F:\down\专利数据").is_err());
        assert!(validate_ipc_prefix_filter("/mnt/f/down/专利数据").is_err());
        assert!(validate_ipc_prefix_filter("~/patents").is_err());
    }

    #[test]
    fn recursive_sync_lists_nested_files_and_parses_mixed_zip_bundle() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let sub = tmp.path().join("sub_folder");
        std::fs::create_dir_all(&sub).expect("mkdir");
        std::fs::write(
            sub.join("cnipa_export.csv"),
            "公开号,发明名称,摘要\nCN990123A,一种微压氧舱泄压阀,微压氧舱自动泄压保护结构\n",
        )
        .expect("write csv");
        std::fs::write(
            sub.join("note.txt"),
            "发明名称：一种多级旋流煤粉燃烧器\n公开号：CN990124A\n摘要：用于热风炉膛的煤粉稳燃装置。\n",
        )
        .expect("write txt");

        // 构造混合 ZIP（含 1 份 CSV + 1 份 TXT）
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut cursor);
            let opts = SimpleFileOptions::default();
            z.start_file("nested/table.csv", opts).expect("start csv");
            z.write_all(
                "公开号,发明名称,摘要\nCN990125A,一种氧舱监测模块,监测氧浓度与微压\n".as_bytes(),
            )
            .expect("write zip csv");
            z.start_file("nested/doc.txt", opts).expect("start txt");
            z.write_all(
                "发明名称：一种热风炉余热回收装置\n摘要：提升沥青热再生热效率。\n".as_bytes(),
            )
            .expect("write zip txt");
            z.finish().expect("finish zip");
        }
        let zip_bytes = cursor.into_inner();
        std::fs::write(tmp.path().join("bundle.zip"), &zip_bytes).expect("write zip");

        let found = list_syncable_files_recursive(tmp.path()).expect("list recursive");
        assert_eq!(
            found.len(),
            3,
            "应递归发现 1 个压缩包 + 子目录内 1 表 1 文档"
        );

        let parsed_zip = parse_any_patent_bytes(
            "bundle.zip",
            &zip_bytes,
            &PatentTableImportOptions::default(),
        )
        .expect("parse mixed zip");
        assert_eq!(
            parsed_zip.matched_patents.len(),
            2,
            "混合 ZIP 应同时解析出内嵌 CSV 与 TXT 文献"
        );

        // 确认 batch_import_patents_with_chunks 同步写入了 char-tfidf-v2 向量
        let db = Database::init(":memory:").expect("init db");
        let (imp, chunks) = db
            .batch_import_patents_with_chunks(&parsed_zip.matched_patents, "char-tfidf-v2")
            .expect("batch import");
        assert_eq!(imp, 2);
        assert!(chunks >= 2);
        let st = db.get_backfill_index_status().expect("status");
        assert_eq!(st["ready_patents"], 2);
        assert_eq!(st["remaining"], 0);
    }

    #[test]
    fn dedup_sibling_table_pairs_prefers_jsonl_over_csv() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let sub = tmp.path().join("2026Q3");
        std::fs::create_dir_all(&sub).expect("mkdir");
        std::fs::write(
            sub.join("BIBS_01.csv"),
            "patent_number,title\nCN990301A,T1\n",
        )
        .unwrap();
        std::fs::write(
            sub.join("BIBS_01.jsonl"),
            "{\"patent_number\":\"CN990301A\",\"title\":\"T1\"}\n",
        )
        .unwrap();
        std::fs::write(
            sub.join("ONLY_CSV.csv"),
            "patent_number,title\nCN990302A,T2\n",
        )
        .unwrap();

        let (kept, deduped) = list_syncable_files_recursive_dedup(tmp.path()).expect("list");
        assert_eq!(deduped, 1, "同名 .csv + .jsonl 对必须跳过 .csv");
        assert_eq!(kept.len(), 2);
        let names: Vec<&str> = kept.iter().map(|x| x.0.as_str()).collect();
        assert!(names.contains(&"2026Q3/BIBS_01.jsonl"));
        assert!(names.contains(&"2026Q3/ONLY_CSV.csv"));
    }

    #[test]
    fn stream_jsonl_lightweight_skips_chunks_and_vectors_and_respects_cancel() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let jsonl = tmp.path().join("gazette.jsonl");
        let mut lines = String::new();
        for i in 1..=6 {
            let ipc = if i % 2 == 0 { "F23D1/00" } else { "A61G10/02" };
            lines.push_str(&format!(
                "{{\"patent_number\":\"CN99040{i}A\",\"title\":\"测试专利 {i}\",\"abstract_text\":\"微压氧舱或燃烧器摘要 {i}\",\"ipc_codes\":\"{ipc}\"}}\n"
            ));
        }
        std::fs::write(&jsonl, lines).expect("write jsonl");

        let db = Database::init(":memory:").expect("init db");
        let opts = PatentTableImportOptions {
            ipc_prefix: "F23".to_string(),
            lightweight: true,
            source_tag: "folder_sync".to_string(),
        };
        let cancel = AtomicBool::new(false);
        let out = sync_stream_jsonl_file(&db, &jsonl, &opts, &cancel, |_, _, _, _| {})
            .expect("stream jsonl");
        assert_eq!(out.imported, 3);
        assert_eq!(out.filtered_out, 3);
        assert_eq!(out.chunks, 0, "lightweight 模式不应生成 patent_chunks");
        assert!(!out.cancelled);

        // 验证轻量公报立即可走 FTS5 搜到，且不污染向量待回填队列
        let st = db.get_backfill_index_status().expect("status");
        assert_eq!(st["lightweight_patents"], 3);
        assert_eq!(st["remaining"], 0);
        assert_eq!(db.count_embeddings().expect("emb count"), 0);

        // 预设取消标志应立即提前停止
        let cancel_now = AtomicBool::new(true);
        let out2 = sync_stream_jsonl_file(&db, &jsonl, &opts, &cancel_now, |_, _, _, _| {})
            .expect("cancelled stream");
        assert!(out2.cancelled);
        assert_eq!(out2.imported, 0);
    }

    #[test]
    fn import_filter_supports_chinese_keywords_and_exact_ipc_section_prefix() {
        let coal_pat = Patent {
            patent_number: "CN990501A".to_string(),
            title: "一种沥青热再生热风炉煤粉燃烧器".to_string(),
            abstract_text: "适用于 200 目神木煤粉稳燃。".to_string(),
            ipc_codes: "[\"F23D1/00\"]".to_string(),
            ..Default::default()
        };
        let oxy_pat = Patent {
            patent_number: "CN990502A".to_string(),
            title: "模块化微压氧舱泄压阀".to_string(),
            abstract_text: "医疗级家用微压氧舱。".to_string(),
            ipc_codes: "[\"A61G10/02\"]".to_string(),
            ..Default::default()
        };
        let software_pat = Patent {
            patent_number: "CN990503A".to_string(),
            title: "一种推荐算法排序方法".to_string(),
            abstract_text: "软件神经网络检索排序。".to_string(),
            ipc_codes: "[\"G06F16/24\"]".to_string(),
            ..Default::default()
        };

        // 1. 纯大白话中文关键词过滤
        let kw_rules = parse_ipc_prefixes("煤粉, 氧舱");
        assert!(matches_ipc_prefixes(&coal_pat, &kw_rules));
        assert!(matches_ipc_prefixes(&oxy_pat, &kw_rules));
        assert!(!matches_ipc_prefixes(&software_pat, &kw_rules));

        // 2. 单字母 IPC 部级大类 `F` 必须精准匹配 F23D1/00，绝不能误命中 G06F16/24
        let sec_f_rules = parse_ipc_prefixes("F");
        assert!(matches_ipc_prefixes(&coal_pat, &sec_f_rules));
        assert!(!matches_ipc_prefixes(&oxy_pat, &sec_f_rules));
        assert!(
            !matches_ipc_prefixes(&software_pat, &sec_f_rules),
            "大类 F 严禁误匹配包含字母 F 的软件分类号 G06F"
        );

        // 3. 中文关键词 + IPC 混写（OR 语义）
        let mixed_rules = parse_ipc_prefixes("F23, 氧舱");
        assert!(matches_ipc_prefixes(&coal_pat, &mixed_rules));
        assert!(matches_ipc_prefixes(&oxy_pat, &mixed_rules));
        assert!(!matches_ipc_prefixes(&software_pat, &mixed_rules));
    }

    #[test]
    fn stream_jsonl_supports_chinese_keyword_filter() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let jsonl = tmp.path().join("kw_gazette.jsonl");
        std::fs::write(
            &jsonl,
            concat!(
                "{\"patent_number\":\"CN990601A\",\"title\":\"超临界二氧化碳无水染色釜\",\"abstract_text\":\"纺织印染节能设备\",\"ipc_codes\":\"D06B3/00\"}\n",
                "{\"patent_number\":\"CN990602A\",\"title\":\"手机屏幕触控算法\",\"abstract_text\":\"提升滑动流畅度\",\"ipc_codes\":\"G06F3/041\"}\n"
            ),
        )
        .expect("write jsonl");

        let db = Database::init(":memory:").expect("init db");
        let opts = PatentTableImportOptions {
            ipc_prefix: "超临界二氧化碳, 染色".to_string(),
            lightweight: true,
            source_tag: "folder_sync".to_string(),
        };
        let cancel = AtomicBool::new(false);
        let out = sync_stream_jsonl_file(&db, &jsonl, &opts, &cancel, |_, _, _, _| {})
            .expect("stream keyword jsonl");
        assert_eq!(out.imported, 1);
        assert_eq!(out.filtered_out, 1);
        assert_eq!(out.sample_numbers, vec!["CN990601A".to_string()]);
    }

    #[test]
    fn sync_folder_skips_work_temp_dirs_and_supports_jsonl_gz_dedup() {
        let tmp = tempfile::tempdir().expect("tempdir");
        // 1) work_v2 临时下载目录内的半成品 ZIP 必须完全跳过
        let work_dir = tmp.path().join("work_v2").join("10-B").join("20260714");
        std::fs::create_dir_all(&work_dir).expect("mkdir work_v2");
        std::fs::write(work_dir.join("20260714.zip"), b"partial zip without eocd")
            .expect("write partial zip");
        std::fs::write(work_dir.join("20260714.z01"), b"split volume").expect("write z01");

        // 2) headers_v2 下同时存在 .csv / .jsonl / .jsonl.gz 及 .part 临时文件
        let headers = tmp.path().join("headers_v2").join("2026Q3");
        std::fs::create_dir_all(&headers).expect("mkdir headers");
        std::fs::write(
            headers.join("BIBS-10-A_20260728.csv"),
            b"patent_number,title\n",
        )
        .expect("write csv");
        std::fs::write(
            headers.join("BIBS-10-A_20260728.jsonl"),
            b"{\"patent_number\":\"CN990701A\",\"title\":\"raw\"}\n",
        )
        .expect("write jsonl");
        std::fs::write(headers.join("downloading.jsonl.part"), b"{}").expect("write part");

        // 写入真实的 gzip 压缩 .jsonl.gz
        let gz_path = headers.join("BIBS-10-A_20260728.jsonl.gz");
        {
            let f = std::fs::File::create(&gz_path).expect("create gz");
            let mut enc = flate2::write::GzEncoder::new(f, flate2::Compression::default());
            enc.write_all(
                concat!(
                    "{\"patent_number\":\"CN990701A\",\"title\":\"高压氧舱安全阀\",\"ipc_codes\":\"A61G10/02\"}\n",
                    "{\"patent_number\":\"CN990702A\",\"title\":\"手机壳装饰工艺\",\"ipc_codes\":\"A45C11/00\"}\n"
                )
                .as_bytes(),
            )
            .expect("write gz body");
            enc.finish().expect("finish gz");
        }

        let (files, deduped) = list_syncable_files_recursive_dedup(tmp.path()).expect("list files");
        assert_eq!(files.len(), 1, "应只保留最高优先级的 .jsonl.gz: {files:?}");
        assert!(files[0].0.ends_with(".jsonl.gz"));
        assert_eq!(deduped, 2, "同前缀 .csv 与 .jsonl 均应自动去重");

        // 流式解析 .jsonl.gz + IPC 过滤
        let db = Database::init(":memory:").expect("init db");
        let opts = PatentTableImportOptions {
            ipc_prefix: "A61".to_string(),
            lightweight: true,
            source_tag: "folder_sync".to_string(),
        };
        let cancel = AtomicBool::new(false);
        let out = sync_stream_jsonl_file(&db, &gz_path, &opts, &cancel, |_, _, _, _| {})
            .expect("stream gz");
        assert_eq!(out.imported, 1);
        assert_eq!(out.filtered_out, 1);
        assert_eq!(out.sample_numbers, vec!["CN990701A".to_string()]);
    }

    #[test]
    fn collector_status_summary_strips_sensitive_fields_and_tracks_pending_changes() {
        let tmp = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            tmp.path().join("STATUS_v2.json"),
            r#"{
              "running": true,
              "pid": 99999,
              "total_tasks": 881,
              "done": 61,
              "failed": 2,
              "extracted_rows": 911459,
              "total_speed_kbps": 960.6,
              "free_disk_gb": 31.2,
              "active": {"10-A:20260529": "20260529.zip via ftp1/secret_account"},
              "updated_at": "2026-11-28T07:25:41"
            }"#,
        )
        .expect("write status");

        let summary = read_collector_status_summary(tmp.path()).expect("collector summary");
        assert_eq!(summary["running"], true);
        assert_eq!(summary["done"], 61);
        assert_eq!(summary["total_tasks"], 881);
        assert_eq!(summary["active_count"], 1);
        let serialized = summary.to_string();
        assert!(!serialized.contains("secret_account"));
        assert!(!serialized.contains("99999"));

        // 增量待同步检测
        let db = Arc::new(Database::init(":memory:").expect("init db"));
        std::fs::write(
            tmp.path().join("batch1.jsonl"),
            "{\"patent_number\":\"CN990801A\",\"title\":\"增量测试专利\",\"ipc_codes\":\"F23D1/00\"}\n",
        )
        .expect("write batch1");
        assert_eq!(count_pending_sync_files(&db, tmp.path(), "", true), 1);

        let cancel = Arc::new(AtomicBool::new(false));
        let _ = execute_folder_sync_worker(
            db.clone(),
            tmp.path().to_path_buf(),
            tmp.path().to_string_lossy().to_string(),
            None,
            String::new(),
            true,
            false,
            cancel,
            false,
        )
        .expect("sync batch1");
        assert_eq!(
            count_pending_sync_files(&db, tmp.path(), "", true),
            0,
            "已入库且未变更文件不应重复触发自动同步"
        );
    }
}
