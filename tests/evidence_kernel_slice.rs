//! 证据内核 v2 垂直切片集成测试（合成案件夹具 + 三个负控 + 接缝 + 示例报告 golden）。
//!
//! 设计与验收标准：`docs/design/evidence-kernel-v2.md` §7。
//!
//! ## 负控验证纪律（红 → 绿，PR 正文记录真实执行输出）
//! - NC1（伪造引文）：把 `verifier.rs` 的 `CorpusIndex::resolve` 临时替换为 #79 否决的
//!   "数字包含式检查器"（section_id.contains(number)）→ `nc1_fabricated_citation_must_fail` 必须翻红
//!   （实测：`[D1:第3段]` 因 "D1:para:35" 含 "3" 被假命中 → 断言"必须 AnchorNotFound"失败）。
//! - NC2（删证据）：把 `critic::collect_findings` 临时改为静默返回空 →
//!   `nc2_deleting_evidence_must_downgrade` 必须翻红（实测："删证据后必须降级"失败）。
//! - NC3（空检索）——不变量 I3 有**两个执行点**，红通道分两半，均已实测翻红：
//!   - NC3-i：`rubric::score` 的 `InsufficientEvidence => 0.0` 硬门临时改为不归零 →
//!     单测 `evidence_kernel::rubric::tests::grounding_zero_when_insufficient` 翻红；
//!   - NC3-ii：`mod.rs` 空语料提前返回的 `rubric: RubricScore::default()` 临时改为满分 →
//!     `nc3_empty_corpus_must_not_score_high` 翻红。
//!     教训：只变异 score() 时 nc3 不红——空语料路径提前返回、不经过 score()；
//!     这正是"负控证伪不了就停下来查"抓出来的盲区。
//! - NC4（existence ≠ support，PR #125 复核 Blocking）：`verified_weight()` 修复前
//!   （AnchorOnly ⇒ Some(0.7)）`nc4_anchor_only_must_not_support_nor_raise_grounding`
//!   实测翻红——裸 `[引用N]` 句被 relation() 判 Support、grounding=0.7 越过 0.5 决策门；
//!   修复（AnchorOnly ⇒ None，grounding 分子只计 Exact/Approx）后复绿。
//!   前置形态断言（该句必须纯 AnchorOnly）保证变异可见，不测空气。
//!
//! 红通道执行方式：临时 patch 对应函数 → `cargo test ...` → 记录 FAIL → `git checkout --` 还原 → 复绿。

use innoforge::evidence_kernel as ek;
use innoforge::evidence_kernel::model::*;
use innoforge::pipeline::context::ReferenceChunk;

// ═══════════════════════════════════════════════════════════════
// 合成案件夹具：2 篇对比专利 + 1 个全文切片 + 一份混合引用质量的分析文本
// ═══════════════════════════════════════════════════════════════

fn fixture_corpus() -> SourceCorpus {
    SourceCorpus {
        docs: vec![
            SourceDoc {
                doc_id: "D1".into(),
                kind: "patent".into(),
                title: "对比文件1（温控风扇）".into(),
                sections: vec![
                    SourceSection {
                        section_id: "D1:para:35".into(),
                        kind: LocatorKind::Paragraph,
                        number: 35,
                        text: "当温度超过阈值时，控制器自动启动风扇并根据温差调节转速。".into(),
                    },
                    SourceSection {
                        section_id: "D1:claim:2".into(),
                        kind: LocatorKind::Claim,
                        number: 2,
                        text: "根据权利要求1所述的装置，其特征在于，风扇转速可自动调节。".into(),
                    },
                ],
            },
            SourceDoc {
                doc_id: "D2".into(),
                kind: "patent".into(),
                title: "对比文件2（相变储热）".into(),
                sections: vec![SourceSection {
                    section_id: "D2:para:12".into(),
                    kind: LocatorKind::Paragraph,
                    number: 12,
                    text: "相变储热模块在温度上升时吸收热量，抑制峰值温度。".into(),
                }],
            },
            SourceDoc {
                doc_id: "引用1".into(),
                kind: "patent_chunk:description".into(),
                title: "本申请说明书切片".into(),
                sections: vec![SourceSection {
                    section_id: "引用1:chunk:0".into(),
                    kind: LocatorKind::Chunk,
                    number: 0,
                    text: "散热底座与均热板之间设置有导热界面材料，用于降低接触热阻。".into(),
                }],
            },
        ],
    }
}

/// 混合引用质量的分析文本：逐字命中 / 仅锚点 / 伪造锚点 / 引文不符 / 遗留格式 / 推论 / 建议。
fn fixture_analysis() -> String {
    [
        "# 技术查新分析结论",
        "D1 公开了温差驱动的风扇控制方法，即“当温度超过阈值时，控制器自动启动风扇并根据温差调节转速”[D1:第0035段]。",
        "D2 记载了“相变储热模块在温度上升时吸收热量，抑制峰值温度”[D2:第0012段]，与之相比本方案的区别在于组合了均热板结构。",
        "本申请说明书还记载了导热界面材料降低接触热阻的方案[引用1]。",
        "D1 公开了“风扇转速可自动调节”的从属结构[D1:权利要求2]。",
        "D1 进一步公开了通过机器学习预测热失效的算法[D1:第9999段]。",
        "D2 还记载了“石墨烯涂层用于均热板表面以提升导热效率”[D2:第0012段]。",
        "对比文件已记载相同技术路线【依据：对比文件D1第35段】。",
        "从工程视角看该路线的量产成本仍具竞争力。",
        "建议后续补充温差-转速映射的实验验证数据。",
    ]
    .join("\n\n")
}

fn fixture_chunks() -> Vec<ReferenceChunk> {
    vec![ReferenceChunk {
        id: "c1".into(),
        patent_id: "MY-APP".into(),
        chunk_index: 0,
        source_type: "description".into(),
        content: "散热底座与均热板之间设置有导热界面材料，用于降低接触热阻。".into(),
        relevance_score: 0.9,
    }]
}

// ═══════════════════════════════════════════════════════════════
// 正控 AC1–AC3：端到端跑通、逐句 evidence id、示例报告可复现
// ═══════════════════════════════════════════════════════════════

#[test]
fn ac1_end_to_end_mixed_quality_analysis() {
    let (report, text) = ek::run_kernel(&fixture_analysis(), &fixture_corpus());
    // 状态：存在伪造锚点与引文不符 → 降级
    assert_eq!(report.status, KernelStatus::Degraded);
    // 逐字命中至少 2（D1:35 引文、D2:12 引文）；D1:claim2 句的引文不与锚点紧邻 → 按邻接规则记仅锚点
    assert!(
        report.metrics.verified_exact >= 2,
        "metrics={:?}",
        report.metrics
    );
    assert!(
        report.metrics.anchor_only >= 2,
        "metrics={:?}",
        report.metrics
    );
    // 伪造锚点 D1:9999 必须计为 AnchorNotFound
    assert!(
        report.metrics.anchor_not_found >= 1,
        "metrics={:?}",
        report.metrics
    );
    // 引文不符（石墨烯句子）必须计为 QuoteMismatch
    assert!(
        report.metrics.quote_mismatch >= 1,
        "metrics={:?}",
        report.metrics
    );
    // 未支撑句被降级标注且保留原文
    assert!(text.contains("【未证实】D1 进一步公开了"), "text={text}");
    // 幻觉率分母只含可解析锚点，且 > 0
    let hr = report.metrics.hallucination_rate.expect("存在可解析锚点");
    assert!(hr > 0.0 && hr < 1.0, "hr={hr}");
}

#[test]
fn ac2_report_renders_per_claim_evidence_ids() {
    let (report, _) = ek::run_kernel(&fixture_analysis(), &fixture_corpus());
    let md = ek::report::render_markdown(&report);
    assert!(md.contains("### 逐句结论与证据"), "{md}");
    assert!(
        md.contains(" — E1") && md.contains("| E1 |"),
        "全局证据编号必须同时出现在逐句清单与核验表: {md}"
    );
    assert!(md.contains("### 证据核验表"), "{md}");
    assert!(md.contains("D1:para:35"), "{md}");
    assert!(md.contains("锚点未命中"), "{md}");
    assert!(md.contains("| **overall 总分** |"), "{md}");
    assert!(md.contains("### 未支撑断言清单"), "{md}");
}

#[test]
fn ac3_example_report_golden() {
    // golden 由 print_example_report 生成并人工核读后提交；不一致即测试红。
    let (report, _) = ek::run_kernel(&fixture_analysis(), &fixture_corpus());
    let md = ek::report::render_markdown(&report);
    let golden = include_str!("fixtures/evidence_kernel/example_report.md");
    assert_eq!(
        md, golden,
        "示例报告与 golden 不一致：重新生成并核读后更新（cargo test --test evidence_kernel_slice print_example_report -- --ignored --nocapture）"
    );
}

/// 生成示例报告。默认打印供人工核读；设 `EK_WRITE_GOLDEN=1` 时直接覆写 golden 文件
/// （与 `check_html_functions.mjs --refresh` 同款基线刷新模式——变更可审计、可复现）。
#[test]
#[ignore]
fn print_example_report() {
    let (report, text) = ek::run_kernel(&fixture_analysis(), &fixture_corpus());
    let md = ek::report::render_markdown(&report);
    if std::env::var("EK_WRITE_GOLDEN").is_ok() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/evidence_kernel/example_report.md");
        std::fs::write(&path, &md).unwrap_or_else(|e| panic!("写 golden 失败: {e}"));
        println!("golden 已写入: {}", path.display());
    } else {
        println!("=====REPORT-BEGIN=====");
        println!("{md}");
        println!("=====REPORT-END=====");
    }
    println!("=====DOWNGRADED-TEXT-BEGIN=====");
    println!("{text}");
    println!("=====DOWNGRADED-TEXT-END=====");
}

// ═══════════════════════════════════════════════════════════════
// 负控 NC1–NC3（红通道执行方式见文件头注释）
// ═══════════════════════════════════════════════════════════════

/// NC1：伪造引文（编造段落号 + 编造引文）核验器必须红。
#[test]
fn nc1_fabricated_citation_must_fail() {
    let mut text = fixture_analysis();
    // 换成更极端的伪造：锚点 9999 + 完全编造的引文（若核验器退化为子串包含，这里会假绿）
    text.push_str("\n\nD1 公开了“完全编造的量子冷却超导环路描述”[D1:第9999段]。");
    let (report, text) = ek::run_kernel(&text, &fixture_corpus());
    // 伪造锚点必须 AnchorNotFound，绝不 Verified*
    let fabricated = report
        .claims
        .iter()
        .flat_map(|c| c.verifications.iter())
        .filter(|(e, _)| e.raw.contains("9999"))
        .collect::<Vec<_>>();
    assert!(!fabricated.is_empty(), "伪造句应带锚点被解析");
    for (ev, status) in &fabricated {
        assert!(
            !status.verified_weight().is_some(),
            "伪造引文不得通过核验: raw={} status={:?}",
            ev.raw,
            status
        );
    }
    // 数字包含红线：引文 "35" 不得因原文含数字串而通过
    let idx = ek::verifier::CorpusIndex::build(&fixture_corpus());
    let ev = EvidenceRef {
        raw: "[D1:第0035段]".into(),
        anchor: ek::anchor::parse_anchor("[D1:第0035段]"),
        quote: "0035".into(),
    };
    let s = ek::verifier::verify_ref(&ev, &idx);
    assert!(
        !matches!(s, VerifyStatus::VerifiedExact { .. }),
        "纯数字子串不得逐字通过: {s:?}"
    );
    // #79 的核心假绿形态：段落 3 不存在，包含式检查会因 "D1:para:35" 含 "3" 而放行——
    // 精确三元组匹配必须拒绝
    let ev3 = EvidenceRef {
        raw: "[D1:第3段]".into(),
        anchor: ek::anchor::parse_anchor("[D1:第3段]"),
        quote: "任意内容".into(),
    };
    let s3 = ek::verifier::verify_ref(&ev3, &idx);
    assert!(
        matches!(s3, VerifyStatus::AnchorNotFound),
        "不存在的段落号必须 AnchorNotFound（包含式检查在此假绿）: {s3:?}"
    );
    // 降级与清单
    assert!(
        text.contains("【未证实】D1 进一步公开了") || text.contains("【未证实】D1 公开了“完全编造")
    );
    assert!(report
        .critique
        .iter()
        .any(|f| f.text.contains("机器学习预测热失效")));
}

/// NC2 专用夹具：全部句子逐字核验（Exact）——「证据齐全」在 existence ≠ support
/// 语义下必须真的齐全（混合质量的 fixture_analysis 含裸引用句，grounding 只有
/// 2/6≈0.33，不适合再当「齐全」基线；那是 NC4 的管辖面）。
fn nc2_full_evidence_analysis() -> String {
    [
        "D1 公开了“当温度超过阈值时，控制器自动启动风扇”[D1:第0035段]。",
        "D1 记载了“根据温差调节转速”[D1:第0035段]。",
        "D2 公开了“相变储热模块在温度上升时吸收热量”[D2:第0012段]。",
    ]
    .join("\n\n")
}

/// NC2：删除证据后，原已核验的结论必须降级且决策门关闭。
#[test]
fn nc2_deleting_evidence_must_downgrade() {
    let full = ek::run_kernel(&nc2_full_evidence_analysis(), &fixture_corpus());
    assert!(
        !ek::should_suppress_decision(&full.0),
        "证据齐全时（grounding={:.2}）不应抑制决策",
        full.0.rubric.grounding
    );

    // 删掉 D1 全部证据（模拟上游检索丢结果/竞态）
    let mut shrunk = fixture_corpus();
    shrunk.docs.retain(|d| d.doc_id != "D1");
    let (report, text) = ek::run_kernel(&nc2_full_evidence_analysis(), &shrunk);

    // 原 Verified 的 D1 句子现在必须 Unsupported → 降级
    assert!(
        text.contains("【未证实】D1 公开了") && text.contains("【未证实】D1 记载了"),
        "删证据后必须降级: {text}"
    );
    // grounding 显著下降且决策门关闭（3/3 → 1/3）
    assert!(
        report.rubric.grounding < full.0.rubric.grounding,
        "grounding 应下降: {:.2} -> {:.2}",
        full.0.rubric.grounding,
        report.rubric.grounding
    );
    assert!(
        ek::should_suppress_decision(&report),
        "grounding={:.2} 必须触发决策抑制",
        report.rubric.grounding
    );
}

/// NC3：检索为空（空语料）不得产出事实断言、不得高分。
#[test]
fn nc3_empty_corpus_must_not_score_high() {
    let empty = SourceCorpus::default();
    let (report, text) = ek::run_kernel(&fixture_analysis(), &empty);
    assert_eq!(report.status, KernelStatus::InsufficientEvidence);
    assert_eq!(report.rubric.grounding, 0.0);
    assert_eq!(report.rubric.overall, 0.0, "空证据硬门：overall 必须 0");
    assert!(
        !ek::has_factual_claims(&report),
        "空语料不得产出事实断言记录"
    );
    // 文本不被改写（不装模作样降级，而是整体判证据不足）
    assert_eq!(text, fixture_analysis());
    let md = ek::report::render_markdown(&report);
    assert!(md.contains("InsufficientEvidence"), "{md}");
    assert!(md.contains("不构成新颖性"), "{md}");
    // 决策门必须关
    assert!(ek::should_suppress_decision(&report));
}

/// NC4（existence ≠ support，PR #125 复核 Blocking）：只含裸 `[引用N]`（锚点命中、
/// 无引文）的事实句，不得被判为 Support，也不得抬高 grounding——否则一份
/// "事实句只写 [引用N]、不写引文"的报告 grounding 可达 0.7，越过 0.5 决策门，
/// 即"零逐字支撑仍放行决策"。修复前此断言红（relation=Support、grounding=0.7、
/// 决策门不抑制）；修复 = `verified_weight()` 收紧为逐字核验（AnchorOnly ⇒ None），
/// grounding 分子只计 Exact/Approx，AnchorOnly 保留为独立诊断计量（m.anchor_only）。
#[test]
fn nc4_anchor_only_must_not_support_nor_raise_grounding() {
    // 生产形态语料：corpus_from_chunks（引用N:chunk:0，number 恒 0）——
    // 复核意见指出的放大路径，不用手摆 fixture。
    let corpus = ek::corpus_from_chunks(&fixture_chunks());
    // 单条事实句、只带裸引用号、无引文——锚点真实存在（existence 成立）
    let text = "本申请说明书还记载了导热界面材料降低接触热阻的方案[引用1]。".to_string();
    let (report, text) = ek::run_kernel(&text, &corpus);

    let fact = report
        .claims
        .iter()
        .find(|c| c.kind == ClaimKind::FactFromSource)
        .expect("含锚点句应被识别为 FactFromSource");
    // 该句的证据必须是 AnchorOnly（锚点命中、无引文）——确认测的是目标形态
    assert!(
        !fact.verifications.is_empty()
            && fact
                .verifications
                .iter()
                .all(|(_, s)| matches!(s, VerifyStatus::AnchorOnly { .. })),
        "前置：该句应为纯 AnchorOnly，实际 {:?}",
        fact.verifications
    );
    // ① relation 不得是 Support（仅锚点 = existence，不是 support）
    assert_eq!(
        fact.relation(),
        Relation::Insufficient,
        "裸 [引用N]（无引文）不得单独支撑事实断言: verifications={:?}",
        fact.verifications
    );
    // ② 不得抬高 grounding（分子只计 Exact/Approx）
    assert_eq!(
        report.metrics.grounding,
        Some(0.0),
        "仅 AnchorOnly 的报告 grounding 分子必须为 0: metrics={:?}",
        report.metrics
    );
    // ③ 放大效应兜底：零逐字支撑必须关闭决策门（0.0 < 0.5）
    assert!(
        ek::should_suppress_decision(&report),
        "grounding={:?} 必须抑制决策建议",
        report.rubric.grounding
    );
    // ④ AnchorOnly 仍是可见诊断（不消失，只是不再冒充支撑）
    assert!(
        report.metrics.anchor_only >= 1,
        "AnchorOnly 计量保留: metrics={:?}",
        report.metrics
    );
    // ⑤ 修复或降级：v1 生产路径（no_repair）下该句必须带【未证实】标注
    assert!(
        text.contains("【未证实】"),
        "零逐字支撑的事实句必须降级标注，不得悬空: {text}"
    );
}

// ═══════════════════════════════════════════════════════════════
// 接缝测试：finalize 真实接线（kernel_report 落 ctx + 决策门 + 渲染追加）
// ═══════════════════════════════════════════════════════════════

fn run_finalize_with(
    analysis: &str,
    chunks: Vec<ReferenceChunk>,
) -> (innoforge::pipeline::context::PipelineContext, String) {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let db_path = dir.path().join("ek.db");
    let db = std::sync::Arc::new(
        innoforge::db::Database::init(db_path.to_str().expect("utf8 path")).expect("db init"),
    );
    let mut ctx = innoforge::pipeline::context::PipelineContext::new("idea-ek", "温控创意", "描述");
    ctx.ai_analysis = analysis.to_string();
    ctx.rag_chunks = chunks;
    ctx.novelty_status = innoforge::pipeline::context::NoveltyStatus::Sufficient;
    ctx.novelty_score = 65.0;
    // 决策需要 evidence_chain（provenance 的理由模板从证据取材）
    for i in 0..4 {
        ctx.evidence_chain.push(innoforge::types::idea::Evidence {
            id: format!("ev-{i}"),
            idea_id: "idea-ek".into(),
            claim: format!("证据断言 {i}"),
            source_type: "patent".into(),
            source_id: "CNTEST0000000A".into(),
            source_title: "对比专利".into(),
            source_url: String::new(),
            claim_number: None,
            excerpt: "摘要片段".into(),
            relation: "supports".into(),
            confidence: 0.8,
            produced_by: "RankAndFilter".into(),
            created_at: String::new(),
        });
    }
    // Database 生命周期：TempDir drop 时删除；测试进程内先持有再释放
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    rt.block_on(async {
        innoforge::pipeline::steps::finalize::execute(&mut ctx, &db)
            .await
            .expect("finalize execute");
    });
    std::thread::sleep(std::time::Duration::from_millis(50));
    (ctx, dir.path().to_str().expect("utf8").to_string())
}

#[test]
fn seam_finalize_writes_kernel_report_and_gates_decision() {
    // 场景 A：证据齐全、引用真实（引文逐字来自切片内容）→ 决策放行
    let good_analysis =
        "本申请记载了“散热底座与均热板之间设置有导热界面材料，用于降低接触热阻”[引用1]。";
    let (ctx, _p) = run_finalize_with(good_analysis, fixture_chunks());
    let kr = ctx
        .kernel_report
        .as_ref()
        .expect("kernel_report 必须落 ctx");
    let report: KernelReport = serde_json::from_str(kr).expect("kernel_report JSON 可解析");
    assert_ne!(report.status, KernelStatus::InsufficientEvidence);
    assert!(
        ctx.ai_analysis.contains("## 证据核验报告"),
        "渲染段必须追加"
    );
    assert!(
        ctx.decision.is_some(),
        "grounding={:.2} 应放行决策",
        report.rubric.grounding
    );
    assert!(ctx.ai_analysis.contains("## 决策建议"));

    // 场景 B：同样文本但零证据（无切片）→ 空语料硬门 + 决策抑制说明
    let (ctx2, _p2) = run_finalize_with(good_analysis, Vec::new());
    assert!(ctx2.kernel_report.is_some());
    let r2: KernelReport =
        serde_json::from_str(ctx2.kernel_report.as_ref().expect("kr")).expect("parse");
    assert_eq!(r2.status, KernelStatus::InsufficientEvidence);
    assert!(ctx2.decision.is_none(), "空证据必须抑制决策");
    assert!(
        ctx2.ai_analysis.contains("证据核验未达门槛"),
        "{}",
        ctx2.ai_analysis
    );
}

// ═══════════════════════════════════════════════════════════════
// corpus_from_chunks：编号与 deep_reasoning 的 [引用N] 严格一致
// ═══════════════════════════════════════════════════════════════

#[test]
fn corpus_numbering_matches_deep_reasoning_prompt() {
    let chunks: Vec<ReferenceChunk> = (1..=3)
        .map(|i| ReferenceChunk {
            id: format!("c{i}"),
            patent_id: format!("P{i}"),
            chunk_index: 0,
            source_type: "description".into(),
            content: format!("切片内容 {i}：导热界面材料降低接触热阻。"),
            relevance_score: 0.5,
        })
        .collect();
    let corpus = ek::corpus_from_chunks(&chunks);
    assert_eq!(corpus.docs.len(), 3);
    assert_eq!(corpus.docs[0].doc_id, "引用1"); // 1 基，与 deep_reasoning.rs:171 一致
    assert_eq!(corpus.docs[2].doc_id, "引用3");
    // AI 写 [引用3] → 可信还原到第 3 个切片
    let idx = ek::verifier::CorpusIndex::build(&corpus);
    let ev = EvidenceRef {
        raw: "[引用3]".into(),
        anchor: ek::anchor::parse_anchor("[引用3]"),
        quote: "切片内容 3：导热界面材料降低接触热阻".into(),
    };
    let s = ek::verifier::verify_ref(&ev, &idx);
    assert!(matches!(s, VerifyStatus::VerifiedExact { .. }), "got {s:?}");
}
