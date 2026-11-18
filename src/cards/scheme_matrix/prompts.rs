//! 卡片 ④ 多方案对决与矩阵卡提示词（`src/cards/scheme_matrix/prompts.rs`）
//!
//! 覆盖三重视角（工程性能成本 / 学术 SOTA Baseline 增益 / 专利 Claim Chart 覆盖度）与融合优化建议。

use crate::core::runtime::llm_router::wrap_user_input;

/// 多方案矩阵对决系统提示词
pub const MATRIX_SYSTEM_PROMPT: &str = "\
你是创研台 InnoForge 的多方案对决与矩阵分析专家。
你必须严格基于提供的对比方案文本、Hirschberg 语义差异摘要以及跨实施例拼凑核验结果进行分析：
1. 禁止编造未在输入中出现的实验数据、专利号、DOI 或权利要求编号；
2. 必须区分工程性能/成本、学术 Baseline 增益、专利 Claim Chart 覆盖度三重视角；
3. 对存在跨实施例拼凑预警（Cross-Embodiment Stitching）的对比文献，必须明确指出其不可组合性。";

/// 返回指定视角的中文分析焦点说明
pub fn perspective_prompt_label(mode: &str) -> &'static str {
    match mode.trim() {
        "paper_baseline" => "学术 SOTA Baseline 增益视角：聚焦方法创新增量、对照实验指标提升、消融依据与复现风险",
        "claim_chart" => "专利 Claim Chart 覆盖度视角：聚焦独立/从属权项特征逐条覆盖、区别技术特征与跨实施例拼凑风险",
        _ => "工程性能与成本视角：聚焦吞吐/时延/精度指标、BOM 成本、工程复杂度与可制造性权衡",
    }
}

/// 构造三视角综合对比与融合优化建议提示词（包裹 `<user_input>` 防注入）
pub fn build_matrix_synthesis_prompt(
    mode: &str,
    schemes_summary: &str,
    diff_summary: &str,
    stitching_summary: &str,
) -> String {
    let focus = perspective_prompt_label(mode);
    let raw_payload = format!(
        "当前主视角：{focus}\n\n【参与对决方案摘要】\n{schemes_summary}\n\n【Hirschberg 语义差异摘要】\n{diff_summary}\n\n【跨实施例拼凑预警】\n{stitching_summary}"
    );
    let wrapped = wrap_user_input(&raw_payload);
    format!(
        "{wrapped}\n\n请输出简洁、可执行的《多方案融合优化建议》（3～5 条要点），明确：1）各方案不可替代的优势；2）应规避的拼凑或性能短板；3）推荐的融合技术路线。"
    )
}
