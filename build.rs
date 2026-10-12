use std::process::Command;

fn main() {
    // Re-run if git state changes (covers branch switches, new commits, reflog updates)
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs/heads");
    println!("cargo:rerun-if-changed=.git/logs/HEAD");
    // Re-run if any template changes
    println!("cargo:rerun-if-changed=templates");

    let git_commit = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "unknown".to_string());

    let git_branch = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "unknown".to_string());

    let build_time = Command::new("date")
        .args(["-u", "+%Y-%m-%dT%H:%M:%SZ"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "unknown".to_string());

    println!("cargo:rustc-env=GIT_COMMIT={}", git_commit);
    println!("cargo:rustc-env=GIT_BRANCH={}", git_branch);
    println!("cargo:rustc-env=BUILD_TIME={}", build_time);

    // Compute sha256 of each template file via sha256sum (coreutils)
    // Variable name: TEMPLATE_SHA256_<UPPERCASE_FILENAME_WITH_UNDERSCORES>
    let templates_dir = std::path::Path::new("templates");
    if templates_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(templates_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "html") {
                    let filename = path.file_name().unwrap().to_string_lossy().to_string();
                    // Windows 上 entry.path() 形如 `templates\compare.html`。GNU sha256sum
                    // 对含反斜杠的文件名会在**整行输出开头**加一个 `\` 作为转义标记，于是
                    // split_whitespace().next() 取到的是 `\7bf0…` 这种 65 字符脏串——
                    // 结果是 Windows 构建出的二进制里 /api/version 报的模板哈希永远无法与
                    // 真实文件相等（Linux 构建因用 `/` 分隔符而不受影响）。
                    // 这里统一用正斜杠传参，并对取到的 token 做 64 位小写十六进制校验。
                    let arg = path.to_string_lossy().replace('\\', "/");
                    let sha256 = Command::new("sha256sum")
                        .arg(&arg)
                        .output()
                        .ok()
                        .and_then(|o| {
                            if o.status.success() {
                                let stdout = String::from_utf8_lossy(&o.stdout);
                                // sha256sum output: "<hash>  <filename>"
                                let token = stdout.split_whitespace().next().unwrap_or("");
                                let token = token.strip_prefix('\\').unwrap_or(token);
                                let is_hex64 = token.len() == 64
                                    && token
                                        .chars()
                                        .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c));
                                if is_hex64 {
                                    Some(token.to_string())
                                } else {
                                    println!(
                                        "cargo:warning=sha256sum 对 {} 的输出不是 64 位小写十六进制，已记为 unknown（原始 token 长度 {}）",
                                        filename,
                                        token.len()
                                    );
                                    None
                                }
                            } else {
                                println!("cargo:warning=sha256sum 执行失败，模板哈希记为 unknown：{}", filename);
                                None
                            }
                        })
                        .unwrap_or_else(|| "unknown".to_string());

                    let var_name = format!(
                        "TEMPLATE_SHA256_{}",
                        filename
                            .chars()
                            .map(|c| if c.is_ascii_alphanumeric() {
                                c.to_ascii_uppercase()
                            } else {
                                '_'
                            })
                            .collect::<String>()
                    );
                    println!("cargo:rustc-env={}={}", var_name, sha256);
                    println!("cargo:rerun-if-changed=templates/{}", filename);
                }
            }
        }
    }
}
