//! 修改稿特征逐字核验门（UA3 v2）/ Amendment verbatim gate.
//!
//! 背景（M3 实测）：答复书把原始文本里**零记载**的措辞写进「修改后的权利要求」，
//! 并断言"来源于原说明书具体实施方式的明确记载"。旧 UA3 逐 3–8 字中文词回原文，
//! 词的边界错误会把合法撰写变换误判、又会漏掉真实编造；旧 `check_a33_risk` 只认 17 个
//! 硬编码术语，且答复书链路根本不调用任何 A33 校验。
//!
//! 本模块的判据（评审定稿）：
//! 1. 把修改后权利要求拆成**特征句**，归一化后对「原始申请文本（说明书 + 权利要求书）」
//!    做逐字 / 滑窗匹配；
//! 2. 归一化豁免五类**合法撰写变换**：①指代词增删；②权项连接语与语序；
//!    ③附图标记与中英文括号；④标点；⑤数值范围符号与单位全半角；
//! 3. 匹配内核复用 [`crate::evidence_kernel::verifier`]（检索范围限制在本申请的
//!    权利要求 + 具体实施方式段落三元组内），支撑判定复用
//!    [`crate::evidence_kernel::critic`] 所消费的严格 `Relation::Support`
//!    （仅锚点命中 AnchorOnly 不算支撑）——本模块**不写第二套包含匹配**；
//! 4. 阻断形态 = 降级标注而非硬阻断：零命中特征句保留原文、就地标
//!    [`UNVERIFIED_MARK`]；"明确记载/来源于原说明书"类断言替换为降级标注；
//!    调用方据 `unsupported_count` 加阻断横幅并把提交度维度压死。
//!
//! 漏放面（如实声明，见 `docs/design/evidence-kernel-v2.md` FMEA）：同义替换、
//! 上位概括、跨实施例拼凑这道门拦不住；由段落三元组限制 + `critic` 的严格
//! Support 判定补第二层。

use crate::evidence_kernel::critic;
use crate::evidence_kernel::model::{
    Anchor, ClaimKind, ClaimRecord, EvidenceRef, LocatorKind, Relation, SourceCorpus, SourceDoc,
    SourceSection, VerifyStatus,
};
use crate::evidence_kernel::verifier::{CorpusIndex, ScopedHit};

/// 本申请在证据宇宙中的 doc_id（与对比文件 D1/D2 区分）。
pub const OWN_DOC_ID: &str = "本申请";

/// 零命中特征句的就地降级标注（保留原句，绝不静默删除）。
pub const UNVERIFIED_MARK: &str = "【未证实：该措辞在原始文本零命中】";

/// "明确记载 / 来源于原说明书"类断言的降级标注（整句替换，防止原断言残留）。
pub const ASSERTION_DOWNGRADE_BLOCK: &str = "【已降级：本句原为对修改依据的记载断言，但本稿修改后的权利要求存在原始文本零命中的措辞，不得据此断言有原文依据，请人工核对专利法第33条】";

/// 特征句最小长度（归一化后中文字符计）：短于此的片段（如「还包括」残留）不单独判定。
const MIN_FEATURE_CHARS: usize = 3;

/// 单个修改稿段落的最大扫描字符数（防单段吞掉整篇答复书）。
const MAX_BLOCK_CHARS: usize = 1500;

/// 修改后权利要求的段落标记（模型/模板的几种写法）。
const AMENDMENT_MARKERS: &[&str] = &[
    "修改后的权利要求",
    "修改后权利要求",
    "修改后的权利要求书",
    "关于权利要求的修改",
    "权利要求修改方案",
    "建议修改的权利要求",
];

/// 门的状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateStatus {
    /// 文本里没有修改后权利要求 → 不判定
    NoAmendment,
    /// 无原始申请文本可比对 → 跳过（不得据此声称"已核验"）
    NoOriginal,
    /// 全部特征句均在原始文本命中
    Verified,
    /// 存在零命中特征句（已就地降级标注）
    Degraded,
}

impl GateStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            GateStatus::NoAmendment => "no_amendment",
            GateStatus::NoOriginal => "skipped_no_original",
            GateStatus::Verified => "verified",
            GateStatus::Degraded => "degraded",
        }
    }
}

/// 一条零命中特征句。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct UnsupportedFeature {
    pub claim_no: String,
    /// 特征句原文（保留写进答复书的原措辞）
    pub text: String,
    /// critic 给出的判定理由（复用内核语义，不另造话术）
    pub reason: String,
    /// 最近似候选的 3-gram 分数（无任何候选时为 0）
    pub best_score: f64,
}

/// 门的结果。
#[derive(Debug, Clone, PartialEq)]
pub struct AmendmentGateResult {
    pub status: GateStatus,
    /// 就地标注后的文本（原文一字不删；`gated_text` 即最终下发/落库文本）
    pub gated_text: String,
    pub unsupported: Vec<UnsupportedFeature>,
    pub unsupported_count: usize,
    /// 被替换为降级标注的"明确记载"类断言条数
    pub assertions_downgraded: usize,
    pub amended_claim_count: usize,
    pub checked_feature_count: usize,
    /// 提交度维度：`unsupported_count == 0` 才可能为真（压死规则见调用方）
    pub submission_ready: bool,
}

impl AmendmentGateResult {
    fn empty(status: GateStatus, text: &str) -> Self {
        Self {
            status,
            gated_text: text.to_string(),
            unsupported: Vec::new(),
            unsupported_count: 0,
            assertions_downgraded: 0,
            amended_claim_count: 0,
            checked_feature_count: 0,
            submission_ready: status != GateStatus::Degraded,
        }
    }

    /// 阻断横幅（复用 M2-C 横幅形态：`> ⚠️` blockquote，随正文首部一并下发）。
    pub fn banner(&self) -> String {
        if self.unsupported_count == 0 {
            return String::new();
        }
        format!(
            "> ⚠️ 修改稿核验阻断（amendment_unsupported={}）：本稿修改后的权利要求中有 {} 处特征句在原始申请文本（说明书 + 权利要求书）零命中，已就地标注{}；相关「明确记载 / 来源于原说明书」类断言已降级。提交前必须逐处核对专利法第33条（修改超范围）。\n\n",
            self.unsupported_count, self.unsupported_count, UNVERIFIED_MARK
        )
    }
}

/// 门主入口：核验并返回就地标注后的文本。
///
/// 纯函数、零 AI、零网络（与内核 ADR-1 一致）。
pub fn gate_amendment_text(letter: &str, original_application: &str) -> AmendmentGateResult {
    if letter.trim().is_empty() {
        return AmendmentGateResult::empty(GateStatus::NoAmendment, letter);
    }
    let blocks = amendment_blocks(letter);
    if blocks.is_empty() {
        return AmendmentGateResult::empty(GateStatus::NoAmendment, letter);
    }
    if original_application.trim().is_empty() {
        return AmendmentGateResult::empty(GateStatus::NoOriginal, letter);
    }

    let corpus = build_own_application_corpus(original_application);
    let index = CorpusIndex::build(&corpus);

    let mut evals: Vec<FeatureEval> = Vec::new();
    let mut claim_count = 0usize;
    for (bi, (start, end)) in blocks.iter().enumerate() {
        let block_text = letter.get(*start..*end).unwrap_or("");
        for claim in extract_claims(block_text) {
            claim_count += 1;
            for raw in split_features(&claim.text) {
                let needle = strip_legal_variation(&raw);
                if needle.chars().count() < MIN_FEATURE_CHARS {
                    continue;
                }
                let hit = index.verify_quote_in_doc(
                    &needle,
                    OWN_DOC_ID,
                    &[LocatorKind::Claim, LocatorKind::Paragraph],
                );
                let (supported, reason, best_score) = judge(&needle, &hit);
                evals.push(FeatureEval {
                    block: bi,
                    claim_no: claim.claim_no.clone(),
                    raw,
                    supported,
                    reason,
                    best_score,
                });
            }
        }
    }

    if evals.is_empty() {
        return AmendmentGateResult {
            status: GateStatus::Verified,
            gated_text: letter.to_string(),
            unsupported: Vec::new(),
            unsupported_count: 0,
            assertions_downgraded: 0,
            amended_claim_count: claim_count,
            checked_feature_count: 0,
            submission_ready: true,
        };
    }

    // 零命中 → 就地插入降级标注（保留原句；按块内顺序定位，重复句逐处标注）
    let mut insertions: Vec<usize> = Vec::new();
    for (bi, (start, end)) in blocks.iter().enumerate() {
        let block_len = end - start;
        let block_text = letter.get(*start..).unwrap_or("");
        let mut cursor = 0usize;
        for ev in evals.iter().filter(|e| e.block == bi && !e.supported) {
            let Some(rel) = block_text
                .get(cursor..)
                .and_then(|rest| rest.find(ev.raw.as_str()))
            else {
                continue;
            };
            let abs = cursor + rel;
            if abs >= block_len {
                break;
            }
            insertions.push(start + abs);
            cursor = abs + ev.raw.len();
        }
    }
    insertions.sort_unstable();
    insertions.dedup();

    let mut gated = String::with_capacity(letter.len() + insertions.len() * UNVERIFIED_MARK.len());
    let mut prev = 0usize;
    for pos in &insertions {
        gated.push_str(letter.get(prev..*pos).unwrap_or(""));
        gated.push_str(UNVERIFIED_MARK);
        prev = *pos;
    }
    gated.push_str(letter.get(prev..).unwrap_or(""));

    let unsupported: Vec<UnsupportedFeature> = evals
        .iter()
        .filter(|e| !e.supported)
        .map(|e| UnsupportedFeature {
            claim_no: e.claim_no.clone(),
            text: e.raw.clone(),
            reason: e.reason.clone(),
            best_score: e.best_score,
        })
        .collect();

    // 零命中时，"明确记载 / 来源于原说明书"类断言一律替换为降级标注
    let (gated, assertions_downgraded) = if unsupported.is_empty() {
        (gated, 0usize)
    } else {
        downgrade_spec_assertions(&gated)
    };

    let status = if unsupported.is_empty() {
        GateStatus::Verified
    } else {
        GateStatus::Degraded
    };
    AmendmentGateResult {
        status,
        gated_text: gated,
        unsupported_count: unsupported.len(),
        unsupported,
        assertions_downgraded,
        amended_claim_count: claim_count,
        checked_feature_count: evals.len(),
        submission_ready: gate_submission_ready(status),
    }
}

fn gate_submission_ready(status: GateStatus) -> bool {
    !matches!(status, GateStatus::Degraded)
}

struct FeatureEval {
    block: usize,
    claim_no: String,
    raw: String,
    supported: bool,
    reason: String,
    best_score: f64,
}

/// 支撑判定 = critic 消费的严格 `Relation::Support`：
/// 必须有逐字核验（Exact/Approx）；AnchorOnly（仅锚点无引文）不算支撑。
fn judge(needle: &str, hit: &Option<ScopedHit>) -> (bool, String, f64) {
    let (anchor, status, score) = match hit {
        Some(h) => {
            let score = match &h.status {
                VerifyStatus::VerifiedExact { .. } => 1.0,
                VerifyStatus::VerifiedApprox { score, .. } => *score,
                VerifyStatus::QuoteMismatch { best_score } => *best_score,
                _ => 0.0,
            };
            (Some(h.anchor.clone()), h.status.clone(), score)
        }
        None => (None, VerifyStatus::QuoteMismatch { best_score: 0.0 }, 0.0),
    };
    let record = ClaimRecord {
        claim_id: "F1".to_string(),
        sentence_index: 0,
        text: needle.to_string(),
        kind: ClaimKind::FactFromSource,
        verifications: vec![(
            EvidenceRef {
                raw: anchor
                    .as_ref()
                    .map(|a: &Anchor| format!("[{}:{}]", a.doc_id, a.number))
                    .unwrap_or_default(),
                anchor,
                quote: needle.to_string(),
            },
            status,
        )],
        downgraded: false,
    };
    let supported = record.relation() == Relation::Support;
    let reason = if supported {
        "原始文本中找到逐字/滑窗命中".to_string()
    } else {
        critic::collect_findings(std::slice::from_ref(&record))
            .first()
            .map(|f| f.reason.clone())
            .unwrap_or_else(|| "未通过证据核验".to_string())
    };
    (supported, reason, score)
}

// ═══════════════════════════════════════════════════════════════════════
//  修改后权利要求抽取
// ═══════════════════════════════════════════════════════════════════════

/// 从文本中定位全部「修改后权利要求」段落，返回 (start, end) 字节区间（已排序合并且均落字符边界）。
pub fn amendment_blocks(text: &str) -> Vec<(usize, usize)> {
    let mut raw: Vec<(usize, usize)> = Vec::new();
    for marker in AMENDMENT_MARKERS {
        let mut from = 0usize;
        while let Some(pos) = text.get(from..).and_then(|s| s.find(marker)) {
            let abs = from + pos;
            raw.push((abs, block_end(text, abs)));
            from = abs + marker.len().max(1);
            if raw.len() >= 8 {
                break;
            }
        }
    }
    // 「修改为：「…」」句式：模型常把修改文本直接放在引号里
    if let Ok(re) = regex::Regex::new(
        r#"(?:修改为|改为|修改文本为|修改后内容为)\s*[：:]?\s*[「“"]([^」”"]{2,300})[」”"]"#,
    ) {
        for cap in re.captures_iter(text) {
            if let Some(q) = cap.get(1) {
                raw.push((q.start(), q.end()));
            }
        }
    }
    raw.sort_unstable();
    // 合并重叠区间
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (s, e) in raw {
        if let Some(last) = merged.last_mut() {
            if s <= last.1 {
                last.1 = last.1.max(e);
                continue;
            }
        }
        merged.push((s, e));
    }
    merged
}

/// 段落终点：遇到下一节标题（`#`/`一、`/`**三`/`修改要点说明`）或超长上限即止。
fn block_end(text: &str, start: usize) -> usize {
    let tail = text.get(start..).unwrap_or("");
    let mut consumed = 0usize;
    let mut chars = 0usize;
    let mut first = true;
    for line in tail.split_inclusive('\n') {
        if !first && is_section_boundary(line) {
            break;
        }
        consumed += line.len();
        chars += line.chars().count();
        if chars > MAX_BLOCK_CHARS {
            break;
        }
        first = false;
    }
    let end = start + consumed;
    text.get(start..end).map(|_| end).unwrap_or(text.len())
}

fn is_section_boundary(line: &str) -> bool {
    let t = line.trim_start();
    if t.is_empty() {
        return false;
    }
    t.starts_with('#') || t.starts_with("修改要点说明") || regex_section_heading().is_match(t)
}

fn regex_section_heading() -> &'static regex::Regex {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        regex::Regex::new(r"^\*{0,2}[一二三四五六七八九十]{1,3}[、.．]")
            .unwrap_or_else(|e| panic!("section heading 正则常量失败即 bug: {e}"))
    })
}

struct AmendedClaim {
    claim_no: String,
    text: String,
}

fn extract_claims(block: &str) -> Vec<AmendedClaim> {
    // 只认行首的「[修改后的]权利要求N」——正文里的"根据权利要求1所述的……"
    // 是引用不是权项边界，误当边界会把一句话腰斩成两个假特征句。
    let Ok(re) =
        regex::Regex::new(r"(?m)^[ \t*#\-]*(?:修改后的)?权利要求\s*(\d{1,3})\s*[：:、.．]?")
    else {
        return vec![AmendedClaim {
            claim_no: "0".to_string(),
            text: block.to_string(),
        }];
    };
    let hits: Vec<(usize, usize, String)> = re
        .captures_iter(block)
        .filter_map(|c| {
            let m = c.get(0)?;
            let num = c.get(1)?.as_str().to_string();
            Some((m.start(), m.end(), num))
        })
        .collect();
    if hits.is_empty() {
        return vec![AmendedClaim {
            claim_no: "0".to_string(),
            text: block.to_string(),
        }];
    }
    let mut out = Vec::new();
    for (i, (_, body_start, num)) in hits.iter().enumerate() {
        let body_end = hits.get(i + 1).map(|h| h.0).unwrap_or(block.len());
        let body = block.get(*body_start..body_end).unwrap_or("");
        let body = body.trim().trim_start_matches(['*', '-', '•', ' ']);
        out.push(AmendedClaim {
            claim_no: num.clone(),
            text: body.to_string(),
        });
    }
    out
}

/// 把一项权利要求拆成特征句：句读 + 权项并列连接语（及其/以及）。
/// 关于修改本身的陈述句（"来源于原说明书……的明确记载""未超出……范围"）不是技术特征，
/// 不参与逐字核验（它们由断言降级通道单独处理）。
fn split_features(claim_text: &str) -> Vec<String> {
    let unified = claim_text.replace("及其", "，").replace("以及", "，");
    unified
        .split(['。', '；', ';', '\n', '，', ','])
        .map(|s| {
            s.trim()
                .trim_start_matches(['*', '-', '•', ' '])
                .to_string()
        })
        .filter(|s| !s.is_empty() && !is_declarative_meta(s))
        .collect()
}

fn is_declarative_meta(fragment: &str) -> bool {
    ASSERTION_MARKERS.iter().any(|m| fragment.contains(m))
        || fragment.contains("未超出")
        || fragment.contains("修改依据")
        || fragment.contains("修改理由")
        || fragment.contains("修改要点")
}

// ═══════════════════════════════════════════════════════════════════════
//  归一化：五类合法撰写变换的豁免
// ═══════════════════════════════════════════════════════════════════════

/// 归一化（两侧都做）：
/// ① 指代词（所述/前述/上述/该）增删；② 权项连接语（其特征在于/还包括/根据权利要求X所述的）；
/// ③ 附图标记与中英文括号；⑤ 数值范围符号与单位全半角。
/// ④ 标点由 verifier 的去标点宽松层豁免（此处不重复处理，保持单一职责）。
pub fn strip_legal_variation(s: &str) -> String {
    // ⑤ 全角 ASCII → 半角（单位/数字/运算符全半角等价）
    let half: String = s.chars().map(to_halfwidth).collect();

    // ③ 附图标记：括号内只有编号（(1)/(1a)/（1））
    let mut t = half;
    for re in [
        r"[（(]\s*\d+[a-zA-Z]?\s*[)）]",
        r"[\[［]\s*\d+[a-zA-Z]?\s*[\]］]",
    ] {
        if let Ok(re) = regex::Regex::new(re) {
            t = re.replace_all(&t, "").to_string();
        }
    }

    // ② 权项连接语（长的先删），含"根据权利要求X所述的"
    for re in [
        r"根据(?:上述)?权利要求\s*\d+\s*所述(?:的)?",
        r"如权利要求\s*\d+\s*所述(?:的)?",
        r"权利要求\s*\d+\s*所述(?:的)?",
        r"其?特征(?:还)?(?:在于|是)",
        r"(?:还|进一步)?包括",
    ] {
        if let Ok(re) = regex::Regex::new(re) {
            t = re.replace_all(&t, "").to_string();
        }
    }

    // ① 指代词
    for w in ["所述", "前述", "上述"] {
        t = t.replace(w, "");
    }
    // 「该」只在作指示代词时删（"应该""本该"保留）
    let chars: Vec<char> = t.chars().collect();
    let mut out = String::with_capacity(t.len());
    for (i, c) in chars.iter().enumerate() {
        if *c == '该' {
            let prev = if i > 0 { Some(chars[i - 1]) } else { None };
            if matches!(prev, Some('应') | Some('本')) {
                out.push(*c);
            }
            continue;
        }
        if *c == '至' {
            // ⑤ 「至」只在数字之间等价于范围号"-"（"至少"不受影响）
            let prev = if i > 0 { Some(chars[i - 1]) } else { None };
            let next = chars.get(i + 1).copied();
            if prev.map(|p| p.is_ascii_digit()).unwrap_or(false)
                && next.map(|n| n.is_ascii_digit()).unwrap_or(false)
            {
                out.push('-');
                continue;
            }
        }
        for r in ['～', '~', '—', '–', '－', '−'] {
            if *c == r {
                out.push('-');
                break;
            }
        }
        if matches!(c, '～' | '~' | '—' | '–' | '－' | '−') {
            continue;
        }
        out.push(*c);
    }
    out
}

fn to_halfwidth(c: char) -> char {
    let u = c as u32;
    if (0xFF01..=0xFF5E).contains(&u) {
        char::from_u32(u - 0xFEE0).unwrap_or(c)
    } else if u == 0x3000 {
        ' '
    } else {
        c
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  原始申请文本 → 证据语料（本申请：权利要求 + 具体实施方式段落）
// ═══════════════════════════════════════════════════════════════════════

/// 构造「本申请」语料：权利要求书各权项（`Claim` 三元组）+ 说明书具体实施方式各段
/// （`Paragraph` 三元组）。检索范围因此被限制在本申请内部，对比文件不参与。
pub fn build_own_application_corpus(original: &str) -> SourceCorpus {
    let text = original.trim();
    if text.is_empty() {
        return SourceCorpus::default();
    }

    let claims_region = claims_region(text);
    let spec = spec_region(text, claims_region);

    let mut sections: Vec<SourceSection> = Vec::new();

    if let Some((cs, ce)) = claims_region {
        let region = text.get(cs..ce).unwrap_or("");
        let claims = split_claims(region);
        for (i, body) in claims.into_iter().enumerate() {
            sections.push(SourceSection {
                section_id: format!("{OWN_DOC_ID}:claim:{}", i + 1),
                kind: LocatorKind::Claim,
                number: (i + 1) as u32,
                text: strip_legal_variation(&body),
            });
        }
    }

    for (i, para) in split_paragraphs(spec).into_iter().enumerate() {
        sections.push(SourceSection {
            section_id: format!("{OWN_DOC_ID}:para:{}", i + 1),
            kind: LocatorKind::Paragraph,
            number: (i + 1) as u32,
            text: strip_legal_variation(&para),
        });
    }

    if sections.is_empty() {
        return SourceCorpus::default();
    }
    SourceCorpus {
        docs: vec![SourceDoc {
            doc_id: OWN_DOC_ID.to_string(),
            kind: "OwnApplication".to_string(),
            title: "本申请原始文本".to_string(),
            sections,
        }],
    }
}

fn claims_region(text: &str) -> Option<(usize, usize)> {
    let start = text.find("权利要求书")?;
    let after = start + "权利要求书".len();
    let stop = ["说明书", "技术领域", "背景技术", "发明内容", "具体实施方式"]
        .iter()
        .filter_map(|m| text.get(after..).and_then(|s| s.find(m)).map(|p| after + p))
        .min()
        .unwrap_or(text.len());
    if stop <= after {
        return None;
    }
    Some((after, stop))
}

/// 说明书区域：优先「具体实施方式」之后；否则整篇（去掉权利要求书区域）。
fn spec_region(text: &str, claims: Option<(usize, usize)>) -> &str {
    if let Some(pos) = text.find("具体实施方式") {
        return text.get(pos..).unwrap_or("");
    }
    match claims {
        Some((cs, ce)) => {
            let head = text.get(..cs).unwrap_or("");
            let tail = text.get(ce..).unwrap_or("");
            if tail.trim().len() > head.trim().len() {
                tail
            } else {
                head
            }
        }
        None => text,
    }
}

fn split_claims(region: &str) -> Vec<String> {
    let Ok(re) = regex::Regex::new(r"(?m)^\s*(\d{1,3})\s*[.、．]") else {
        return vec![region.to_string()];
    };
    let hits: Vec<usize> = re.find_iter(region).map(|m| m.start()).collect();
    if hits.is_empty() {
        return vec![region.to_string()];
    }
    let mut out = Vec::new();
    for (i, s) in hits.iter().enumerate() {
        let e = hits.get(i + 1).copied().unwrap_or(region.len());
        let body = region.get(*s..e).unwrap_or("").trim();
        if body.chars().count() >= 10 {
            out.push(body.to_string());
        }
    }
    if out.is_empty() {
        vec![region.to_string()]
    } else {
        out
    }
}

fn split_paragraphs(spec: &str) -> Vec<String> {
    // 优先按段号 [0001]/【0001】切
    if let Ok(re) = regex::Regex::new(r"[\[【]\d{4}[\]】]") {
        let hits: Vec<(usize, usize)> = re.find_iter(spec).map(|m| (m.start(), m.end())).collect();
        if hits.len() >= 2 {
            let mut out = Vec::new();
            for (i, (_, body_start)) in hits.iter().enumerate() {
                let body_end = hits.get(i + 1).map(|h| h.0).unwrap_or(spec.len());
                let body = spec.get(*body_start..body_end).unwrap_or("").trim();
                if body.chars().count() >= 10 {
                    out.push(body.to_string());
                }
            }
            if !out.is_empty() {
                return out;
            }
        }
    }
    // 其次按空行切段
    let by_blank: Vec<String> = spec
        .split("\n\n")
        .map(|s| s.trim().to_string())
        .filter(|s| s.chars().count() >= 10)
        .collect();
    if by_blank.len() >= 2 {
        return by_blank;
    }
    vec![spec.trim().to_string()]
}

// ═══════════════════════════════════════════════════════════════════════
//  「明确记载」类断言降级
// ═══════════════════════════════════════════════════════════════════════

const ASSERTION_MARKERS: &[&str] = &[
    "明确记载",
    "来源于原说明书",
    "原说明书中有记载",
    "原说明书中记载",
    "原始申请文件记载",
];

const ATTRIBUTION_MARKERS: &[&str] = &["原说明书", "说明书", "原始申请", "原始文本"];

/// 逐行扫描：含"来源归属 + 明确记载类"断言的句子，整句替换为降级标注。
/// 只在存在零命中特征句时调用（无零命中 → 断言可能成立，不动）。
fn downgrade_spec_assertions(text: &str) -> (String, usize) {
    let mut count = 0usize;
    let mut out = String::with_capacity(text.len());
    let mut first = true;
    for line in text.split_inclusive('\n') {
        if !first {
            out.push('\n');
        }
        first = false;
        let body = line.strip_suffix('\n').unwrap_or(line);
        let has_attr = ATTRIBUTION_MARKERS.iter().any(|m| body.contains(m));
        let has_assert = ASSERTION_MARKERS.iter().any(|m| body.contains(m));
        if has_attr && has_assert {
            out.push_str(ASSERTION_DOWNGRADE_BLOCK);
            count += 1;
        } else {
            out.push_str(body);
        }
    }
    (out, count)
}

/// 便捷入口：一句话回答"这段修改稿能不能声称有记载"（供 fact_check 适配层使用）。
pub fn is_supported(feature: &str, original: &str) -> bool {
    let needle = strip_legal_variation(feature);
    if needle.chars().count() < MIN_FEATURE_CHARS || original.trim().is_empty() {
        return false;
    }
    let corpus = build_own_application_corpus(original);
    let index = CorpusIndex::build(&corpus);
    let hit = index.verify_quote_in_doc(
        &needle,
        OWN_DOC_ID,
        &[LocatorKind::Claim, LocatorKind::Paragraph],
    );
    judge(&needle, &hit).0
}

#[cfg(test)]
mod tests {
    use super::*;

    // 通用技术词夹具（无申请号/人名/公开号/案件专有长句）
    const SPEC: &str = "\
## 说明书\n\
### 技术领域\n\
本发明涉及一种物料输送装置。\n\
### 具体实施方式\n\
[0001] 一种物料输送装置，包括机架（1），机架（1）上设有输送带（2），机架（1）侧面固定有驱动电机（3）。\n\
[0002] 输送带（2）上方设有导料板（4），导料板（4）与输送带（2）之间的间距为20～30厘米。\n\
[0003] 驱动电机（3）的输出轴与输送带（2）的主动辊通过链条连接，链条外侧设有张紧轮（5）。\n\
[0004] 机架（1）底部设有可调支脚，可调支脚的调节行程不超过50毫米。\n";

    fn gate(letter: &str) -> AmendmentGateResult {
        gate_amendment_text(letter, SPEC)
    }

    // ── 正控：五类合法撰写变换各一条（修复前后均必须 ok，证明不误伤）──

    #[test]
    fn positive_control_1_pronoun_added_or_removed() {
        // ① 指代词增删："所述"在原文有、修改稿去掉（原文侧归一化同样去掉）
        let letter = "## 二、关于权利要求的修改\n\n修改后的权利要求1：\n一种物料输送装置，机架（1）上设有输送带（2），机架（1）侧面固定有驱动电机（3）。\n";
        let r = gate(letter);
        assert_eq!(
            r.unsupported_count, 0,
            "指代词增删不得误伤: {:?}",
            r.unsupported
        );
        assert!(r.gated_text.contains("一种物料输送装置"));
    }

    #[test]
    fn positive_control_2_connective_and_word_order() {
        // ② 权项连接语与语序：其特征在于 / 根据权利要求X所述的 / 还包括 的位置变化
        let letter = "## 二、关于权利要求的修改\n\n修改后的权利要求2：\n根据权利要求1所述的物料输送装置，其特征在于，还包括导料板（4），导料板（4）与输送带（2）之间的间距为20至30厘米。\n";
        let r = gate(letter);
        assert_eq!(
            r.unsupported_count, 0,
            "连接语/语序变化不得误伤: {:?}",
            r.unsupported
        );
    }

    #[test]
    fn positive_control_3_reference_signs_and_brackets() {
        // ③ 附图标记与中英文括号：修改稿去掉 (1)/(2)、保留全角括号
        let letter = "## 二、关于权利要求的修改\n\n修改后的权利要求1：\n一种物料输送装置，包括机架，机架上设有输送带，机架侧面固定有驱动电机。\n";
        let r = gate(letter);
        assert_eq!(
            r.unsupported_count, 0,
            "附图标记/括号差异不得误伤: {:?}",
            r.unsupported
        );
    }

    #[test]
    fn positive_control_4_punctuation() {
        // ④ 标点：修改稿用分号/无标点，原文用逗号/句号
        let letter = "## 二、关于权利要求的修改\n\n修改后的权利要求2：\n输送带上方设有导料板;导料板与输送带之间的间距为20~30厘米。\n";
        let r = gate(letter);
        assert_eq!(
            r.unsupported_count, 0,
            "标点差异不得误伤: {:?}",
            r.unsupported
        );
    }

    #[test]
    fn positive_control_5_number_range_and_units() {
        // ⑤ 数值范围符号与单位全半角：～/至/-，全角数字与符号
        let letter = "## 二、关于权利要求的修改\n\n修改后的权利要求3：\n驱动电机的输出轴与输送带的主动辊通过链条连接，链条外侧设有张紧轮（5），调节行程不超过５０毫米。\n";
        let r = gate(letter);
        assert_eq!(
            r.unsupported_count, 0,
            "数值范围/单位全半角差异不得误伤: {:?}",
            r.unsupported
        );
    }

    #[test]
    fn positive_control_6_dependent_claim_merged_into_independent() {
        // 从属权项并入独立权项（合法改写）：权2/权3 的特征在原文里都在
        let letter = "## 二、关于权利要求的修改\n\n修改后的权利要求1：\n一种物料输送装置，包括机架（1），机架（1）上设有输送带（2），机架（1）侧面固定有驱动电机（3），输送带（2）上方设有导料板（4），链条外侧设有张紧轮（5）。\n";
        let r = gate(letter);
        assert_eq!(
            r.unsupported_count, 0,
            "从属权项并入独立权项不得误伤: {:?}",
            r.unsupported
        );
    }

    // ── 负控：零命中却声称"明确记载" ──

    #[test]
    fn zero_hit_feature_is_marked_in_place() {
        let letter = "\
## 二、关于权利要求的修改\n\n\
修改后的权利要求1：\n\
一种物料输送装置，包括机架（1），其特征在于，还包括设置在机架上的隔离罩，隔离罩与输送带之间的间隙小于预设差值。\n\n\
上述修改均来源于原说明书具体实施方式的明确记载，未超出原始申请文件记载的范围。\n";
        let r = gate(letter);
        assert_eq!(r.status, GateStatus::Degraded);
        assert!(r.unsupported_count >= 1, "零命中必须被抓住: {r:?}");
        assert!(
            r.gated_text
                .contains("【未证实：该措辞在原始文本零命中】还包括设置在机架上的隔离罩"),
            "就地标注在特征句前: {}",
            r.gated_text
        );
        assert!(
            r.unsupported.iter().any(|u| u.text.contains("隔离罩")),
            "unsupported 清单须点名措辞: {:?}",
            r.unsupported
        );
        assert!(!r.submission_ready, "有零命中时提交度维度必须压死");
    }

    #[test]
    fn supported_features_are_not_marked() {
        let letter = "## 二、关于权利要求的修改\n\n修改后的权利要求1：\n一种物料输送装置，包括机架（1），机架上设有输送带（2）。\n";
        let r = gate(letter);
        assert_eq!(r.status, GateStatus::Verified);
        assert!(!r.gated_text.contains(UNVERIFIED_MARK));
        assert!(r.submission_ready);
    }

    #[test]
    fn assertion_downgraded_on_zero_hit() {
        let letter = "\
## 二、关于权利要求的修改\n\n\
修改后的权利要求1：\n\
一种物料输送装置，包括机架（1），机架上设有伸缩轴行程限位块。\n\n\
本修改来源于原说明书具体实施方式的明确记载。\n";
        let r = gate(letter);
        assert_eq!(r.unsupported_count, 1, "{:?}", r.unsupported);
        assert_eq!(r.assertions_downgraded, 1);
        assert!(
            !r.gated_text.contains("来源于原说明书"),
            "零命中后不得保留出处断言原句: {}",
            r.gated_text
        );
        assert!(
            r.gated_text.contains(ASSERTION_DOWNGRADE_BLOCK),
            "断言必须整句替换为降级标注: {}",
            r.gated_text
        );
    }

    #[test]
    fn no_amendment_block_is_noop() {
        let r = gate("## 一、关于审查意见的答复\n\n申请人认为对比文件未公开该特征。\n");
        assert_eq!(r.status, GateStatus::NoAmendment);
        assert_eq!(r.unsupported_count, 0);
    }

    #[test]
    fn missing_original_skips_gate_without_accusing() {
        let letter =
            "## 二、关于权利要求的修改\n\n修改后的权利要求1：\n一种物料输送装置，包括机架。\n";
        let r = gate_amendment_text(letter, "");
        assert_eq!(r.status, GateStatus::NoOriginal);
        assert_eq!(r.unsupported_count, 0, "缺原文不得凭空判零命中");
        assert_eq!(r.gated_text, letter);
    }

    #[test]
    fn annotation_keeps_original_sentence_intact() {
        let letter = "## 二、关于权利要求的修改\n\n修改后的权利要求1：\n一种物料输送装置，还包括设置在机架上的隔离罩。\n";
        let r = gate(letter);
        assert!(r.gated_text.contains("还包括设置在机架上的隔离罩"));
        assert_eq!(r.gated_text.matches(UNVERIFIED_MARK).count(), 1);
    }

    #[test]
    fn corpus_keeps_claims_and_spec_paragraphs_separate() {
        let original = "权利要求书\n1. 一种物料输送装置，包括机架。\n2. 根据权利要求1所述的装置，其特征在于，机架上设有输送带。\n说明书\n技术领域\n本发明涉及输送设备。\n具体实施方式\n[0001] 机架上设有输送带，输送带上方设有导料板。\n";
        let corpus = build_own_application_corpus(original);
        assert_eq!(corpus.docs.len(), 1);
        assert_eq!(corpus.docs[0].doc_id, OWN_DOC_ID);
        let claims = corpus.docs[0]
            .sections
            .iter()
            .filter(|s| s.kind == LocatorKind::Claim)
            .count();
        assert_eq!(claims, 2, "权利要求书应拆成 2 条权项三元组");
        let paras = corpus.docs[0]
            .sections
            .iter()
            .filter(|s| s.kind == LocatorKind::Paragraph)
            .count();
        assert!(paras >= 1, "具体实施方式应至少 1 段: {corpus:?}");
    }

    #[test]
    fn halfwidth_and_range_symbols_are_equivalent() {
        let a = strip_legal_variation("间距为２０～３０ｃｍ");
        let b = strip_legal_variation("间距为20-30cm");
        assert_eq!(a, b);
        let c = strip_legal_variation("调节行程20至30毫米");
        let d = strip_legal_variation("调节行程20-30毫米");
        assert_eq!(c, d);
        // "至少"不得被当成范围号
        assert!(strip_legal_variation("至少一个").contains("至少"));
    }
}
