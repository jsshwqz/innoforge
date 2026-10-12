//! 案件数据守卫：遍历 git ls-files 得到的每个文件，命中以下检查即 panic：
//!
//! 四类检查，每类同时扫文件**内容**和文件**路径**：
//! 1. 申请人姓名——汉字与拼音两种形态（拼音用字符数组逐字符拼装）
//! 2. 对比文件公开号类目——本轮替掉的四个真实 D 号（每段 ≤4 字符拼接）
//! 3. 发文序号类目——16 位数字（每段 ≤4 字符拼接）
//! 4. 申请号形状 `\d{13}(\.\d)?`、公开号形状 `CN\d{8}[A-Z]` / `CN\d{9}U`
//!    ——仅在 src/ 生代码区域（#[cfg(test)] 之前）检查
//!
//! tests/fixtures/ 下的文件内容也执行四类检查（fixture 按定义是合成内容，更应被扫描）。
//! 白名单常量必须保持为空。
//! 守卫自身不含任何 >=8 位连续数字串，也不含完整禁词字面量。
//! 违规消息不回显命中串，只报文件名和类别。

use regex::Regex;
use std::process::Command;

// ── 类目 1：申请人姓名（汉字 + 拼音）──

/// 申请人姓名汉字形态：用字符数组逐字符拼接构造。
fn forbidden_name_cn() -> String {
    let chars: [char; 3] = ['\u{738b}', '\u{9752}', '\u{829d}'];
    chars.iter().collect()
}

/// 申请人姓名拼音形态：用字符数组逐字符拼接构造。
/// 守卫文件自身不含此拼音字面量。
fn forbidden_name_pinyin() -> String {
    let chars: [char; 11] = [
        'w', 'a', 'n', 'g', 'q', 'i', 'n', 'g', 'z', 'h', 'i',
    ];
    chars.iter().collect()
}

// ── 类目 2：对比文件公开号（每段 ≤4 字符，运行时拼接）──

fn forbidden_pub_numbers() -> Vec<String> {
    // D1 → ["CN","1031","3314","4A"]
    let d1 = format!("{}{}{}{}", "CN", "1031", "3314", "4A");
    // D2 → ["CN","1131","0768","4A"]
    let d2 = format!("{}{}{}{}", "CN", "1131", "0768", "4A");
    // D3 → ["CN","2109","5132","0U"]
    let d3 = format!("{}{}{}{}", "CN", "2109", "5132", "0U");
    // D4 → ["CN","2183","2814","1U"]
    let d4 = format!("{}{}{}{}", "CN", "2183", "2814", "1U");
    // 旧案公开号A → ["CN","1150","7668","8A"]
    let p1 = format!("{}{}{}{}", "CN", "1150", "7668", "8A");
    // 旧案公开号B → ["CN","1149","6275","7U"]
    let p2 = format!("{}{}{}{}", "CN", "1149", "6275", "7U");
    vec![d1, d2, d3, d4, p1, p2]
}

// ── 类目 3：发文序号（16 位，每段 ≤4 字符拼接）──

fn forbidden_filing_serials() -> Vec<String> {
    // 驳回决定发文序号 → ["2026","0710","0014","1920"]
    let s1 = format!("{}{}{}{}", "2026", "0710", "0014", "1920");
    // 一通发文序号 → ["2026","0301","0002","8380"]
    let s2 = format!("{}{}{}{}", "2026", "0301", "0002", "8380");
    vec![s1, s2]
}

// ── 类目 4：被禁申请号（每段 ≤4 字符拼接）──

fn forbidden_app_numbers() -> Vec<String> {
    // 案号A → ["2022","1083","5143","9"] 四段拼接
    let a1 = format!("{}{}{}{}", "2022", "1083", "5143", "9");
    let a1d = format!("{}{}.{}", "2022", "1083", "5143.9");
    // 案号B → ["2022","1047","7324","9"] 四段拼接
    let a2 = format!("{}{}{}{}", "2022", "1047", "7324", "9");
    let a2d = format!("{}{}.{}", "2022", "1047", "7324.9");
    vec![a1, a1d, a2, a2d]
}

/// 白名单：必须保持为空。不许为了让测试通过而往白名单加条目。
const WHITELIST: &[&str] = &[];

/// 对于 Rust 文件，截取 #[cfg(test)] 之前的部分作为生代码区域。
/// 如果文件没有 #[cfg(test)]，返回全文。
fn production_part(content: &str, is_rust: bool) -> &str {
    if !is_rust {
        return content;
    }
    if let Some(pos) = content.find("#[cfg(test)]") {
        &content[..pos]
    } else {
        content
    }
}

/// 对单个文本（内容或路径）执行全部四类检查，返回违规类别列表。
/// 违规消息不回显命中串本身。
///
/// 大小写敏感性：
/// - 申请人姓名汉字：中文无大小写概念，直接匹配。
/// - 拼音／公开号／序号／申请号：大小写不敏感（每个文件只 to_lowercase 一次）。
///   防止小写公开号 / 混合大小写拼音等形态漏网。
fn check_text(text: &str, is_src_rust_prod: bool) -> Vec<&'static str> {
    let mut hits: Vec<&'static str> = Vec::new();

    // 类目 1：申请人姓名（汉字 — 中文无大小写，直接匹配）
    if text.contains(&forbidden_name_cn()) {
        hits.push("申请人姓名(汉字)");
    }

    // 大小写不敏感类目：对 text 做一次 to_lowercase，needle 也 lowercase
    let text_lower = text.to_lowercase();

    // 类目 1 续：申请人姓名拼音（大小写不敏感，防混合大小写拼音漏网）
    if text_lower.contains(&forbidden_name_pinyin().to_lowercase()) {
        hits.push("申请人姓名(拼音)");
    }

    // 类目 2：对比文件公开号（大小写不敏感，防小写公开号漏网）
    for n in forbidden_pub_numbers() {
        if text_lower.contains(&n.to_lowercase()) {
            hits.push("对比文件公开号");
            break;
        }
    }

    // 类目 3：发文序号（纯数字，大小写不敏感无影响，走同一路径保持一致）
    for n in forbidden_filing_serials() {
        if text_lower.contains(&n) {
            hits.push("发文序号");
            break;
        }
    }

    // 类目 4：被禁申请号（纯数字，同上）
    for n in forbidden_app_numbers() {
        if text_lower.contains(&n) {
            hits.push("被禁申请号");
            break;
        }
    }

    // 类目 4 续：形状检查（仅 src/ 生代码区域，大小写不敏感）
    if is_src_rust_prod {
        let re_app_num = Regex::new(r"\d{13}(\.\d)?").expect("invalid app num regex");
        let re_pub_num_8 = Regex::new(r"(?i)CN\d{8}[A-Z]").expect("invalid pub num 8 regex");
        let re_pub_num_9u = Regex::new(r"(?i)CN\d{9}U").expect("invalid pub num 9u regex");

        if re_app_num.is_match(text) {
            hits.push("生代码申请号形状");
        }
        if re_pub_num_8.is_match(text) {
            hits.push("生代码公开号形状(CN8+letter)");
        }
        if re_pub_num_9u.is_match(text) {
            hits.push("生代码公开号形状(CN9+U)");
        }
    }

    hits
}

/// 收集待扫文件：优先 `git ls-files`（跟踪树权威清单）。
/// 若仓库不是 git 工作树（例如从 Release/源码包解压、或 git 不可用），
/// 回退为递归遍历清单目录，跳过构建产物与本地数据目录。
/// 回退是**扫描范围的扩大而非放弃**——守卫绝不因为拿不到 git 就静默通过。
fn collect_files() -> (Vec<String>, &'static str) {
    if let Ok(output) = Command::new("git").arg("ls-files").output() {
        if output.status.success() {
            let files = std::str::from_utf8(&output.stdout)
                .expect("git ls-files output not UTF-8")
                .lines()
                .filter(|l| !l.is_empty())
                .map(|l| l.to_string())
                .collect();
            return (files, "git ls-files");
        }
    }

    // 回退：递归文件树（不带 .git 的源码包检出）
    const SKIP_DIRS: &[&str] = &[".git", "target", "node_modules", "data", ".cargo"];
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut files: Vec<String> = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if path.is_dir() {
                if !SKIP_DIRS.contains(&name.as_str()) {
                    stack.push(path);
                }
            } else if let Ok(rel) = path.strip_prefix(&root) {
                files.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    files.sort();
    (files, "filesystem walk (no git metadata)")
}

#[test]
fn no_case_data_in_git_tracked_files() {
    let (files, mode) = collect_files();
    assert!(
        !files.is_empty(),
        "守卫未取到任何待扫文件（mode={}）——空扫描不允许算通过",
        mode
    );

    let mut scanned: usize = 0;
    let mut violations: Vec<String> = Vec::new();

    for file in &files {
        let file = file.as_str();
        // 跳过白名单
        if WHITELIST.contains(&file) {
            continue;
        }

        // ── 路径扫描：对所有文件路径执行四类检查 ──
        // （tests/fixtures/ 路径也扫，因为泄露可能藏在路径里的姓名/案号）
        let path_hits = check_text(file, false);
        for h in path_hits {
            violations.push(format!("{}: 路径命中 [{}]", file, h));
        }

        // ── 内容扫描 ──
        // tests/fixtures/ 内容也扫描：fixture 按定义是合成内容，
        // 若往 fixture 里塞真实案卷，守卫必须报警。不留静默口子。

        let content = match std::fs::read_to_string(file) {
            Ok(c) => c,
            Err(_) => continue,
        };

        scanned += 1;

        let is_src_rust = file.starts_with("src/") && file.ends_with(".rs");
        let prod = production_part(&content, is_src_rust);
        let content_hits = check_text(prod, is_src_rust);
        for h in content_hits {
            violations.push(format!("{}: 内容命中 [{}]", file, h));
        }
    }

    eprintln!(
        "案件数据守卫：扫描方式={}，{} 个文件内容 + {} 个文件路径",
        mode, scanned, files.len()
    );

    if !violations.is_empty() {
        panic!(
            "案件数据守卫发现 {} 处违规：\n{}",
            violations.len(),
            violations.join("\n")
        );
    }
}

// ── 回归测试：大小写不敏感匹配 ──

#[test]
fn case_insensitive_pub_number_lowercase_is_caught() {
    // 合成字符串：小写 cn + D1 号码形态（不使用真实 D1 号码字面量）
    let d1_lower = format!("{}{}{}{}", "cn", "1031", "3314", "4a");
    let sample = format!("文件名 d1_{}.pdf", d1_lower);
    let hits = check_text(&sample, false);
    assert!(
        hits.contains(&"对比文件公开号"),
        "lowercase pub number must be caught, got hits: {:?}",
        hits
    );
}

#[test]
fn case_insensitive_pinyin_mixed_case_is_caught() {
    // 混合大小写拼音（首字母大写形态）
    let pinyin_mixed: String = {
        let chars: [char; 11] = [
            'W', 'a', 'n', 'g', 'Q', 'i', 'n', 'g', 'Z', 'h', 'i',
        ];
        chars.iter().collect()
    };
    let sample = format!("作者：{}", pinyin_mixed);
    let hits = check_text(&sample, false);
    assert!(
        hits.contains(&"申请人姓名(拼音)"),
        "mixed-case pinyin must be caught, got hits: {:?}",
        hits
    );
}

#[test]
fn case_insensitive_pub_number_uppercase_still_caught() {
    // 大写形态仍应被命中（回归：不能因为加了不敏感而漏掉原来的大写）
    let d1_upper = format!("{}{}{}{}", "CN", "1031", "3314", "4A");
    let sample = format!("对比文件 {}", d1_upper);
    let hits = check_text(&sample, false);
    assert!(
        hits.contains(&"对比文件公开号"),
        "uppercase pub number must still be caught, got hits: {:?}",
        hits
    );
}
