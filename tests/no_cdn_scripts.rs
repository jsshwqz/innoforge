//! CDN 外链守卫：断言 templates/** 与 static/** 不得出现 http(s) 外链 `<script src>`。
//!
//! 白名单为空——所有脚本必须从本地路径加载（/static/... 或相对路径）。
//! 命中任何 `http://` 或 `https://` 的 `<script src="...">` 即 fail。
//!
//! 防回归背景：Chart.js 曾经经 jsDelivr CDN 引入，离线环境不渲染统计图表。
//! 现已 vendoring 进 static/vendor/，本守卫确保不再回退到 CDN。

use std::fs;
use std::path::{Path, PathBuf};

/// 正则匹配 `<script ... src="http(s)://...">` —— 外链脚本标签。
fn extract_cdn_script_srcs(html: &str) -> Vec<String> {
    let mut hits = Vec::new();

    // 逐行扫描，避免跨行误匹配；每行找 <script ... src="..."> 中的 URL
    for line in html.lines() {
        let trimmed = line.trim();
        // 跳过注释行
        if trimmed.starts_with("<!--") && trimmed.ends_with("-->") {
            continue;
        }
        // 查找所有 <script ...> 标签中的 src 属性
        let mut rest = line;
        while let Some(script_start) = rest.find("<script") {
            // 找到对应的 >
            let after_tag = &rest[script_start..];
            if let Some(tag_end) = after_tag.find('>') {
                let tag_content = &after_tag[..tag_end];
                // 在标签内容中找 src="..."
                if let Some(src_start) = tag_content.find("src=\"") {
                    let src_value_start = src_start + 5; // len("src=\"")
                    let after_src = &tag_content[src_value_start..];
                    if let Some(src_end) = after_src.find('"') {
                        let src_url = &after_src[..src_end];
                        if src_url.starts_with("http://") || src_url.starts_with("https://") {
                            hits.push(src_url.to_string());
                        }
                    }
                }
                rest = &after_tag[tag_end + 1..];
            } else {
                break;
            }
        }
    }

    hits
}

/// 递归收集目录下所有文件路径。
fn collect_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                files.extend(collect_files(&path));
            } else {
                files.push(path);
            }
        }
    }
    files
}

#[test]
fn no_cdn_script_tags_in_templates_or_static() {
    let whitelist: &[&str] = &[]; // 白名单为空

    let mut violations: Vec<(PathBuf, String)> = Vec::new();

    for dir_name in &["templates", "static"] {
        let dir = Path::new(dir_name);
        if !dir.exists() {
            continue;
        }
        for file_path in collect_files(dir) {
            // 只扫 HTML 文件
            let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if ext != "html" && ext != "htm" {
                continue;
            }
            let content = match fs::read_to_string(&file_path) {
                Ok(c) => c,
                Err(_) => continue,
            };
            for src_url in extract_cdn_script_srcs(&content) {
                if whitelist.contains(&src_url.as_str()) {
                    continue;
                }
                violations.push((file_path.clone(), src_url));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "发现 CDN 外链 <script src>（白名单为空，所有脚本必须本地加载）：\n{}",
        violations
            .iter()
            .map(|(p, u)| format!("  {} -> {}", p.display(), u))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

// ── Negative controls: prove the guard actually fires on violations ──

/// Negative control: synthetic HTML with a CDN `<script src="https://...">`
/// must be detected by `extract_cdn_script_srcs`.
#[test]
fn negative_control_cdn_https_is_detected() {
    let synthetic = r#"<script src="https://cdn.jsdelivr.net/npm/chart.js"></script>"#;
    let hits = extract_cdn_script_srcs(synthetic);
    assert_eq!(
        hits.len(),
        1,
        "negative control: CDN https script must be detected, got {:?}",
        hits
    );
    assert!(
        hits[0].starts_with("https://"),
        "negative control: detected URL should be https://, got {}",
        hits[0]
    );
}

/// Negative control: synthetic HTML with a CDN `<script src="http://...">`
/// must also be detected.
#[test]
fn negative_control_cdn_http_is_detected() {
    let synthetic = r#"<script src="http://unpkg.com/vue@3"></script>"#;
    let hits = extract_cdn_script_srcs(synthetic);
    assert_eq!(
        hits.len(),
        1,
        "negative control: CDN http script must be detected, got {:?}",
        hits
    );
}

/// Negative control: local script tags (`/static/...`) must NOT be flagged.
#[test]
fn negative_control_local_script_passes() {
    let synthetic = r#"<script src="/static/vendor/chart.umd.min.js"></script>"#;
    let hits = extract_cdn_script_srcs(synthetic);
    assert!(
        hits.is_empty(),
        "negative control: local script must not be flagged, got {:?}",
        hits
    );
}

/// Negative control: HTML comments containing CDN URLs must NOT be flagged
/// (the guard skips comment lines).
#[test]
fn negative_control_commented_cdn_passes() {
    let synthetic = r#"<!-- <script src="https://cdn.example.com/lib.js"></script> -->"#;
    let hits = extract_cdn_script_srcs(synthetic);
    assert!(
        hits.is_empty(),
        "negative control: commented-out CDN script must not be flagged, got {:?}",
        hits
    );
}
