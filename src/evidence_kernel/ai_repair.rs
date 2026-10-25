//! AI 修复轮（ADR-6 的 repair 半环）：核验 → 未支撑句清单 → AI 改写 → 再核验。
//!
//! 边界纪律（设计文档 §4 ADR-1/ADR-6）：
//! - AI 只做「改写」，不做「判据」——修复产物必须过同一核验器，改不好照样降级；
//! - 修复指令明确三选一：补真实锚点+引文 / 改写为明确的推论措辞 / 保持原文由内核降级；
//! - 禁止 AI 在修复中编造锚点——prompt 给出可用锚点清单（来自语料，编号真实）；
//! - 有界轮次（默认 1），每轮产物全文替换，轮后重走解析+核验。
//!
//! 生产接线：finalize 默认仍走 NoRepair（零 AI 成本）；AI 修复作为显式 opt-in
//! （成本/延迟由用户拍板，PR 正文记录）。测试用注入的 fake 修复函数。

use crate::evidence_kernel::model::{Finding, SourceCorpus};
use std::pin::Pin;

/// 异步修复函数：输入 (当前文本, 未支撑清单, 语料)，返回改写后的全文。
pub type AsyncRepairFn<'a> =
    Box<dyn Fn(&str, &[Finding], &SourceCorpus) -> RepairFuture + Send + Sync + 'a>;
pub type RepairFuture = Pin<Box<dyn std::future::Future<Output = String> + Send>>;

/// 可用锚点清单（给 AI 的「弹药库」——只允许从这里选，杜绝编造编号）。
pub fn available_anchors(corpus: &SourceCorpus, max: usize) -> Vec<String> {
    corpus
        .docs
        .iter()
        .flat_map(|d| {
            d.sections.iter().map(move |s| {
                format!(
                    "[{}:{}]（{}…）",
                    d.doc_id,
                    match s.kind {
                        crate::evidence_kernel::model::LocatorKind::Claim =>
                            format!("权利要求{}", s.number),
                        crate::evidence_kernel::model::LocatorKind::Paragraph =>
                            format!("第{:04}段", s.number),
                        crate::evidence_kernel::model::LocatorKind::Chunk =>
                            return format!("[{}]", d.doc_id),
                    },
                    s.text.chars().take(24).collect::<String>()
                )
            })
        })
        .take(max)
        .collect()
}

/// 构建修复 prompt（system + user）。
pub fn build_repair_prompt(
    text: &str,
    findings: &[Finding],
    corpus: &SourceCorpus,
) -> (String, String) {
    let system = "你是专利分析报告的证据修复器。你的唯一任务：让每一条「未支撑的事实断言」变成可核验的表述。\
严格三选一，逐句处理：\n\
1. 语料确有支撑 → 改写该句，补上真实锚点（如 [D1:第0035段]）并在锚点前用中文引号给出原文引文（≤40字）；\
只能使用「可用锚点清单」里列出的锚点，禁止编造任何编号。\n\
2. 语料没有支撑但该句有分析价值 → 改写为明确的推论措辞（例如「据推演，…」「一种可能的方向是…」），不加锚点。\n\
3. 不确定 → 保持该句原样，不要添加任何锚点（内核会标注【未证实】）。\n\
其余句子一字不改。只输出改写后的完整报告全文，不要解释。";
    let findings_list = findings
        .iter()
        .map(|f| format!("- （句#{}）{}", f.sentence_index, f.text))
        .collect::<Vec<_>>()
        .join("\n");
    let anchors = available_anchors(corpus, 40).join("\n");
    let user = format!(
        "## 可用锚点清单（只准用这里的，禁止编造）\n{anchors}\n\n\
         ## 未支撑的事实断言（需要逐句处理）\n{findings_list}\n\n\
         ## 报告全文（其余句子保持原样）\n{text}"
    );
    (system.to_string(), user)
}

/// 生产用 AI 修复函数构造器（注入 AiClient）。
/// 每轮调用 = 一次 chat（温度 0.2，修复要稳定不要发散）。
pub fn make_ai_repair<'a>(
    client: crate::ai::AiClient,
    corpus: &'a SourceCorpus,
) -> AsyncRepairFn<'a> {
    Box::new(move |text: &str, findings: &[Finding], _c: &SourceCorpus| {
        let (system, user) = build_repair_prompt(text, findings, corpus);
        let c = client.clone();
        // 借用必须在 async 块前结束（RepairFuture 是 'static）：回退原文用 owned 副本
        let orig = text.to_string();
        Box::pin(async move {
            match c.chat_with_system(&system, &user, 0.2).await {
                Ok(repaired) => {
                    // 防御：AI 返回空/过短文本时视为修复失败，回退原文（由内核降级兜底）
                    if repaired.trim().len() < orig.len() / 4 {
                        tracing::warn!("EK-v2 AI 修复返回过短，回退原文");
                        orig
                    } else {
                        repaired
                    }
                }
                Err(e) => {
                    tracing::warn!("EK-v2 AI 修复调用失败，回退原文: {}", e);
                    orig
                }
            }
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finding(text: &str, idx: usize) -> Finding {
        Finding {
            claim_id: format!("C{idx}"),
            sentence_index: idx,
            text: text.into(),
            reason: "测试".into(),
        }
    }

    #[test]
    fn available_anchors_only_from_corpus() {
        let corpus = crate::evidence_kernel::tests_support::fixture_corpus();
        let anchors = available_anchors(&corpus, 10);
        assert!(
            anchors.iter().any(|a| a.contains("[D1:第0035段]")),
            "{anchors:?}"
        );
        assert!(anchors.iter().any(|a| a.contains("引用1")), "{anchors:?}");
        assert!(!anchors.iter().any(|a| a.contains("9999")));
    }

    #[test]
    fn repair_prompt_lists_findings_and_anchor_whitelist() {
        let corpus = crate::evidence_kernel::tests_support::fixture_corpus();
        let (system, user) = build_repair_prompt(
            "报告正文。",
            &[finding("D1 公开了编造的算法。", 3)],
            &corpus,
        );
        assert!(system.contains("禁止编造"), "{system}");
        assert!(user.contains("D1:第0035段"), "锚点白名单必须给出: {user}");
        assert!(user.contains("句#3"), "未支撑句必须带句号定位: {user}");
    }
}

#[cfg(test)]
mod async_behavior_tests {
    use super::*;
    use crate::evidence_kernel::model::*;

    fn corpus() -> SourceCorpus {
        crate::evidence_kernel::tests_support::fixture_corpus()
    }

    fn fake_good_repair<'a>() -> AsyncRepairFn<'a> {
        // 模拟 AI 依规修复（三选一中的选项 2）：整句改写为推论措辞并去掉编造锚点
        Box::new(|text: &str, _findings: &[Finding], _c: &SourceCorpus| {
            let repaired = text.replace(
                "D1 进一步公开了通过机器学习预测热失效的算法[D1:第9999段]。",
                "据推演，D1 可能还涉及通过机器学习预测热失效的算法。",
            );
            Box::pin(async move { repaired })
        })
    }

    fn fake_bad_repair<'a>() -> AsyncRepairFn<'a> {
        // 模拟 AI 修复失败：原样返回（内核必须降级兜底，不许因"修过"就放行）
        Box::new(|text: &str, _f: &[Finding], _c: &SourceCorpus| {
            let t = text.to_string();
            Box::pin(async move { t })
        })
    }

    #[tokio::test]
    async fn good_repair_recovers_status() {
        let text = "D1 公开了温差驱动的风扇控制方法，即“当温度超过阈值时，控制器自动启动风扇并根据温差调节转速”[D1:第0035段]。\n\nD1 进一步公开了通过机器学习预测热失效的算法[D1:第9999段]。";
        let c = corpus();
        let (report, text) =
            crate::evidence_kernel::run_kernel_with_repair_async(text, &c, fake_good_repair(), 1)
                .await;
        assert_eq!(report.status, KernelStatus::Ok, "修复为推论措辞后应回 Ok");
        assert_eq!(report.repair_rounds, 1);
        assert!(text.contains("据推演"), "修复文本应被采用: {text}");
        assert!(report.metrics.anchor_not_found == 0);
    }

    #[tokio::test]
    async fn bad_repair_falls_back_to_downgrade() {
        let text = "D1 进一步公开了通过机器学习预测热失效的算法[D1:第9999段]。";
        let c = corpus();
        // 轮次语义：首轮为强制处理轮，max_repair_rounds 为追加修复次数 → 共 1+2=3 次调用
        let (report, text) =
            crate::evidence_kernel::run_kernel_with_repair_async(text, &c, fake_bad_repair(), 2)
                .await;
        assert_eq!(report.status, KernelStatus::Degraded, "修复无效必须降级");
        assert_eq!(report.repair_rounds, 3, "应跑满 1+2 轮");
        assert!(text.contains("【未证实】"), "降级标注必须落文本: {text}");
        assert!(report.metrics.anchor_not_found >= 1);
    }

    #[tokio::test]
    async fn empty_corpus_hard_gate_holds_for_async_too() {
        let (report, _) = crate::evidence_kernel::run_kernel_with_repair_async(
            "任意文本。",
            &SourceCorpus::default(),
            fake_good_repair(),
            3,
        )
        .await;
        assert_eq!(report.status, KernelStatus::InsufficientEvidence);
        assert_eq!(report.rubric.overall, 0.0);
    }
}
