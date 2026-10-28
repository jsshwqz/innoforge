//! #73 静态防线：禁止新增「`&s[..n]` 且 n 未做 `is_char_boundary` 回退」的写法。
//!
//! 非 ASCII 输入在多字节字符中间按字节切片会触发
//! `byte index n is not a char boundary` panic。`mask_api_key`（#73 主因）
//! 与 `sandbox::truncate` 已修，本测试守新增违例。
//!
//! 实现方式：**基线快照**。记录当前 `src/` 下所有含 `[..]` 切片的文件及其计数，
//! 若新增文件出现该模式、或已知文件计数上升，测试红。下降（删除切片）不红。
//! 这样既不误报已审查的安全切片（`find()` 返回的位置、`char_indices()` 的
//! `byte_pos`、`is_char_boundary` 回退后的 `&s[..end]` 等），又能卡住新增。
//!
//! 审查指南见 `AGENTS.md` §2.7。

/// 基线快照（#73 落地时冻结）。格式：`("相对路径", 行数)`。
/// 每条都已人工审查确认安全：索引来自 `find()`/`char_indices()`/`is_char_boundary`
/// 回退/已知 ASCII 数据。新增切片需在 PR 中审查后追加到此表。
const BASELINE: &[(&str, usize)] = &[
    ("src/ai/chat.rs", 6),
    ("src/ai/client.rs", 3),
    ("src/ai/evidence.rs", 1),
    ("src/ai/extraction.rs", 1),
    ("src/ai/fact_check.rs", 3),
    ("src/ai/golden_regression.rs", 2),
    ("src/ai/prohibition_check.rs", 1),
    ("src/ai/response_pipeline.rs", 3),
    ("src/ai/search_report.rs", 1),
    ("src/ai/whitespace_search.rs", 1),
    ("src/ai/zero_evidence.rs", 1),
    ("src/bin/mcp-server.rs", 1),
    ("src/context.rs", 1),
    ("src/routes/ai.rs", 2),
    ("src/routes/idea.rs", 1),
    ("src/routes/ipc.rs", 2),
    ("src/routes/upload.rs", 3),
    ("src/search/enrichment.rs", 1),
    ("src/search/providers/serpapi.rs", 4),
    ("src/search/query.rs", 5),
];

/// 统计一个文件中「疑似裸字节切片」的行数。
///
/// 启发式：行含 `[..` 且不含 `is_char_boundary` / `as_bytes()` / 注释前缀。
/// 这会捕获所有 `&s[..n]` / `s[..n]` 写法（含安全的），靠基线区分新旧。
fn count_byte_slices(content: &str) -> usize {
    content
        .lines()
        .filter(|line| {
            let t = line.trim();
            !t.starts_with("//")
                && !t.starts_with("///")
                && line.contains("[..")
                && !line.contains("is_char_boundary")
                && !line.contains("as_bytes()")
        })
        .count()
}

#[test]
fn no_new_unsafe_byte_slice_in_src() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let src_dir = std::path::Path::new(manifest_dir).join("src");

    // 收集当前所有 .rs 文件的切片计数
    let mut current: std::collections::BTreeMap<String, usize> = Default::default();
    walk_rs(&src_dir, &mut |path, content| {
        let rel = path
            .strip_prefix(manifest_dir)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        let count = count_byte_slices(content);
        if count > 0 {
            current.insert(rel, count);
        }
    });

    let baseline_map: std::collections::BTreeMap<&str, &usize> =
        BASELINE.iter().map(|&(f, ref c)| (f, c)).collect();

    let mut problems = Vec::new();

    // 新文件出现切片
    for (file, count) in &current {
        if !baseline_map.contains_key(file.as_str()) {
            problems.push(format!(
                "  新增 {file} 含 {count} 处 `[..]` 切片——请改用 `safe_truncate`/`chars().take()`，\n  \
                 或审查确认安全后加入 BASELINE"
            ));
        }
    }

    // 已知文件计数上升
    for (file, baseline_count) in BASELINE {
        let actual = current.get(*file).copied().unwrap_or(0);
        if actual > *baseline_count {
            problems.push(format!(
                "  {file} 切片数从 {baseline_count} 升到 {actual}——新增的 `[..]` 需审查，\n  \
                 确认安全后更新 BASELINE，否则改用 `safe_truncate`/`chars().take()`"
            ));
        }
    }

    if !problems.is_empty() {
        panic!(
            "#73 防线：检测到新增 `&str` 裸字节切片（`[..n]`）。\n\
             非 ASCII 输入会 panic（byte index not a char boundary）。\n\
             详见 AGENTS.md §2.7。\n\n{}\n",
            problems.join("\n")
        );
    }
}

fn walk_rs(dir: &std::path::Path, cb: &mut dyn FnMut(&std::path::Path, &str)) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk_rs(&path, cb);
            } else if path.extension().map(|e| e == "rs").unwrap_or(false) {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    cb(&path, &content);
                }
            }
        }
    }
}
