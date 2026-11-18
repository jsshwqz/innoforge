//! 创研台 InnoForge v1.0 下层五大确定性共享超级内核（`src/core/`）
//!
//! 严格对应 `AGENTS.md` §二.2 与 `docs/plans/2026-11-18-v1-overhaul-plan.md` §2.1：
//! - [`vault`]：统一四维文献、双栖结构化切片、部件标号词典、特征原子、对照实验账本与 RRF 混合检索
//! - [`multimodal`]：三级自适应文档/图像解析、附图部件标号联动、多轮图文共创会话与基线卡收敛
//! - [`omni_search`]：全球专利 + 四大免 Key 学术论文源 + 开源工程 + 行业标准四维检索与真实 DOI 核验
//! - [`evidence_rebuttal`]：O(1) 原文/DOI/实验锚点回查、跨实施例拼凑识破、第 33 条/论文一致性、公开时序守卫与五维硬阻断门禁
//! - [`runtime`]：9 服务商自动容灾与看图路由、可分叉可暂停 `dag_runner` 与一源四出 `forge_export` 编译器

pub mod evidence_rebuttal;
pub mod multimodal;
pub mod omni_search;
pub mod runtime;
pub mod vault;
