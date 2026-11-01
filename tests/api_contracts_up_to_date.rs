//! 守卫：`docs/API-CONTRACTS.md` 必须与代码一致。
//!
//! 契约文档是**生成**的（`scripts/gen_api_contracts.js`），不是手写的：
//! 手写字段说明会漂移，而「文档说有、代码说没有」正是 #62/#67 的病根。
//! 这里用同一脚本的 `--check` 模式重算一遍并比对，代码改了没重跑生成器就红。
//!
//! 与 `route_inventory_up_to_date.rs` 的分工：那边守「有哪些端点」，
//! 这边守「每个端点收什么、回什么」。
//!
//! node 缺失时**失败**而不是跳过：跳过等于把守卫悄悄关掉（见 #59 的教训）。

use std::process::Command;

fn node_version() -> Result<String, String> {
    let out = Command::new("node")
        .arg("--version")
        .output()
        .map_err(|e| {
            format!("无法启动 node（`{e}`）。契约守卫需要 node ≥ 18；CI 的 test job 已安装。")
        })?;
    if !out.status.success() {
        return Err(format!(
            "`node --version` 失败：{}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[test]
fn api_contracts_match_generated_output() {
    let version = node_version().expect("契约守卫需要 node");

    let output = Command::new("node")
        .arg("scripts/gen_api_contracts.js")
        .arg("--check")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("spawn node scripts/gen_api_contracts.js --check");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "docs/API-CONTRACTS.md 与代码不一致（node {version}）。\n\
         重跑生成器：node scripts/gen_api_contracts.js\n\
         --- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );

    // 防止「脚本根本没跑起来却返回 0」：成功时必须打印 ok 行与路由数。
    assert!(
        stdout.contains("ok:") && stdout.contains("api routes"),
        "`--check` 返回成功但没有校验任何东西（stdout: {stdout:?}）——守卫失效，修脚本而不是放宽测试"
    );
}
