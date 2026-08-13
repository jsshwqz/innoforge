//! 沙箱执行器 / Sandbox runner
//!
//! 在隔离的子进程中执行验证脚本，捕获输出，强制超时。
//! 临时文件统一放在 data/runtime-temp 下（不污染系统临时目录）。
//! 超时通过 tokio::time::timeout 实现，超时后子进程自动终止。

use crate::common::new_temp_file;
use crate::experiment::types::ExperimentSpec;
use crate::pipeline::context::ExperimentResult;
use anyhow::Result;
use std::io::Write;
use std::process::Command;
use std::time::{Duration, Instant};
use tokio::process::Command as AsyncCommand;

/// 最大超时上限（防止 timeout_secs 被设为过大）
const MAX_TIMEOUT_SECS: u64 = 300;

/// 在沙箱中运行实验脚本
pub async fn run_experiment(spec: &ExperimentSpec) -> Result<ExperimentResult> {
    let start = Instant::now();

    let script_id = uuid::Uuid::new_v4().to_string();

    let (ext, interpreter) = match spec.language.as_str() {
        "python" => ("py", find_python()),
        "rust" => ("rs", "rustc".to_string()),
        _ => ("py", find_python()),
    };

    // 使用项目专属临时目录 + UUID 文件名
    let script_path = new_temp_file("exp", ext)?;

    {
        let mut file = std::fs::File::create(&script_path)?;
        file.write_all(spec.script_content.as_bytes())?;
    }

    // 实际超时时间：clamp 到合理范围
    let timeout_secs = spec.timeout_secs
        .max(10)
        .min(MAX_TIMEOUT_SECS)
        .max(60);
    let timeout = Duration::from_secs(timeout_secs);

    // 使用 tokio::time::timeout 实现真正的超时控制。
    // 超时后子进程会被 tokio 自动终止（kill_on_drop=true）。
    let output = tokio::time::timeout(timeout, async {
        if ext == "py" {
            let mut cmd = AsyncCommand::new(&interpreter);
            cmd.arg(&script_path);
            cmd.env("PYTHONDONTWRITEBYTECODE", "1");
            cmd.kill_on_drop(true);
            cmd.output().await
        } else {
            let bin_path = script_path.with_extension("exe");
            let mut compile_cmd = AsyncCommand::new("rustc");
            compile_cmd
                .arg(&script_path)
                .arg("-o")
                .arg(&bin_path)
                .kill_on_drop(true);
            let compile = compile_cmd.output().await;
            match compile {
                Ok(c) if c.status.success() => {
                    let mut run_cmd = AsyncCommand::new(&bin_path);
                    run_cmd.kill_on_drop(true);
                    run_cmd.output().await
                }
                Ok(c) => Ok(c),
                Err(e) => Err(e),
            }
        }
    }).await;

    // 无论成功/超时/失败，清理临时文件
    let _ = std::fs::remove_file(&script_path);
    let _ = std::fs::remove_file(script_path.with_extension("exe"));

    let duration_ms = start.elapsed().as_millis() as u64;

    match output {
        Err(_) => {
            Ok(ExperimentResult {
                script_path: format!("exp_{}.{}", script_id, ext),
                language: spec.language.clone(),
                exit_code: -1,
                stdout: String::new(),
                stderr: format!(
                    "实验脚本执行超时（超过 {} 秒）。请缩短脚本或增加超时设置。",
                    timeout_secs
                ),
                metrics: serde_json::Value::Null,
                duration_ms,
                success: false,
            })
        }
        Ok(Err(e)) => Ok(ExperimentResult {
            script_path: format!("exp_{}.{}", script_id, ext),
            language: spec.language.clone(),
            exit_code: -1,
            stdout: String::new(),
            stderr: format!("Failed to execute: {}", e),
            metrics: serde_json::Value::Null,
            duration_ms,
            success: false,
        }),
        Ok(Ok(output)) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let exit_code = output.status.code().unwrap_or(-1);
            let success = output.status.success();
            let metrics = extract_json_metrics(&stdout);

            Ok(ExperimentResult {
                script_path: format!("exp_{}.{}", script_id, ext),
                language: spec.language.clone(),
                exit_code,
                stdout: truncate(&stdout, 5000),
                stderr: truncate(&stderr, 2000),
                metrics,
                duration_ms,
                success,
            })
        }
    }
}

/// 从输出中提取 JSON 行格式的指标
fn extract_json_metrics(stdout: &str) -> serde_json::Value {
    let mut metrics = serde_json::Map::new();
    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('{') && trimmed.ends_with('}') {
            if let Ok(obj) = serde_json::from_str::<serde_json::Value>(trimmed) {
                if let Some(map) = obj.as_object() {
                    for (k, v) in map {
                        metrics.insert(k.clone(), v.clone());
                    }
                }
            }
        }
    }
    if metrics.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::Value::Object(metrics)
    }
}

/// 查找 Python 解释器
fn find_python() -> String {
    for name in &["python3", "python"] {
        if Command::new(name).arg("--version").output().is_ok() {
            return name.to_string();
        }
    }
    "python".to_string()
}

#[cfg(test)]
fn python_available() -> bool {
    for name in &["python3", "python"] {
        if Command::new(name)
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            return true;
        }
    }
    false
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...(truncated)", &s[..max])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_simple_python_experiment() {
        if !python_available() {
            eprintln!("Skipping test: Python not available");
            return;
        }

        let spec = ExperimentSpec {
            title: "test".to_string(),
            language: "python".to_string(),
            script_content: r#"import json
print(json.dumps({"accuracy": 0.95}))
print("EXPERIMENT_DONE")"#.to_string(),
            hypothesis: "test hypothesis".to_string(),
            timeout_secs: 10,
        };
        let result = run_experiment(&spec).await.unwrap();
        assert!(result.success, "脚本应成功执行: {}", result.stderr);
        assert!(
            result.stdout.contains("EXPERIMENT_DONE"),
            "stdout 应包含完成标记: {}",
            result.stdout
        );
    }

    #[tokio::test]
    #[ignore]
    async fn test_timeout_terminates_script() {
        let spec = ExperimentSpec {
            title: "timeout_test".to_string(),
            language: "python".to_string(),
            script_content: r#"import time
time.sleep(300)"#.to_string(),
            hypothesis: "timeout test".to_string(),
            timeout_secs: 5,
        };
        let result = run_experiment(&spec).await.unwrap();
        assert!(!result.success, "超时脚本应失败");
        assert!(result.stderr.contains("超时"), "应报告超时错误: {}", result.stderr);
    }

    #[test]
    fn test_extract_json_metrics() {
        let stdout = r#"Starting...
{"accuracy": 0.95}
{"latency_ms": 12.3}
Done
"#;
        let metrics = extract_json_metrics(stdout);
        assert_eq!(metrics["accuracy"], 0.95);
        assert_eq!(metrics["latency_ms"], 12.3);
    }

    #[test]
    fn test_extract_no_metrics() {
        let stdout = r#"Hello world
No JSON here
"#;
        let metrics = extract_json_metrics(stdout);
        assert!(metrics.is_null());
    }
}