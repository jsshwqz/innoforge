# Stage 1: Build
# 官方 rust 镜像自带 rustup；复制 rust-toolchain.toml 后，首次 cargo 调用会按该文件
# 拉取钉住的 1.98.0 工具链，与 CI / 本机保持一致。
FROM rust:1.82-bookworm AS builder
WORKDIR /app
# 注意：Cargo.lock 当前被 .gitignore 忽略、未入库，所以这里不能 COPY 它
# （干净检出里没有这个文件，COPY 不存在的源会让构建直接失败）。
# cargo build 会自行解析并生成锁文件。发布级可复现构建需要把 Cargo.lock 纳入版本管理，
# 这一条留给下一轮决定。
COPY Cargo.toml build.rs rust-toolchain.toml ./
COPY src/ src/
# 源目录不带结尾斜杠：把目录本身复制进去，保留 templates/ static/ 层级
# （build.rs 要遍历 templates/ 计算 sha256；templates 走 include_str!、static 走 rust-embed，
#  都是编译期读取）。tests/ 被 .dockerignore 排除，且 --release --bin 构建不需要它。
COPY templates static ./
RUN cargo build --release --bin innoforge-server

# Stage 2: Runtime
# 模板与静态资源经 include_str! / rust-embed 编译进二进制，运行镜像只需要主程序。
# 工作目录设为 /data 并声明卷：innoforge.db 与 data/uploads/ 落在持久化卷里，
# 容器重建不丢数据。
FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && mkdir -p /data/uploads
WORKDIR /data
COPY --from=builder /app/target/release/innoforge-server /usr/local/bin/innoforge-server
COPY .env.example /data/.env.example

EXPOSE 3000
ENV RUST_LOG=info
VOLUME ["/data"]
CMD ["innoforge-server"]
