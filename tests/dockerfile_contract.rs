//! Static structural contract guard for `Dockerfile` (issue 67 P3.7).
//!
//! Ensures the multi-stage Docker build preserves directory nesting required by
//! `build.rs` (`templates/`, `static/`) and enforces non-loopback host bind
//! configuration.

const DOCKERFILE: &str = include_str!("../Dockerfile");

#[test]
fn dockerfile_preserves_nested_asset_directories_and_release_build() {
    assert!(
        DOCKERFILE.contains("COPY Cargo.toml build.rs rust-toolchain.toml ./"),
        "Dockerfile 必须复制 Cargo.toml、build.rs 与 rust-toolchain.toml"
    );
    assert!(
        DOCKERFILE.contains("COPY src/ src/"),
        "Dockerfile 必须复制 src/ 目录结构"
    );
    assert!(
        DOCKERFILE.contains("COPY templates/ templates/"),
        "Dockerfile 必须保留 templates/ 子目录层级（禁止扁平 COPY templates static ./）"
    );
    assert!(
        DOCKERFILE.contains("COPY static/ static/"),
        "Dockerfile 必须保留 static/ 子目录层级（禁止扁平 COPY templates static ./）"
    );
    assert!(
        !DOCKERFILE.contains("COPY templates static ./"),
        "Dockerfile 不得退回会打平目录结构的 COPY templates static ./"
    );
    assert!(
        DOCKERFILE.contains("cargo build --release --bin innoforge-server"),
        "Dockerfile 必须构建 innoforge-server release 二进制"
    );
    assert!(
        DOCKERFILE.contains("ENV INNOFORGE_BIND_HOST=0.0.0.0"),
        "Dockerfile 必须显式配置容器监听 INNOFORGE_BIND_HOST=0.0.0.0"
    );
}
