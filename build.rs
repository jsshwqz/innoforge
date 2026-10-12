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
                if path.extension().map_or(false, |e| e == "html") {
                    let filename = path.file_name().unwrap().to_string_lossy().to_string();
                    let sha256 = Command::new("sha256sum")
                        .arg(&path)
                        .output()
                        .ok()
                        .and_then(|o| {
                            if o.status.success() {
                                let stdout = String::from_utf8_lossy(&o.stdout);
                                // sha256sum output: "<hash>  <filename>"
                                Some(stdout.split_whitespace().next().unwrap_or("unknown").to_string())
                            } else {
                                None
                            }
                        })
                        .unwrap_or_else(|| "unknown".to_string());

                    let var_name = format!(
                        "TEMPLATE_SHA256_{}",
                        filename.chars()
                            .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_uppercase() } else { '_' })
                            .collect::<String>()
                    );
                    println!("cargo:rustc-env={}={}", var_name, sha256);
                    println!("cargo:rerun-if-changed=templates/{}", filename);
                }
            }
        }
    }
}
