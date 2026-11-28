//! 跨网盘通用远端只读专利源增量同步模块（路线 B：协议无关，零厂商绑定）
//!
//! 原生支持两套开放标准协议，覆盖主流国内外网盘与对象存储（接收方免注册网盘账号、免装客户端）：
//! 1. **通用 WebDAV（`webdav`）**：兼容天翼云盘/阿里云盘/夸克/115 + OpenList (AList) 只读 WebDAV、
//!    坚果云、群晖 Synology NAS、Nextcloud、OwnCloud 与 Rclone WebDAV。
//!    支持安全跟随 `302 重定向`（如 OpenList -> 天翼云 CDN 直链），且跨域名跳转绝不泄露 Basic/Bearer 凭证。
//! 2. **通用 HTTP(S) 清单订阅源（`manifest`）**：兼容 Cloudflare R2（自定义域名 / Worker）、
//!    阿里云 OSS、腾讯云 COS、七牛云、MinIO 与任意静态 HTTP 服务；拉取 `innoforge-feed.json`（或 `manifest.json`）
//!    对比增量，仅流式拉取新增/变更的 `.jsonl.gz` / `.jsonl` / `.csv` 并通过独立 SQLite WAL 写入。

use crate::routes::patent_import::{
    classify_sync_filename, free_disk_mb_for_db, free_disk_mb_for_path, is_temp_download_filename,
    normalize_auto_interval_mins, parse_any_patent_bytes, should_skip_sync_dir,
    sync_stream_jsonl_file, table_dedup_stem_and_rank, table_ext_for_filename,
    validate_ipc_prefix_filter, PatentTableImportOptions, SyncFileKind, MIN_FREE_DISK_MB,
};
use crate::routes::AppState;
use axum::extract::State;
use axum::response::Json;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

const SETTING_REMOTE_MODE: &str = "patent_remote_sync_mode";
const SETTING_REMOTE_URL: &str = "patent_remote_sync_url";
const SETTING_REMOTE_USERNAME: &str = "patent_remote_sync_username";
const SETTING_REMOTE_PASSWORD: &str = "patent_remote_sync_password";
const SETTING_REMOTE_TOKEN: &str = "patent_remote_sync_token";
const SETTING_REMOTE_IPC_PREFIX: &str = "patent_remote_sync_ipc_prefix";
const SETTING_REMOTE_LIGHTWEIGHT: &str = "patent_remote_sync_lightweight";
const SETTING_REMOTE_AUTO_MINS: &str = "patent_remote_sync_auto_interval_mins";
const SETTING_REMOTE_LAST_AT: &str = "patent_remote_sync_last_at";
const SETTING_REMOTE_LAST_SUMMARY: &str = "patent_remote_sync_last_summary";
const SETTING_REMOTE_MANIFEST: &str = "patent_remote_sync_file_manifest";

const MAX_REMOTE_DEPTH: usize = 4;
const MAX_REMOTE_FILES: usize = 500;
const MAX_REMOTE_FILE_BYTES: u64 = 200 * 1024 * 1024; // 单文件 200 MB 安全上限
const MAX_REDIRECT_HOPS: usize = 5;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteSyncRequest {
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub clear_password: bool,
    #[serde(default)]
    pub token: Option<String>,
    #[serde(default)]
    pub clear_token: bool,
    #[serde(default)]
    pub ipc_prefix: String,
    #[serde(default = "default_true")]
    pub lightweight: bool,
    #[serde(default)]
    pub auto_interval_mins: Option<u64>,
    #[serde(default)]
    pub action: String, // "save" | "test" | "sync"
    #[serde(default)]
    pub force: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteSyncJobSnapshot {
    pub state: String,
    pub mode: String,
    pub url: String,
    pub current_file: String,
    pub files_done: usize,
    pub files_total: usize,
    pub deduped_pair_files: usize,
    pub imported_files: usize,
    pub skipped_unchanged_files: usize,
    pub imported_patents: usize,
    pub filtered_out: usize,
    pub chunks: usize,
    pub bytes_downloaded: u64,
    pub rows_per_sec: usize,
    pub free_disk_mb: Option<u64>,
    pub elapsed_secs: u64,
    pub sample_numbers: Vec<String>,
    pub summary: String,
    pub errors: Vec<String>,
}

impl Default for RemoteSyncJobSnapshot {
    fn default() -> Self {
        Self {
            state: "idle".to_string(),
            mode: "webdav".to_string(),
            url: String::new(),
            current_file: String::new(),
            files_done: 0,
            files_total: 0,
            deduped_pair_files: 0,
            imported_files: 0,
            skipped_unchanged_files: 0,
            imported_patents: 0,
            filtered_out: 0,
            chunks: 0,
            bytes_downloaded: 0,
            rows_per_sec: 0,
            free_disk_mb: None,
            elapsed_secs: 0,
            sample_numbers: Vec::new(),
            summary: String::new(),
            errors: Vec::new(),
        }
    }
}

struct RemoteSyncJobManager {
    snapshot: RemoteSyncJobSnapshot,
    cancel_flag: Arc<AtomicBool>,
}

impl Default for RemoteSyncJobManager {
    fn default() -> Self {
        Self {
            snapshot: RemoteSyncJobSnapshot::default(),
            cancel_flag: Arc::new(AtomicBool::new(false)),
        }
    }
}

static REMOTE_SYNC_MANAGER: OnceLock<Mutex<RemoteSyncJobManager>> = OnceLock::new();

fn remote_sync_manager() -> &'static Mutex<RemoteSyncJobManager> {
    REMOTE_SYNC_MANAGER.get_or_init(|| Mutex::new(RemoteSyncJobManager::default()))
}

fn current_remote_sync_snapshot() -> RemoteSyncJobSnapshot {
    remote_sync_manager()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .snapshot
        .clone()
}

fn update_remote_sync_snapshot(mutator: impl FnOnce(&mut RemoteSyncJobSnapshot)) {
    let mut guard = remote_sync_manager()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    mutator(&mut guard.snapshot);
}

#[derive(Debug, Clone)]
struct RemoteAuth {
    username: String,
    password: String,
    token: String,
}

#[derive(Debug, Clone)]
struct RemoteFileEntry {
    rel_path: String,
    download_url: String,
    sig: String,
    size: Option<u64>,
    kind: SyncFileKind,
}

fn normalize_remote_mode(raw: &str) -> &'static str {
    match raw.trim().to_ascii_lowercase().as_str() {
        "manifest" | "http" | "https" | "r2" | "oss" | "cos" | "s3" => "manifest",
        _ => "webdav",
    }
}

fn validate_remote_url(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }
    let parsed = reqwest::Url::parse(trimmed).map_err(|_| {
        "远端地址格式无效，请输入完整的 http:// 或 https:// 地址（如 https://example.com/dav/patents）"
            .to_string()
    })?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("远端同步仅支持 http:// 或 https:// 协议".to_string());
    }
    if parsed.host_str().is_none() {
        return Err("远端地址缺少主机名".to_string());
    }
    Ok(trimmed.to_string())
}

fn host_port_key(url: &reqwest::Url) -> (String, Option<u16>) {
    (
        url.host_str().unwrap_or("").to_ascii_lowercase(),
        url.port_or_known_default(),
    )
}

fn build_remote_http_client(bypass_proxy: bool) -> Result<reqwest::Client, String> {
    let mut builder = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(180))
        .user_agent(concat!("InnoForge/", env!("CARGO_PKG_VERSION")));
    if bypass_proxy {
        builder = builder.no_proxy();
    }
    builder
        .build()
        .map_err(|e| format!("初始化远端 HTTP 客户端失败: {e}"))
}

/// 发起带安全重定向跟随的 HTTP 请求：同源跳转保留认证头，跨域名跳转（如 OpenList -> 天翼云 CDN 302 直链）
/// 自动剥离 Basic/Bearer 认证头，绝不向第三方 CDN 域名泄漏用户口令。
async fn send_with_safe_redirects(
    client: &reqwest::Client,
    method: reqwest::Method,
    initial_url: &str,
    auth: &RemoteAuth,
    extra_headers: &[(&str, &str)],
    body_bytes: Option<Vec<u8>>,
) -> Result<(reqwest::Response, reqwest::Url), String> {
    let origin_url = reqwest::Url::parse(initial_url)
        .map_err(|e| format!("URL 解析失败 ({initial_url}): {e}"))?;
    let origin_hp = host_port_key(&origin_url);
    let mut current_url = origin_url;
    let mut current_method = method;

    for _hop in 0..=MAX_REDIRECT_HOPS {
        let mut req = client.request(current_method.clone(), current_url.clone());
        let same_origin = host_port_key(&current_url) == origin_hp;
        if same_origin {
            if !auth.token.trim().is_empty() {
                req = req.bearer_auth(auth.token.trim());
            } else if !auth.username.trim().is_empty() || !auth.password.is_empty() {
                req = req.basic_auth(auth.username.trim(), Some(auth.password.as_str()));
            }
            for (k, v) in extra_headers {
                req = req.header(*k, *v);
            }
            if let Some(ref payload) = body_bytes {
                req = req.body(payload.clone());
            }
        }

        let resp = req
            .send()
            .await
            .map_err(|e| format!("请求远端失败 ({current_url}): {e}"))?;

        let status = resp.status();
        if matches!(status.as_u16(), 301 | 302 | 303 | 307 | 308) {
            let Some(loc) = resp.headers().get(reqwest::header::LOCATION) else {
                return Ok((resp, current_url));
            };
            let loc_str = loc
                .to_str()
                .map_err(|_| "远端重定向 Location 头包含非法字符".to_string())?;
            let next_url = current_url
                .join(loc_str)
                .map_err(|e| format!("解析重定向地址失败 ({loc_str}): {e}"))?;
            if !matches!(next_url.scheme(), "http" | "https") {
                return Err(format!(
                    "拒绝重定向到非 HTTP(S) 协议: {}",
                    next_url.scheme()
                ));
            }
            if status.as_u16() == 303 {
                current_method = reqwest::Method::GET;
            }
            current_url = next_url;
            continue;
        }

        return Ok((resp, current_url));
    }

    Err(format!("远端重定向超过 {MAX_REDIRECT_HOPS} 次上限"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DavItem {
    href: String,
    is_dir: bool,
    size: Option<u64>,
    last_modified: String,
    etag: String,
}

fn unescape_xml_entities(raw: &str) -> String {
    raw.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

/// 在 XML 片段中提取指定本地标签名的首个内部文本（忽略命名空间前缀与大小写，禁止裸字节切片）。
fn extract_xml_tag_text(block: &str, local_name: &str) -> Option<String> {
    let lower = block.to_ascii_lowercase();
    let target = local_name.to_ascii_lowercase();
    let mut search_from = 0usize;

    while let Some(rel_lt) = lower.get(search_from..)?.find('<') {
        let open_pos = search_from + rel_lt;
        let after_lt = open_pos + 1;
        let rest = lower.get(after_lt..)?;
        if rest.starts_with('/') || rest.starts_with('!') || rest.starts_with('?') {
            search_from = after_lt;
            continue;
        }
        let rel_gt = rest.find('>')?;
        let tag_header = rest.get(..rel_gt)?;
        let self_closing = tag_header.trim_end().ends_with('/');
        let tag_name_full = tag_header
            .split_whitespace()
            .next()
            .unwrap_or("")
            .trim_end_matches('/');
        let tag_local = tag_name_full
            .rsplit_once(':')
            .map(|(_, l)| l)
            .unwrap_or(tag_name_full);

        let content_start = after_lt + rel_gt + 1;
        if tag_local == target {
            if self_closing {
                return Some(String::new());
            }
            let close_pat_a = format!("</{tag_name_full}>");
            let close_pat_b = format!("</{target}>");
            let tail = lower.get(content_start..)?;
            let rel_close = tail
                .find(&close_pat_a)
                .or_else(|| tail.find(&close_pat_b))?;
            let raw_val = block.get(content_start..content_start + rel_close)?;
            return Some(unescape_xml_entities(raw_val.trim()));
        }
        search_from = content_start;
    }
    None
}

/// 拆分 WebDAV `multistatus` XML 中的 `<*:response>...</*:response>` 条目并提取基础元数据。
fn parse_webdav_multistatus_xml(xml: &str) -> Vec<DavItem> {
    let lower = xml.to_ascii_lowercase();
    let mut items = Vec::new();
    let mut cursor = 0usize;

    while let Some(tail) = lower.get(cursor..) {
        let Some(rel_lt) = tail.find('<') else {
            break;
        };
        let open_pos = cursor + rel_lt;
        let Some(after_lt_slice) = lower.get(open_pos + 1..) else {
            break;
        };
        let Some(rel_gt) = after_lt_slice.find('>') else {
            break;
        };
        let Some(tag_header) = after_lt_slice.get(..rel_gt) else {
            break;
        };
        let tag_name_full = tag_header
            .split_whitespace()
            .next()
            .unwrap_or("")
            .trim_end_matches('/');
        let tag_local = tag_name_full
            .rsplit_once(':')
            .map(|(_, l)| l)
            .unwrap_or(tag_name_full);

        let body_start = open_pos + 1 + rel_gt + 1;
        if !tag_name_full.starts_with('/') && tag_local == "response" {
            let close_a = format!("</{tag_name_full}>");
            let close_b = "</response>";
            if let Some(after_body) = lower.get(body_start..) {
                if let Some(rel_end) = after_body
                    .find(&close_a)
                    .or_else(|| after_body.find(close_b))
                {
                    if let Some(block) = xml.get(body_start..body_start + rel_end) {
                        if let Some(href) = extract_xml_tag_text(block, "href") {
                            let resourcetype =
                                extract_xml_tag_text(block, "resourcetype").unwrap_or_default();
                            let rt_lower = resourcetype.to_ascii_lowercase();
                            let is_dir = rt_lower.contains("collection")
                                || (href.ends_with('/')
                                    && extract_xml_tag_text(block, "getcontentlength").is_none());
                            let size = extract_xml_tag_text(block, "getcontentlength")
                                .and_then(|s| s.trim().parse::<u64>().ok());
                            let last_modified =
                                extract_xml_tag_text(block, "getlastmodified").unwrap_or_default();
                            let etag = extract_xml_tag_text(block, "getetag")
                                .unwrap_or_default()
                                .trim_matches('"')
                                .to_string();
                            items.push(DavItem {
                                href,
                                is_dir,
                                size,
                                last_modified,
                                etag,
                            });
                        }
                    }
                    cursor = body_start + rel_end + close_b.len();
                    continue;
                }
            }
        }
        cursor = body_start;
    }
    items
}

fn dedup_remote_table_entries(entries: Vec<RemoteFileEntry>) -> (Vec<RemoteFileEntry>, usize) {
    let mut best_rank: HashMap<String, u8> = HashMap::new();
    for entry in &entries {
        if entry.kind == SyncFileKind::Table {
            if let Some((stem, rank)) = table_dedup_stem_and_rank(&entry.rel_path) {
                let cur = best_rank.entry(stem).or_insert(0);
                if rank > *cur {
                    *cur = rank;
                }
            }
        }
    }
    if best_rank.is_empty() {
        return (entries, 0);
    }
    let mut kept = Vec::with_capacity(entries.len());
    let mut deduped = 0usize;
    for entry in entries {
        if entry.kind == SyncFileKind::Table {
            if let Some((stem, rank)) = table_dedup_stem_and_rank(&entry.rel_path) {
                if best_rank.get(&stem).is_some_and(|best| rank < *best) {
                    deduped += 1;
                    continue;
                }
            }
        }
        kept.push(entry);
    }
    (kept, deduped)
}

fn path_has_skipped_segment(rel_path: &str) -> bool {
    let parts: Vec<&str> = rel_path.split('/').filter(|s| !s.is_empty()).collect();
    if parts.is_empty() {
        return true;
    }
    for dir_seg in parts.iter().take(parts.len().saturating_sub(1)) {
        if should_skip_sync_dir(dir_seg) {
            return true;
        }
    }
    let filename = parts.last().copied().unwrap_or("");
    is_temp_download_filename(filename)
}

/// 递归枚举 WebDAV 目录中的可同步专利文件（最多 4 层、500 个文件，自动跳过 `work_*` 与临时文件并去重）。
async fn discover_webdav_files(
    client: &reqwest::Client,
    root_url_str: &str,
    auth: &RemoteAuth,
) -> Result<(Vec<RemoteFileEntry>, usize), String> {
    let normalized_root = if root_url_str.ends_with('/') {
        root_url_str.to_string()
    } else {
        format!("{root_url_str}/")
    };
    let root_url =
        reqwest::Url::parse(&normalized_root).map_err(|e| format!("WebDAV 地址解析失败: {e}"))?;
    let root_path_decoded = urlencoding::decode(root_url.path())
        .map(|c| c.into_owned())
        .unwrap_or_else(|_| root_url.path().to_string());
    let root_prefix = root_path_decoded.trim_end_matches('/').to_string();

    let propfind_method = reqwest::Method::from_bytes(b"PROPFIND")
        .map_err(|e| format!("构造 PROPFIND 方法失败: {e}"))?;
    let propfind_body = br#"<?xml version="1.0" encoding="utf-8"?><d:propfind xmlns:d="DAV:"><d:prop><d:resourcetype/><d:getcontentlength/><d:getlastmodified/><d:getetag/></d:prop></d:propfind>"#.to_vec();

    let mut queue: Vec<(reqwest::Url, usize)> = vec![(root_url.clone(), 0)];
    let mut visited: HashSet<String> = HashSet::new();
    let mut found: Vec<RemoteFileEntry> = Vec::new();

    while let Some((dir_url, depth)) = queue.pop() {
        if found.len() >= MAX_REMOTE_FILES {
            break;
        }
        let dir_key = dir_url.path().trim_end_matches('/').to_string();
        if !visited.insert(dir_key.clone()) {
            continue;
        }

        let (resp, resolved_dir_url) = send_with_safe_redirects(
            client,
            propfind_method.clone(),
            dir_url.as_str(),
            auth,
            &[
                ("Depth", "1"),
                ("Content-Type", "application/xml; charset=utf-8"),
            ],
            Some(propfind_body.clone()),
        )
        .await?;

        let status = resp.status();
        if !status.is_success() && status.as_u16() != 207 {
            let hint = match status.as_u16() {
                401 | 403 => "（认证失败或无只读权限，请检查 WebDAV 账号/密码/Token 或基本路径权限）",
                404 => "（目录不存在，请检查路径是否包含 /dav/...）",
                405 => "（目标地址不支持 WebDAV PROPFIND，若是 R2/OSS 直链请切换为「HTTP 清单订阅」模式）",
                _ => "",
            };
            return Err(format!("WebDAV PROPFIND 返回 HTTP {status}{hint}"));
        }

        let xml_body = resp
            .text()
            .await
            .map_err(|e| format!("读取 WebDAV 响应失败: {e}"))?;
        let dav_items = parse_webdav_multistatus_xml(&xml_body);
        let cur_decoded = urlencoding::decode(resolved_dir_url.path())
            .map(|c| c.into_owned())
            .unwrap_or_else(|_| resolved_dir_url.path().to_string());
        let cur_trimmed = cur_decoded.trim_end_matches('/');

        for item in dav_items {
            if found.len() >= MAX_REMOTE_FILES {
                break;
            }
            let Ok(item_url) = resolved_dir_url.join(&item.href) else {
                continue;
            };
            let item_path_decoded = urlencoding::decode(item_url.path())
                .map(|c| c.into_owned())
                .unwrap_or_else(|_| item_url.path().to_string());
            let item_trimmed = item_path_decoded.trim_end_matches('/');

            // 跳过当前目录自身
            if item_trimmed == cur_trimmed || item_trimmed.is_empty() {
                continue;
            }
            let rel = if !root_prefix.is_empty() {
                item_trimmed
                    .strip_prefix(&root_prefix)
                    .unwrap_or(item_trimmed)
                    .trim_start_matches('/')
                    .to_string()
            } else {
                item_trimmed.trim_start_matches('/').to_string()
            };
            if rel.is_empty() {
                continue;
            }

            if item.is_dir {
                let dir_name = rel.rsplit('/').next().unwrap_or(&rel);
                if depth < MAX_REMOTE_DEPTH && !should_skip_sync_dir(dir_name) {
                    let mut next_url = item_url.clone();
                    if !next_url.path().ends_with('/') {
                        let new_path = format!("{}/", next_url.path());
                        next_url.set_path(&new_path);
                    }
                    queue.push((next_url, depth + 1));
                }
                continue;
            }

            if path_has_skipped_segment(&rel) {
                continue;
            }
            let filename = rel.rsplit('/').next().unwrap_or(&rel);
            let Some(kind) = classify_sync_filename(filename) else {
                continue;
            };
            if let Some(sz) = item.size {
                if sz == 0 || sz > MAX_REMOTE_FILE_BYTES {
                    continue;
                }
            }
            let sig = if !item.etag.is_empty() {
                format!("etag:{}:{}", item.size.unwrap_or(0), item.etag)
            } else {
                format!("lm:{}:{}", item.size.unwrap_or(0), item.last_modified)
            };
            found.push(RemoteFileEntry {
                rel_path: rel,
                download_url: item_url.to_string(),
                sig,
                size: item.size,
                kind,
            });
        }
    }

    found.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    let (mut deduped, deduped_pairs) = dedup_remote_table_entries(found);
    deduped.truncate(MAX_REMOTE_FILES);
    Ok((deduped, deduped_pairs))
}

/// 解析 HTTP(S) 清单 JSON（兼容 `{ "files": [...] }` 与顶层数组 `[...]`，适用于 R2 / OSS / COS / MinIO / 静态站）。
fn parse_remote_manifest_json(
    manifest_url: &reqwest::Url,
    json_val: &Value,
) -> Result<(Vec<RemoteFileEntry>, usize), String> {
    let arr = if let Some(list) = json_val.get("files").and_then(Value::as_array) {
        list
    } else if let Some(list) = json_val.as_array() {
        list
    } else {
        return Err("清单 JSON 格式无效：需要包含 `files` 数组或顶层文件数组".to_string());
    };

    let mut entries = Vec::new();
    for item in arr {
        if entries.len() >= MAX_REMOTE_FILES {
            break;
        }
        let raw_path = item
            .get("path")
            .or_else(|| item.get("name"))
            .or_else(|| item.get("key"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        let explicit_url = item
            .get("url")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty());

        let rel = raw_path
            .replace('\\', "/")
            .trim_start_matches('/')
            .to_string();
        if rel.is_empty() || rel.contains("..") || path_has_skipped_segment(&rel) {
            continue;
        }
        let filename = rel.rsplit('/').next().unwrap_or(&rel);
        let Some(kind) = classify_sync_filename(filename) else {
            continue;
        };
        let size = item.get("size").and_then(Value::as_u64);
        if let Some(sz) = size {
            if sz == 0 || sz > MAX_REMOTE_FILE_BYTES {
                continue;
            }
        }

        let download_url = if let Some(u) = explicit_url {
            match reqwest::Url::parse(u) {
                Ok(parsed) if matches!(parsed.scheme(), "http" | "https") => parsed.to_string(),
                _ => match manifest_url.join(u) {
                    Ok(joined) => joined.to_string(),
                    Err(_) => continue,
                },
            }
        } else {
            match manifest_url.join(&rel) {
                Ok(joined) => joined.to_string(),
                Err(_) => continue,
            }
        };

        let sha = item.get("sha256").and_then(Value::as_str).unwrap_or("");
        let etag = item
            .get("etag")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim_matches('"');
        let mtime = item
            .get("mtime")
            .and_then(|v| {
                v.as_u64()
                    .map(|n| n.to_string())
                    .or_else(|| v.as_str().map(str::to_string))
            })
            .unwrap_or_default();
        let sig = if !sha.is_empty() {
            format!("sha256:{}:{sha}", size.unwrap_or(0))
        } else if !etag.is_empty() {
            format!("etag:{}:{etag}", size.unwrap_or(0))
        } else {
            format!("mt:{}:{mtime}", size.unwrap_or(0))
        };

        entries.push(RemoteFileEntry {
            rel_path: rel,
            download_url,
            sig,
            size,
            kind,
        });
    }

    entries.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    let (mut deduped, deduped_pairs) = dedup_remote_table_entries(entries);
    deduped.truncate(MAX_REMOTE_FILES);
    Ok((deduped, deduped_pairs))
}

async fn discover_manifest_files(
    client: &reqwest::Client,
    raw_url: &str,
    auth: &RemoteAuth,
) -> Result<(Vec<RemoteFileEntry>, usize), String> {
    let trimmed = raw_url.trim();
    let candidates: Vec<String> = if trimmed.to_ascii_lowercase().ends_with(".json") {
        vec![trimmed.to_string()]
    } else {
        let base = trimmed.trim_end_matches('/');
        vec![
            format!("{base}/innoforge-feed.json"),
            format!("{base}/manifest.json"),
        ]
    };

    let mut last_err = String::new();
    for cand in candidates {
        let (resp, final_url) = send_with_safe_redirects(
            client,
            reqwest::Method::GET,
            &cand,
            auth,
            &[("Accept", "application/json")],
            None,
        )
        .await?;

        if resp.status().is_success() {
            let text = resp
                .text()
                .await
                .map_err(|e| format!("读取远端清单失败: {e}"))?;
            let json_val: Value = serde_json::from_str(&text)
                .map_err(|e| format!("解析远端清单 JSON 失败 ({cand}): {e}"))?;
            return parse_remote_manifest_json(&final_url, &json_val);
        }
        last_err = format!("GET {cand} 返回 HTTP {}", resp.status());
        if resp.status().as_u16() == 401 || resp.status().as_u16() == 403 {
            return Err(format!(
                "{last_err}（认证失败，请检查只读 Token 或 Basic 账号密码）"
            ));
        }
    }

    Err(format!(
        "无法读取远端清单（{last_err}）；请确认已上传 innoforge-feed.json 或改用 WebDAV 模式"
    ))
}

async fn discover_remote_files(
    client: &reqwest::Client,
    mode: &str,
    url: &str,
    auth: &RemoteAuth,
) -> Result<(Vec<RemoteFileEntry>, usize), String> {
    if normalize_remote_mode(mode) == "manifest" {
        discover_manifest_files(client, url, auth).await
    } else {
        discover_webdav_files(client, url, auth).await
    }
}

/// 一次性可丢弃临时文件守卫（下载完单文件并流式写库后立即自动删除，绝不残留垃圾文件）。
struct TempFileGuard {
    path: PathBuf,
}

impl Drop for TempFileGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// 将远端文件流式下载到一次性临时文件（支持跨域名 302 直链、大小红线校验与随时取消）。
async fn stream_download_to_temp(
    client: &reqwest::Client,
    url: &str,
    filename_hint: &str,
    auth: &RemoteAuth,
    cancel: &AtomicBool,
    mut on_bytes: impl FnMut(u64),
) -> Result<(TempFileGuard, u64), String> {
    let (resp, _) =
        send_with_safe_redirects(client, reqwest::Method::GET, url, auth, &[], None).await?;
    if !resp.status().is_success() {
        return Err(format!("下载返回 HTTP {}", resp.status()));
    }
    if let Some(len) = resp.content_length() {
        if len > MAX_REMOTE_FILE_BYTES {
            return Err(format!(
                "文件大小 {} MB 超过单文件 {} MB 安全上限",
                len / (1024 * 1024),
                MAX_REMOTE_FILE_BYTES / (1024 * 1024)
            ));
        }
    }

    let safe_suffix: String = filename_hint
        .chars()
        .rev()
        .take(40)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let tmp_dir = std::env::temp_dir();
    let tmp_path = tmp_dir.join(format!(
        "innoforge_remote_{}_{}",
        uuid::Uuid::new_v4().simple(),
        safe_suffix
    ));
    let mut file =
        std::fs::File::create(&tmp_path).map_err(|e| format!("创建临时下载文件失败: {e}"))?;
    let guard = TempFileGuard { path: tmp_path };

    let mut downloaded = 0u64;
    let mut stream = resp.bytes_stream();
    while let Some(chunk_res) = stream.next().await {
        if cancel.load(Ordering::Relaxed) {
            return Err("CANCELLED".to_string());
        }
        let chunk = chunk_res.map_err(|e| format!("读取远端数据流失败: {e}"))?;
        downloaded = downloaded.saturating_add(chunk.len() as u64);
        if downloaded > MAX_REMOTE_FILE_BYTES {
            return Err(format!(
                "下载流超过单文件 {} MB 安全上限",
                MAX_REMOTE_FILE_BYTES / (1024 * 1024)
            ));
        }
        file.write_all(&chunk)
            .map_err(|e| format!("写入临时文件失败: {e}"))?;
        on_bytes(chunk.len() as u64);
    }
    file.flush().map_err(|e| format!("刷新临时文件失败: {e}"))?;
    Ok((guard, downloaded))
}

#[allow(clippy::too_many_arguments)]
async fn execute_remote_sync_worker(
    db: Arc<crate::db::Database>,
    mode: String,
    url: String,
    auth: RemoteAuth,
    ipc_prefix: String,
    lightweight: bool,
    force: bool,
    cancel_flag: Arc<AtomicBool>,
    bypass_proxy: bool,
    update_global_job: bool,
) -> Result<Value, String> {
    let started_at = Instant::now();
    let client = build_remote_http_client(bypass_proxy)?;
    let (files, deduped_pairs) = discover_remote_files(&client, &mode, &url, &auth).await?;

    let initial_free_mb = free_disk_mb_for_db(&db);
    if update_global_job {
        update_remote_sync_snapshot(|snap| {
            snap.state = "running".to_string();
            snap.mode = mode.clone();
            snap.url = url.clone();
            snap.files_done = 0;
            snap.files_total = files.len();
            snap.deduped_pair_files = deduped_pairs;
            snap.imported_files = 0;
            snap.skipped_unchanged_files = 0;
            snap.imported_patents = 0;
            snap.filtered_out = 0;
            snap.chunks = 0;
            snap.bytes_downloaded = 0;
            snap.rows_per_sec = 0;
            snap.free_disk_mb = initial_free_mb;
            snap.elapsed_secs = 0;
            snap.sample_numbers.clear();
            snap.errors.clear();
            snap.summary.clear();
        });
    }

    let prev_manifest_raw = db
        .get_setting(SETTING_REMOTE_MANIFEST)
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
    let mut total_bytes = 0u64;
    let mut sample_numbers: Vec<String> = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    let mut was_cancelled = false;
    let mut paused_disk_low_mb: Option<u64> = None;

    let options = PatentTableImportOptions {
        ipc_prefix: ipc_prefix.clone(),
        lightweight,
        source_tag: format!("remote_{mode}"),
    };

    for (idx, entry) in files.iter().enumerate() {
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
            update_remote_sync_snapshot(|snap| {
                snap.current_file = entry.rel_path.clone();
                snap.files_done = idx;
                snap.free_disk_mb = free_mb;
                snap.elapsed_secs = started_at.elapsed().as_secs();
            });
        }

        let manifest_key = format!("{}|{}|{}", entry.rel_path, ipc_prefix, lightweight);
        if !force
            && manifest
                .get(&manifest_key)
                .is_some_and(|old| old == &entry.sig)
        {
            skipped_unchanged_files += 1;
            if update_global_job {
                update_remote_sync_snapshot(|snap| {
                    snap.files_done = idx + 1;
                    snap.skipped_unchanged_files = skipped_unchanged_files;
                });
            }
            continue;
        }

        let filename = entry
            .rel_path
            .rsplit('/')
            .next()
            .unwrap_or(&entry.rel_path)
            .to_string();

        let dl_res = stream_download_to_temp(
            &client,
            &entry.download_url,
            &filename,
            &auth,
            &cancel_flag,
            |delta| {
                total_bytes = total_bytes.saturating_add(delta);
                if update_global_job {
                    update_remote_sync_snapshot(|snap| {
                        snap.bytes_downloaded = total_bytes;
                        snap.elapsed_secs = started_at.elapsed().as_secs();
                    });
                }
            },
        )
        .await;

        let (tmp_guard, _sz) = match dl_res {
            Ok(v) => v,
            Err(e) if e == "CANCELLED" => {
                was_cancelled = true;
                break;
            }
            Err(e) => {
                errors.push(format!("{}: {e}", entry.rel_path));
                continue;
            }
        };

        let is_stream_jsonl = matches!(table_ext_for_filename(&filename), Some(("jsonl", _)));
        let tmp_path = tmp_guard.path.clone();
        let db_w = db.clone();
        let opts_w = options.clone();
        let cancel_w = cancel_flag.clone();
        let filename_w = filename.clone();
        let rel_w = entry.rel_path.clone();

        if is_stream_jsonl {
            let base_imp = total_imported;
            let base_flt = total_filtered;
            let base_chk = total_chunks;
            let stream_res = tokio::task::spawn_blocking(move || {
                let mut acc_imp = base_imp;
                let mut acc_flt = base_flt;
                let mut acc_chk = base_chk;
                sync_stream_jsonl_file(
                    &db_w,
                    &tmp_path,
                    &opts_w,
                    &cancel_w,
                    |imp, flt, chks, smps| {
                        acc_imp += imp;
                        acc_flt += flt;
                        acc_chk += chks;
                        if update_global_job {
                            let secs = started_at.elapsed().as_secs().max(1);
                            let rate = (acc_imp + acc_flt) / secs as usize;
                            update_remote_sync_snapshot(|snap| {
                                snap.imported_patents = acc_imp;
                                snap.filtered_out = acc_flt;
                                snap.chunks = acc_chk;
                                snap.rows_per_sec = rate;
                                snap.elapsed_secs = secs;
                                for s in smps {
                                    if snap.sample_numbers.len() < 5
                                        && !snap.sample_numbers.contains(s)
                                    {
                                        snap.sample_numbers.push(s.clone());
                                    }
                                }
                            });
                        }
                    },
                )
            })
            .await
            .map_err(|e| format!("流式入库线程异常: {e}"))?;

            drop(tmp_guard);
            match stream_res {
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
                    manifest.insert(manifest_key, entry.sig.clone());
                    if out.imported > 0 {
                        imported_files += 1;
                    }
                }
                Err(e) => {
                    errors.push(format!("{rel_w}: {e}"));
                }
            }
        } else {
            let parse_res: Result<(usize, usize, usize, Vec<String>, bool), String> =
                tokio::task::spawn_blocking(
                    move || -> Result<(usize, usize, usize, Vec<String>, bool), String> {
                        let bytes = std::fs::read(&tmp_path)
                            .map_err(|e| format!("读取临时文件失败: {e}"))?;
                        let parsed = parse_any_patent_bytes(&filename_w, &bytes, &opts_w)?;
                        if parsed.matched_patents.is_empty() {
                            return Ok((0usize, parsed.filtered_out, 0usize, Vec::new(), false));
                        }
                        let mut smps = Vec::new();
                        for p in &parsed.matched_patents {
                            if smps.len() >= 5 {
                                break;
                            }
                            let label = if !p.patent_number.is_empty() {
                                p.patent_number.clone()
                            } else {
                                p.title.clone()
                            };
                            if !label.is_empty() && !smps.contains(&label) {
                                smps.push(label);
                            }
                        }
                        let (imp, chks, cancelled) = db_w
                            .batch_import_patents_chunked(
                                &parsed.matched_patents,
                                opts_w.lightweight,
                                "char-tfidf-v2",
                                Some(&cancel_w),
                            )
                            .map_err(|e| format!("入库失败: {e}"))?;
                        Ok((imp, parsed.filtered_out, chks, smps, cancelled))
                    },
                )
                .await
                .map_err(|e| format!("远端解析线程异常: {e}"))?;

            drop(tmp_guard);
            match parse_res {
                Ok((imp, flt, chks, smps, cancelled)) => {
                    total_imported += imp;
                    total_filtered += flt;
                    total_chunks += chks;
                    for s in smps {
                        if sample_numbers.len() < 5 && !sample_numbers.contains(&s) {
                            sample_numbers.push(s);
                        }
                    }
                    if cancelled {
                        was_cancelled = true;
                        break;
                    }
                    manifest.insert(manifest_key, entry.sig.clone());
                    if imp > 0 {
                        imported_files += 1;
                    }
                }
                Err(e) => {
                    errors.push(format!("{rel_w}: {e}"));
                }
            }
        }

        if let Ok(manifest_json) = serde_json::to_string(&manifest) {
            let _ = db.set_settings_batch(&[(SETTING_REMOTE_MANIFEST, &manifest_json)]);
        }
        if update_global_job {
            let secs = started_at.elapsed().as_secs().max(1);
            let rate = (total_imported + total_filtered) / secs as usize;
            update_remote_sync_snapshot(|snap| {
                snap.files_done = idx + 1;
                snap.imported_files = imported_files;
                snap.skipped_unchanged_files = skipped_unchanged_files;
                snap.imported_patents = total_imported;
                snap.filtered_out = total_filtered;
                snap.chunks = total_chunks;
                snap.bytes_downloaded = total_bytes;
                snap.rows_per_sec = rate;
                snap.elapsed_secs = secs;
                snap.sample_numbers = sample_numbers.clone();
                snap.errors = errors.clone();
            });
        }
    }

    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let final_state = if let Some(mb) = paused_disk_low_mb {
        errors.insert(
            0,
            format!("磁盘剩余仅 {mb} MB（低于 {MIN_FREE_DISK_MB} MB 红线），已自动暂停远端同步"),
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
    let mb_dl = (total_bytes as f64) / (1024.0 * 1024.0);
    let summary = format!(
        "{state_prefix}远端扫描 {} 个文件：新增入库 {} 个（共 {} 件专利，过滤 {}），跳过未变更 {} 个（下载 {:.1} MB）",
        files.len(),
        imported_files,
        total_imported,
        total_filtered,
        skipped_unchanged_files,
        mb_dl
    );
    let manifest_json = serde_json::to_string(&manifest).unwrap_or_else(|_| "{}".to_string());
    let _ = db.set_settings_batch(&[
        (SETTING_REMOTE_MANIFEST, &manifest_json),
        (SETTING_REMOTE_LAST_AT, &now),
        (SETTING_REMOTE_LAST_SUMMARY, &summary),
    ]);

    let final_free_mb = free_disk_mb_for_db(&db);
    let elapsed_secs = started_at.elapsed().as_secs();
    let rate = (total_imported + total_filtered) / elapsed_secs.max(1) as usize;
    if update_global_job {
        update_remote_sync_snapshot(|snap| {
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
            snap.bytes_downloaded = total_bytes;
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
        "mode": mode,
        "url": url,
        "scanned_files": files.len(),
        "deduped_pair_files": deduped_pairs,
        "imported_files": imported_files,
        "skipped_unchanged_files": skipped_unchanged_files,
        "imported_patents": total_imported,
        "filtered_out": total_filtered,
        "chunks": total_chunks,
        "bytes_downloaded": total_bytes,
        "free_disk_mb": final_free_mb,
        "elapsed_secs": elapsed_secs,
        "sample_numbers": sample_numbers,
        "last_at": now,
        "summary": summary,
        "errors": errors,
    }))
}

fn load_remote_config_from_db(
    db: &crate::db::Database,
) -> (
    String,
    String,
    RemoteAuth,
    String,
    bool,
    u64,
    String,
    String,
) {
    let mode = db
        .get_setting(SETTING_REMOTE_MODE)
        .ok()
        .flatten()
        .map(|s| normalize_remote_mode(&s).to_string())
        .unwrap_or_else(|| "webdav".to_string());
    let url = db
        .get_setting(SETTING_REMOTE_URL)
        .ok()
        .flatten()
        .unwrap_or_default();
    let username = db
        .get_setting(SETTING_REMOTE_USERNAME)
        .ok()
        .flatten()
        .unwrap_or_default();
    let password = db
        .get_setting(SETTING_REMOTE_PASSWORD)
        .ok()
        .flatten()
        .unwrap_or_default();
    let token = db
        .get_setting(SETTING_REMOTE_TOKEN)
        .ok()
        .flatten()
        .unwrap_or_default();
    let ipc_prefix = db
        .get_setting(SETTING_REMOTE_IPC_PREFIX)
        .ok()
        .flatten()
        .unwrap_or_default();
    let lightweight = db
        .get_setting(SETTING_REMOTE_LIGHTWEIGHT)
        .ok()
        .flatten()
        .map(|v| v == "1" || v == "true")
        .unwrap_or(true);
    let auto_mins = db
        .get_setting(SETTING_REMOTE_AUTO_MINS)
        .ok()
        .flatten()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .map(normalize_auto_interval_mins)
        .unwrap_or(0);
    let last_at = db
        .get_setting(SETTING_REMOTE_LAST_AT)
        .ok()
        .flatten()
        .unwrap_or_default();
    let last_summary = db
        .get_setting(SETTING_REMOTE_LAST_SUMMARY)
        .ok()
        .flatten()
        .unwrap_or_default();

    (
        mode,
        url,
        RemoteAuth {
            username,
            password,
            token,
        },
        ipc_prefix,
        lightweight,
        auto_mins,
        last_at,
        last_summary,
    )
}

/// 启动远端网盘/对象存储定时增量巡检循环（WebDAV / HTTP 清单通用，零开销跳过无更新周期）。
pub fn spawn_remote_auto_sync_loop(db: Arc<crate::db::Database>) {
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        let mut last_attempt: Option<Instant> = None;

        loop {
            let db_cfg = db.clone();
            let cfg_res =
                tokio::task::spawn_blocking(move || load_remote_config_from_db(&db_cfg)).await;

            if let Ok((mode, url, auth, ipc_prefix, lightweight, auto_mins, _, _)) = cfg_res {
                let due = auto_mins > 0
                    && !url.trim().is_empty()
                    && last_attempt
                        .map(|t| t.elapsed().as_secs() >= auto_mins.saturating_mul(60))
                        .unwrap_or(true);

                if due {
                    last_attempt = Some(Instant::now());
                    let disk_ok = free_disk_mb_for_db(&db)
                        .map(|mb| mb >= MIN_FREE_DISK_MB)
                        .unwrap_or(true);
                    if disk_ok {
                        let maybe_cancel = {
                            let mut guard = remote_sync_manager()
                                .lock()
                                .unwrap_or_else(|e| e.into_inner());
                            if guard.snapshot.state == "running" {
                                None
                            } else {
                                let new_cancel = Arc::new(AtomicBool::new(false));
                                guard.cancel_flag = new_cancel.clone();
                                guard.snapshot = RemoteSyncJobSnapshot {
                                    state: "running".to_string(),
                                    mode: mode.clone(),
                                    url: url.clone(),
                                    free_disk_mb: free_disk_mb_for_db(&db),
                                    ..Default::default()
                                };
                                Some(new_cancel)
                            }
                        };

                        if let Some(cancel_flag) = maybe_cancel {
                            let db_run = db.clone();
                            tokio::spawn(async move {
                                if let Err(err) = execute_remote_sync_worker(
                                    db_run,
                                    mode,
                                    url,
                                    auth,
                                    ipc_prefix,
                                    lightweight,
                                    false,
                                    cancel_flag,
                                    false,
                                    true,
                                )
                                .await
                                {
                                    update_remote_sync_snapshot(|snap| {
                                        snap.state = "error".to_string();
                                        snap.current_file.clear();
                                        snap.errors.push(err.clone());
                                        snap.summary = format!("远端自动同步失败: {err}");
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

/// GET `/api/patents/remote-sync` — 读取多网盘远端订阅配置与实时任务快照（绝不回显明文密码或 Token）
pub async fn api_get_patent_remote_sync(State(s): State<AppState>) -> Json<Value> {
    let db = s.db.clone();
    let db_path = db.main_db_file_path();
    let (mode, url, auth, ipc_prefix, lightweight, auto_mins, last_at, last_summary, free_disk_mb) =
        tokio::task::spawn_blocking(move || {
            let (m, u, a, ipc, lw, auto, la, ls) = load_remote_config_from_db(&db);
            let free = db_path.as_deref().and_then(free_disk_mb_for_path);
            (m, u, a, ipc, lw, auto, la, ls, free)
        })
        .await
        .unwrap_or_else(|_| {
            (
                "webdav".to_string(),
                String::new(),
                RemoteAuth {
                    username: String::new(),
                    password: String::new(),
                    token: String::new(),
                },
                String::new(),
                true,
                0,
                String::new(),
                String::new(),
                None,
            )
        });

    Json(json!({
        "status": "ok",
        "mode": mode,
        "url": url,
        "username": auth.username,
        "password_configured": !auth.password.is_empty(),
        "token_configured": !auth.token.is_empty(),
        "ipc_prefix": ipc_prefix,
        "lightweight": lightweight,
        "auto_interval_mins": auto_mins,
        "free_disk_mb": free_disk_mb,
        "last_at": last_at,
        "last_summary": last_summary,
        "job": current_remote_sync_snapshot()
    }))
}

/// GET `/api/patents/remote-sync/progress` — 查询远端多网盘同步后台进度
pub async fn api_get_patent_remote_sync_progress() -> Json<Value> {
    Json(json!({
        "status": "ok",
        "job": current_remote_sync_snapshot()
    }))
}

/// POST `/api/patents/remote-sync/cancel` — 优雅取消正在进行的远端同步（保留已完成断点）
pub async fn api_cancel_patent_remote_sync() -> Json<Value> {
    let snap = {
        let guard = remote_sync_manager()
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        guard.cancel_flag.store(true, Ordering::Relaxed);
        guard.snapshot.clone()
    };
    Json(json!({
        "status": "ok",
        "cancelling": snap.state == "running",
        "job": snap
    }))
}

/// POST `/api/patents/remote-sync` — 保存远端源配置、测试连通性（`action="test"`）或启动后台增量同步（`action="sync"`）
pub async fn api_patent_remote_sync(
    State(s): State<AppState>,
    Json(req): Json<RemoteSyncRequest>,
) -> Json<Value> {
    let mode = normalize_remote_mode(&req.mode).to_string();
    let url = match validate_remote_url(&req.url) {
        Ok(u) => u,
        Err(msg) => return Json(json!({ "status": "error", "message": msg })),
    };
    let validated_ipc = match validate_ipc_prefix_filter(&req.ipc_prefix) {
        Ok(v) => v,
        Err(msg) => return Json(json!({ "status": "error", "message": msg })),
    };

    let existing_password =
        s.db.get_setting(SETTING_REMOTE_PASSWORD)
            .ok()
            .flatten()
            .unwrap_or_default();
    let existing_token =
        s.db.get_setting(SETTING_REMOTE_TOKEN)
            .ok()
            .flatten()
            .unwrap_or_default();
    let effective_password = if req.clear_password {
        String::new()
    } else if let Some(p) = req.password.as_deref().filter(|p| !p.is_empty()) {
        p.to_string()
    } else {
        existing_password
    };
    let effective_token = if req.clear_token {
        String::new()
    } else if let Some(t) = req.token.as_deref().filter(|t| !t.is_empty()) {
        t.trim().to_string()
    } else {
        existing_token
    };
    let effective_auto_mins = if let Some(raw_mins) = req.auto_interval_mins {
        normalize_auto_interval_mins(raw_mins)
    } else {
        s.db.get_setting(SETTING_REMOTE_AUTO_MINS)
            .ok()
            .flatten()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .map(normalize_auto_interval_mins)
            .unwrap_or(0)
    };

    let lw_str = if req.lightweight { "1" } else { "0" };
    let auto_str = effective_auto_mins.to_string();
    let username_trimmed = req.username.trim().to_string();
    if let Err(e) = s.db.set_settings_batch(&[
        (SETTING_REMOTE_MODE, &mode),
        (SETTING_REMOTE_URL, &url),
        (SETTING_REMOTE_USERNAME, &username_trimmed),
        (SETTING_REMOTE_PASSWORD, &effective_password),
        (SETTING_REMOTE_TOKEN, &effective_token),
        (SETTING_REMOTE_IPC_PREFIX, &validated_ipc),
        (SETTING_REMOTE_LIGHTWEIGHT, lw_str),
        (SETTING_REMOTE_AUTO_MINS, &auto_str),
    ]) {
        return Json(json!({
            "status": "error",
            "message": format!("保存远端订阅配置失败: {e}")
        }));
    }

    let action = req.action.trim().to_ascii_lowercase();
    if action == "save" || url.is_empty() {
        return Json(json!({
            "status": "ok",
            "saved": true,
            "mode": mode,
            "url": url,
            "password_configured": !effective_password.is_empty(),
            "token_configured": !effective_token.is_empty(),
            "auto_interval_mins": effective_auto_mins,
        }));
    }

    let auth = RemoteAuth {
        username: username_trimmed,
        password: effective_password,
        token: effective_token,
    };

    if action == "test" {
        let client = match build_remote_http_client(false) {
            Ok(c) => c,
            Err(msg) => return Json(json!({ "status": "error", "message": msg })),
        };
        return match discover_remote_files(&client, &mode, &url, &auth).await {
            Ok((files, deduped_pairs)) => {
                let prev_manifest_raw =
                    s.db.get_setting(SETTING_REMOTE_MANIFEST)
                        .ok()
                        .flatten()
                        .unwrap_or_default();
                let manifest: HashMap<String, String> =
                    serde_json::from_str(&prev_manifest_raw).unwrap_or_default();
                let pending = files
                    .iter()
                    .filter(|f| {
                        let key = format!("{}|{}|{}", f.rel_path, validated_ipc, req.lightweight);
                        !manifest.get(&key).is_some_and(|old| old == &f.sig)
                    })
                    .count();
                let total_mb: f64 =
                    files.iter().filter_map(|f| f.size).sum::<u64>() as f64 / (1024.0 * 1024.0);
                let sample_files: Vec<String> =
                    files.iter().take(5).map(|f| f.rel_path.clone()).collect();
                Json(json!({
                    "status": "ok",
                    "reachable": true,
                    "mode": mode,
                    "detected_files": files.len(),
                    "pending_files": pending,
                    "deduped_pair_files": deduped_pairs,
                    "total_size_mb": (total_mb * 10.0).round() / 10.0,
                    "sample_files": sample_files,
                    "message": format!(
                        "连通成功！发现 {} 个可同步文件（新增/待同步 {} 个，自动去重 {} 个，约 {:.1} MB）",
                        files.len(),
                        pending,
                        deduped_pairs,
                        total_mb
                    )
                }))
            }
            Err(err) => Json(json!({
                "status": "error",
                "reachable": false,
                "message": err
            })),
        };
    }

    if let Some(free_mb) = free_disk_mb_for_db(&s.db) {
        if free_mb < MIN_FREE_DISK_MB {
            return Json(json!({
                "status": "error",
                "message": format!(
                    "本地磁盘剩余空间仅 {free_mb} MB（低于 {MIN_FREE_DISK_MB} MB 安全红线），已暂停远端拉取以保护硬盘"
                )
            }));
        }
    }

    let cancel_flag = {
        let mut guard = remote_sync_manager()
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if guard.snapshot.state == "running" {
            return Json(json!({
                "status": "ok",
                "already_running": true,
                "job": guard.snapshot.clone()
            }));
        }
        let new_cancel = Arc::new(AtomicBool::new(false));
        guard.cancel_flag = new_cancel.clone();
        guard.snapshot = RemoteSyncJobSnapshot {
            state: "running".to_string(),
            mode: mode.clone(),
            url: url.clone(),
            free_disk_mb: free_disk_mb_for_db(&s.db),
            ..Default::default()
        };
        new_cancel
    };

    let db = s.db.clone();
    let mode_bg = mode.clone();
    let url_bg = url.clone();
    let force = req.force;
    let lightweight = req.lightweight;
    tokio::spawn(async move {
        if let Err(err) = execute_remote_sync_worker(
            db,
            mode_bg,
            url_bg,
            auth,
            validated_ipc,
            lightweight,
            force,
            cancel_flag,
            false,
            true,
        )
        .await
        {
            update_remote_sync_snapshot(|snap| {
                snap.state = "error".to_string();
                snap.current_file.clear();
                snap.errors.push(err.clone());
                snap.summary = format!("远端同步失败: {err}");
            });
        }
    });

    Json(json!({
        "status": "ok",
        "started": true,
        "mode": mode,
        "url": url,
        "job": current_remote_sync_snapshot()
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use axum::http::{HeaderMap, StatusCode};
    use axum::response::IntoResponse;
    use axum::routing::{any, get};
    use axum::Router;
    use std::io::Write;

    #[test]
    fn webdav_multistatus_parser_handles_mixed_namespaces_and_filters_work_dirs() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
        <D:multistatus xmlns:D="DAV:">
          <D:response>
            <D:href>/dav/patents/</D:href>
            <D:propstat><D:prop><D:resourcetype><D:collection/></D:resourcetype></D:prop></D:propstat>
          </D:response>
          <D:response>
            <D:href>/dav/patents/2026Q3/</D:href>
            <D:propstat><D:prop><D:resourcetype><D:collection/></D:resourcetype></D:prop></D:propstat>
          </D:response>
          <D:response>
            <D:href>/dav/patents/BIBS-10-A_20260728.jsonl.gz</D:href>
            <D:propstat><D:prop>
              <D:resourcetype/>
              <D:getcontentlength>4096</D:getcontentlength>
              <D:getetag>"abc-123"</D:getetag>
              <D:getlastmodified>Fri, 28 Nov 2026 01:00:00 GMT</D:getlastmodified>
            </D:prop></D:propstat>
          </D:response>
        </D:multistatus>"#;

        let parsed = parse_webdav_multistatus_xml(xml);
        assert_eq!(parsed.len(), 3);
        assert!(parsed[0].is_dir);
        assert!(parsed[1].is_dir);
        assert!(!parsed[2].is_dir);
        assert_eq!(parsed[2].size, Some(4096));
        assert_eq!(parsed[2].etag, "abc-123");
    }

    #[test]
    fn manifest_parser_dedups_jsonl_gz_over_csv_and_skips_work_temp_paths() {
        let base =
            reqwest::Url::parse("https://r2.example.com/patents/innoforge-feed.json").expect("url");
        let manifest = json!({
            "version": 1,
            "files": [
                {"path": "work_v2/10-A/20260728.zip", "size": 9999},
                {"path": "STATUS_v2.json", "size": 500},
                {"path": "2026Q3/BIBS-10-A_20260728.csv", "size": 2000, "sha256": "aaa"},
                {"path": "2026Q3/BIBS-10-A_20260728.jsonl", "size": 1800, "sha256": "bbb"},
                {"path": "2026Q3/BIBS-10-A_20260728.jsonl.gz", "size": 450, "sha256": "ccc"}
            ]
        });
        let (files, deduped) = parse_remote_manifest_json(&base, &manifest).expect("parse");
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].rel_path, "2026Q3/BIBS-10-A_20260728.jsonl.gz");
        assert_eq!(
            files[0].download_url,
            "https://r2.example.com/patents/2026Q3/BIBS-10-A_20260728.jsonl.gz"
        );
        assert_eq!(deduped, 2);
    }

    #[tokio::test]
    async fn webdav_302_redirect_and_manifest_sync_import_gz_and_skip_unchanged() {
        // 准备真实的 .jsonl.gz 数据（模拟天翼云 CDN 302 直链或 R2/OSS 托管文件）
        let mut gz_buf = Vec::new();
        {
            let mut enc =
                flate2::write::GzEncoder::new(&mut gz_buf, flate2::Compression::default());
            enc.write_all(
                concat!(
                    "{\"patent_number\":\"CN990901A\",\"title\":\"高压氧舱调压控制方法\",\"ipc_codes\":\"A61G10/02\"}\n",
                    "{\"patent_number\":\"CN990902A\",\"title\":\"通用手机外壳结构\",\"ipc_codes\":\"H04M1/02\"}\n"
                )
                .as_bytes(),
            )
            .expect("write gz");
            enc.finish().expect("finish gz");
        }
        let gz_bytes = Arc::new(gz_buf);

        // 启动第二个端口模拟「第三方 CDN 主机」，断言跨主机 302 重定向绝不携带 WebDAV Authorization 头！
        let cdn_seen_auth = Arc::new(AtomicBool::new(false));
        let cdn_auth_flag = cdn_seen_auth.clone();
        let cdn_bytes = gz_bytes.clone();
        let cdn_app = Router::new().route(
            "/cdn/2026Q3/batch.jsonl.gz",
            get(move |headers: HeaderMap| {
                let flag = cdn_auth_flag.clone();
                let data = cdn_bytes.clone();
                async move {
                    if headers.contains_key(axum::http::header::AUTHORIZATION) {
                        flag.store(true, Ordering::Relaxed);
                    }
                    (StatusCode::OK, data.as_ref().clone())
                }
            }),
        );
        let cdn_listener = std::net::TcpListener::bind("127.0.0.1:0").expect("cdn bind");
        let cdn_addr = cdn_listener.local_addr().expect("cdn addr");
        let cdn_srv = tokio::spawn(async move {
            let _ = axum::Server::from_tcp(cdn_listener)
                .expect("cdn tcp")
                .serve(cdn_app.into_make_service())
                .await;
        });

        // 启动第一个端口模拟 OpenList WebDAV + Manifest 静态服务
        let cdn_redirect_url = format!("http://{cdn_addr}/cdn/2026Q3/batch.jsonl.gz");
        let dav_size = gz_bytes.len();
        let redir_for_dav = cdn_redirect_url.clone();
        let dav_app = Router::new()
            .route(
                "/dav/patents/",
                any(move |headers: HeaderMap| async move {
                    let has_auth = headers
                        .get(axum::http::header::AUTHORIZATION)
                        .and_then(|v| v.to_str().ok())
                        .is_some_and(|v| v.starts_with("Basic "));
                    if !has_auth {
                        return (StatusCode::UNAUTHORIZED, "need auth".to_string()).into_response();
                    }
                    let xml = format!(
                        r#"<?xml version="1.0" encoding="utf-8"?>
                        <d:multistatus xmlns:d="DAV:">
                          <d:response><d:href>/dav/patents/</d:href><d:propstat><d:prop><d:resourcetype><d:collection/></d:resourcetype></d:prop></d:propstat></d:response>
                          <d:response><d:href>/dav/patents/batch.jsonl.gz</d:href><d:propstat><d:prop><d:resourcetype/><d:getcontentlength>{dav_size}</d:getcontentlength><d:getetag>"v1"</d:getetag></d:prop></d:propstat></d:response>
                        </d:multistatus>"#
                    );
                    (StatusCode::MULTI_STATUS, xml).into_response()
                }),
            )
            .route(
                "/dav/patents/batch.jsonl.gz",
                get(move || {
                    let target = redir_for_dav.clone();
                    async move {
                        (
                            StatusCode::FOUND,
                            [(axum::http::header::LOCATION, target)],
                        )
                    }
                }),
            )
            .route(
                "/feed/innoforge-feed.json",
                get(move || {
                    let target = cdn_redirect_url.clone();
                    async move {
                        Json(json!({
                            "version": 1,
                            "files": [{
                                "path": "2026Q3/batch.jsonl.gz",
                                "url": target,
                                "size": dav_size,
                                "sha256": "deadbeef"
                            }]
                        }))
                    }
                }),
            );
        let dav_listener = std::net::TcpListener::bind("127.0.0.1:0").expect("dav bind");
        let dav_addr = dav_listener.local_addr().expect("dav addr");
        let dav_srv = tokio::spawn(async move {
            let _ = axum::Server::from_tcp(dav_listener)
                .expect("dav tcp")
                .serve(dav_app.into_make_service())
                .await;
        });

        let db = Arc::new(Database::init(":memory:").expect("init db"));
        let auth = RemoteAuth {
            username: "reader".to_string(),
            password: "readonly-secret".to_string(),
            token: String::new(),
        };
        let cancel = Arc::new(AtomicBool::new(false));

        // 1) WebDAV 首次同步（含跨主机 302 跳转 + A61 IPC 过滤）
        let first = execute_remote_sync_worker(
            db.clone(),
            "webdav".to_string(),
            format!("http://{dav_addr}/dav/patents"),
            auth.clone(),
            "A61".to_string(),
            true,
            false,
            cancel.clone(),
            true,
            false,
        )
        .await
        .expect("webdav sync 1");
        assert_eq!(first["imported_patents"], 1);
        assert_eq!(first["filtered_out"], 1);
        assert!(
            !cdn_seen_auth.load(Ordering::Relaxed),
            "跨主机 302 重定向严禁将 WebDAV Authorization 凭证泄露给第三方 CDN"
        );

        // 2) 再次 WebDAV 同步应直接跳过未变更文件
        let second = execute_remote_sync_worker(
            db.clone(),
            "webdav".to_string(),
            format!("http://{dav_addr}/dav/patents"),
            auth,
            "A61".to_string(),
            true,
            false,
            cancel.clone(),
            true,
            false,
        )
        .await
        .expect("webdav sync 2");
        assert_eq!(second["imported_patents"], 0);
        assert_eq!(second["skipped_unchanged_files"], 1);

        // 3) Manifest 清单模式同样可直接拉取 R2/OSS 风格直链
        let manifest_res = execute_remote_sync_worker(
            db.clone(),
            "manifest".to_string(),
            format!("http://{dav_addr}/feed"),
            RemoteAuth {
                username: String::new(),
                password: String::new(),
                token: String::new(),
            },
            String::new(),
            true,
            false,
            cancel,
            true,
            false,
        )
        .await
        .expect("manifest sync");
        assert_eq!(manifest_res["imported_patents"], 2);

        dav_srv.abort();
        cdn_srv.abort();
    }
}
