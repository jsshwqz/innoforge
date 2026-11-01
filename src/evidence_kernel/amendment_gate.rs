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
            GateStatus::NoAmendment => "skipped_no_amendment_section",
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
    /// 成品文本里 [`UNVERIFIED_MARK`] 的**实际处数**。
    ///
    /// 与 `unsupported_count` 平行而非相等：两条特征句若从同一位置开始（或一条是另一条的前缀），
    /// 标注会合并成一个位置，`unsupported_count` 仍逐条计数。真实案件实测 24 条特征句 →
    /// 正文 17 处标注。横幅与 meta 必须报"能数出来"的那个数，否则读者数一遍就对不上
    /// （errors.md ㊶④：展示数与产物数用同一次统计口径）。
    pub marked_count: usize,
    /// 扩范围方向（删限定 / 改限定）的检查结果：`text` 是被删掉的现权项限定句。
    ///
    /// 逐字核验只回答「新增内容是否超范围」（专利法第33条的上界）；**删掉一个限定
    /// ＝扩大保护范围**是另一头，必须拿"审查所针对的那版权项"当基准（issue #159）。
    pub broadened: Vec<UnsupportedFeature>,
    /// 是否真的做了扩范围方向检查。缺现权项文本时为 `false`——此时**必须在产物里可见**
    /// （[`AmendmentGateResult::direction_note`]），不能静默退化成只查单向。
    pub current_claims_checked: bool,
    /// 被替换为降级标注的"明确记载"类断言条数
    pub assertions_downgraded: usize,
    pub amended_claim_count: usize,
    pub checked_feature_count: usize,
    /// 语料里 `Claim` 三元组条数。为 0 说明**原始权利要求书没进语料**——此时"逐字核验"
    /// 实际只比对了说明书，任何"未命中"都可能是权项正文未被索引造成的误报，
    /// 不得以"已核验"口吻交付（issue #140 的可见性要求）。
    pub corpus_claims: usize,
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
            marked_count: 0,
            broadened: Vec::new(),
            current_claims_checked: false,
            assertions_downgraded: 0,
            amended_claim_count: 0,
            checked_feature_count: 0,
            corpus_claims: 0,
            submission_ready: gate_submission_ready(status),
        }
    }

    /// 扩范围方向的可见说明：**不通过**与**没核验**两种情况都要写出来。
    ///
    /// 逐字核验通过并不等于"可以交"：如果修改把现权项的某个限定删掉了，保护范围反而变宽，
    /// 而逐字核验对此完全无感（删掉的内容当然不在原文里"新增"）。
    pub fn direction_note(&self) -> String {
        if !self.broadened.is_empty() {
            return format!(
                "> ⚠️ 修改稿扩范围校验未通过（broadening_findings={}）：修改后的独立权项缺少现权项的部分限定；删除或改写限定等于扩大保护范围，不可直接递交。

",
                self.broadened.len()
            );
        }
        if !self.current_claims_checked
            && matches!(self.status, GateStatus::Verified | GateStatus::Degraded)
        {
            return "> ℹ️ 修改稿扩范围方向未核验：未提供「审查所针对的那版权项」文本，本次只做了「新增内容是否超范围」（专利法第33条）的单向核验。

".to_string();
        }
        String::new()
    }

    /// 阻断横幅（复用 M2-C 横幅形态：`> ⚠️` blockquote，随正文首部一并下发）。
    ///
    /// 横幅里**不得出现** [`UNVERIFIED_MARK`] 的字面量（含 `【】`）：交付文本的标记数是
    /// `横幅 + 正文`，横幅自带一次字面量就会让读者数出 N+1 个标记、与
    /// `amendment_unsupported=N` 对不上（真实案件实测 136 vs 137）。这里改用「」描述。
    ///
    /// 同时把「特征句条数」与「正文标注处数」分开报：两条特征句从同一位置开始时标注会合并，
    /// 真实案件实测 24 条 → 17 处。只报 24 会让读者数出 17，以为计数坏了。
    pub fn banner(&self) -> String {
        if self.unsupported_count == 0 {
            return String::new();
        }
        let merge_note = if self.marked_count == self.unsupported_count {
            String::new()
        } else {
            format!(
                "（同一位置上重叠的命中合并为一处标注，正文实际标注 {} 处）",
                self.marked_count
            )
        };
        format!(
            "> ⚠️ 修改稿核验阻断（amendment_unsupported={}）：本稿修改后的权利要求中有 {} 处特征句在原始申请文本（说明书 + 权利要求书）零命中，已就地逐处标注「未证实：该措辞在原始文本零命中」{}；相关「明确记载 / 来源于原说明书」类断言已降级。提交前必须逐处核对专利法第33条（修改超范围）。\n\n",
            self.unsupported_count, self.unsupported_count, merge_note
        )
    }

    /// 门未做逐字核验时的可见说明（贴在正文首部，避免"没有横幅＝已核验"的假绿）。
    ///
    /// - [`GateStatus::NoAmendment`]：答复书里没定位到修改稿段落——可能本答复确实不含权项修改，
    ///   也可能是模型没按标签格式写；两种情况都必须让读者知道"本次未核验"。
    /// - [`GateStatus::NoOriginal`]：缺原始申请文本，无比对基准。
    ///
    /// 其余状态（verified / degraded）返回空串。
    pub fn skip_note(&self) -> String {
        match self.status {
            GateStatus::NoAmendment => "> ℹ️ 修改稿逐字核验：未定位到「修改后的权利要求」段落，本次未做第33条逐字核验；若本答复含权利要求修改，请人工核对。\n\n".to_string(),
            GateStatus::NoOriginal => "> ℹ️ 修改稿逐字核验：缺少原始申请文本（说明书 + 权利要求书），本次未做第33条逐字核验。\n\n".to_string(),
            GateStatus::Verified | GateStatus::Degraded => {
                // 语料里一条权项都没有 ⇒ "逐字核验"实际只比对了说明书。此时任何零命中都可能
                // 是「权项正文未被索引」造成的误报，必须显式否认"已核验"（issue #140）。
                if self.corpus_claims == 0 {
                    "> ⚠️ 修改稿逐字核验：语料中未索引到任何权利要求条目，本次比对只覆盖说明书，权项正文可能被误判为零命中——不得据此认定已通过第33条核验。\n\n".to_string()
                } else {
                    String::new()
                }
            }
        }
    }
}

/// 剥掉本门自己产出的工件：就地标注、阻断横幅、未核验说明横幅。
///
/// 为什么必须在入口做：这些工件是**本函数写进正文**的东西。把它们当成用户文本再核验一次，
/// 就会出现两种畸形——①标注被切成特征句，一处零命中变两处（28 → 56）；②旧横幅留在产物里，
/// 与新一次核验的结论互相矛盾（门说 verified，首行还写着"核验阻断"）。issue #158。
fn strip_prior_gate_artifacts(text: &str) -> String {
    let without_marks = text.replace(UNVERIFIED_MARK, "");
    let mut out = String::with_capacity(without_marks.len() + 1);
    for line in without_marks.split('\n') {
        let t = line.trim_start();
        if t.starts_with("> ⚠️ 修改稿核验阻断")
            || t.starts_with("> ℹ️ 修改稿逐字核验")
            || t.starts_with("> ⚠️ 修改稿逐字核验")
        {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    if out.ends_with('\n') {
        out.pop();
    }
    out
}

/// 门主入口：核验并返回就地标注后的文本。
///
/// 纯函数、零 AI、零网络（与内核 ADR-1 一致）。
pub fn gate_amendment_text(letter: &str, original_application: &str) -> AmendmentGateResult {
    gate_amendment_text_with_current_claims(letter, original_application, "")
}

/// 带「审查所针对的那版权项」的入口：除了查新增超范围，还查**删限定/改限定**（扩范围）。
///
/// `current_claims` 为空 ⇒ 只做单向核验，但 `current_claims_checked=false` 会把这件事
/// 写进产物（[`AmendmentGateResult::direction_note`]）——不静默。
pub fn gate_amendment_text_with_current_claims(
    letter: &str,
    original_application: &str,
    current_claims: &str,
) -> AmendmentGateResult {
    // 幂等入口：先剥掉本函数自己上一轮的产出（就地标注 + 阻断/说明横幅），再核验。
    // 不剥的话，重跑时标注本身会被切成新的特征句再拿去比对，一处变两处（真实案件实测
    // 28 → 56）；旧横幅还会留在产物里跟新结论打架（issue #158）。
    let cleaned = strip_prior_gate_artifacts(letter);
    let letter = cleaned.as_str();
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
    let mut amended_claims: Vec<AmendedClaim> = Vec::new();
    for (bi, (start, end)) in blocks.iter().enumerate() {
        let block_text = letter.get(*start..*end).unwrap_or("");
        for claim in extract_claims(block_text) {
            claim_count += 1;
            amended_claims.push(claim.clone());
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
        // 定位到了块，却一句可核验的特征都没抽出来 ⇒ 一句也没核验。
        // 这不叫 verified（"核验通过"至少意味着核验过一句），按未核验处理：
        // 读者会从 skip note 读到"本次未做第33条逐字核验"，而不是从一个布尔值读出结论。
        return AmendmentGateResult::empty(GateStatus::NoAmendment, letter);
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
    // 计数口径：标注处数**从成品里数**，不是把插入次数累加。降级替换可能吞掉一处标注，
    // 同一位置的重复命中会被合并——这两种情况下"插入了几次"与"正文里有几处"不相等，
    // 读者只会数正文。
    let marked_count = gated.matches(UNVERIFIED_MARK).count();
    // 扩范围方向：拿"审查所针对的那版权项"当基准，逐条确认现权项的限定还在（issue #159）。
    // 缺现权项文本 ⇒ 不做，但把"没做"这件事标出来（current_claims_checked=false）。
    let (broadened, current_claims_checked) = if current_claims.trim().is_empty() {
        (Vec::new(), false)
    } else {
        (check_broadening(current_claims, &amended_claims), true)
    };
    let status = if !broadened.is_empty() {
        GateStatus::Degraded
    } else {
        status
    };
    AmendmentGateResult {
        status,
        gated_text: gated,
        unsupported_count: unsupported.len(),
        marked_count,
        broadened,
        current_claims_checked,
        unsupported,
        assertions_downgraded,
        amended_claim_count: claim_count,
        checked_feature_count: evals.len(),
        corpus_claims: claim_section_count(&corpus),
        submission_ready: gate_submission_ready(status),
    }
}

/// 提交度维度只看"核验过且全部通过"。
///
/// 曾经写成 `status != Degraded`，于是**没核验**（未定位到修改稿 / 缺原始文本）也会
/// 返回 `submission_ready=true`——"未发现违规"被当成了"核验通过"，是假绿灯里最难察觉
/// 的一种（调用方只看布尔值，不会去看 status）。三种未核验状态一律 `false`，
/// 让读者从 skip note 里读到"为什么没核验"，而不是从 `true` 里读到一个不存在的结论。
fn gate_submission_ready(status: GateStatus) -> bool {
    matches!(status, GateStatus::Verified)
}

/// 扩范围方向检查：现权项（审查所针对的那版）的每条限定，必须仍出现在修改后的独立权项里。
///
/// 逐字核验只回答「新增内容是否超范围」；`删掉一个限定＝扩大保护范围` 是另一头，
/// 必须拿现权项当基准（issue #159）。判定按"独立权项按出现顺序一一对应"，
/// 这是答复书的常规形态（改动集中在独立权项）；对应不上的，宁可报出来让人看。
fn check_broadening(current_claims: &str, amended: &[AmendedClaim]) -> Vec<UnsupportedFeature> {
    // 现权项通常写成「1. 一种…」，而 extract_claims 只认「权利要求N」，所以这里用宽松解析。
    let current = parse_claims_loose(current_claims);
    let cur_indep: Vec<&AmendedClaim> = current
        .iter()
        .filter(|c| !is_dependent_claim(&c.text))
        .collect();
    let new_indep: Vec<&AmendedClaim> = amended
        .iter()
        .filter(|c| !is_dependent_claim(&c.text))
        .collect();
    let mut out = Vec::new();
    if new_indep.len() > cur_indep.len() {
        out.push(UnsupportedFeature {
            claim_no: "-".to_string(),
            text: format!("独立权项数量 {} → {}", cur_indep.len(), new_indep.len()),
            reason: "独立权项数量增加＝保护范围扩大".to_string(),
            best_score: 0.0,
        });
    }
    for (i, c) in cur_indep.iter().enumerate() {
        let Some(new) = new_indep.get(i) else {
            out.push(UnsupportedFeature {
                claim_no: c.claim_no.clone(),
                text: "修改后找不到对应的独立权项".to_string(),
                reason: "现权项的独立权项在修改稿里缺失".to_string(),
                best_score: 0.0,
            });
            continue;
        };
        let hay = strip_legal_variation(&new.text);
        for raw in split_features(&c.text) {
            let needle = strip_legal_variation(&raw);
            if needle.chars().count() < MIN_FEATURE_CHARS {
                continue;
            }
            if !hay.contains(&needle) {
                out.push(UnsupportedFeature {
                    claim_no: c.claim_no.clone(),
                    text: raw,
                    reason:
                        "现权项的这个限定在修改后的独立权项里找不到（删除或改写＝可能扩大保护范围）"
                            .to_string(),
                    best_score: 0.0,
                });
            }
        }
    }
    out
}

/// 宽松权项解析：同时接受「权利要求1：…」与「1. 一种…」两种行首形态（现权项用后者）。
fn parse_claims_loose(text: &str) -> Vec<AmendedClaim> {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| {
        regex::Regex::new(
            r"(?m)^[ \t*#\-]*(?:修改后的)?(?:权利要求\s*([0-9]{1,3})|([0-9]{1,3})\s*[.．、])\s*",
        )
        .unwrap_or_else(|e| panic!("宽松权项正则常量失败即 bug: {e}"))
    });
    // 存 match 的**结束**位置：编号本身不进正文，否则「1.一种…」里的「1.」会被当成限定去比对。
    let heads: Vec<(usize, String)> = re
        .captures_iter(text)
        .map(|c| {
            let num = c
                .get(1)
                .or_else(|| c.get(2))
                .map(|m| m.as_str().to_string());
            (
                c.get(0).map(|m| m.end()).unwrap_or(0),
                num.unwrap_or_default(),
            )
        })
        .collect();
    let mut out = Vec::new();
    for i in 0..heads.len() {
        let end = if i + 1 < heads.len() {
            heads[i].0
        } else {
            text.len()
        };
        let start = heads[i].0;
        // 下一段的起点是"下一行的行首"，本段到此为止；两端都必须是字符边界。
        if let Some(body) = text.get(start..end.max(start)) {
            out.push(AmendedClaim {
                claim_no: heads[i].1.clone(),
                text: body.to_string(),
            });
        }
    }
    out
}

/// 从属权项的判据：正文开头出现引用型引导语。只看前 40 字，避免被权项里的长描述干扰。
fn is_dependent_claim(text: &str) -> bool {
    let head: String = text.chars().take(40).collect();
    head.contains("根据权利要求") || head.contains("如权利要求") || head.contains("按照权利要求")
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
///
/// **只有行首标签位的 marker 才算段落起点**（见 [`is_amendment_label`]）。答复书正文里的
/// 叙述句——"申请人认为修改后的权利要求1-3已克服全部缺陷"——不是起点：一旦把它当起点，
/// 块会一路延伸到下一节标题，块内所有非权项句子被当成特征句逐句核验、全部零命中、
/// 整封打满标记（真实案件实测 `amendment_unsupported=136`）。
pub fn amendment_blocks(text: &str) -> Vec<(usize, usize)> {
    let mut raw: Vec<(usize, usize)> = Vec::new();
    for marker in AMENDMENT_MARKERS {
        let mut from = 0usize;
        while let Some(pos) = text.get(from..).and_then(|s| s.find(marker)) {
            let abs = from + pos;
            if is_amendment_label(text, abs, marker) {
                raw.push((abs, block_end(text, abs)));
            }
            from = abs + marker.len().max(1);
            if raw.len() >= 8 {
                break;
            }
        }
    }
    // 「修改为：「…」」句式：模型常把修改文本直接放在引号里
    for (s, e) in quoted_amendment_ranges(text) {
        raw.push((s, e));
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
    // 「本节只是在说明为什么不修改」：定位到标题不等于有修改稿（issue #160）。
    // 只做"起点定位"会把整个论述小节当修改稿，整段论述被切句去和原文逐字比对，
    // 全部零命中，一封合法的"不修改"答复书因此被压成不可交。
    merged.retain(|(s, e)| !is_no_amendment_section(text, *s, *e));
    merged
}

/// 本节是否"声明不修改、且确实没有贴修改稿"。
///
/// 判据必须双向，缺一不可：
/// ① 节内有明确的不修改声明（`不修改权利要求` / `不作修改` / `不进行修改` …）；
/// ② 节内**既没有** `修改后的权利要求N：` 这类引出标签，**也没有** `1.一种…` /
///     `2.根据…` 式的编号权项行。
///
/// 只满足①不满足②的文本（声明不修改却又贴了权项正文）必须继续走核验——
/// 那是"改了一部分"，不是"没改"。
fn is_no_amendment_section(text: &str, start: usize, end: usize) -> bool {
    let Some(body) = text.get(start..end) else {
        return false;
    };
    if !NO_AMENDMENT_DECLARATIONS
        .iter()
        .any(|marker| body.contains(marker))
    {
        return false;
    }
    !has_claim_draft_text(body)
}

/// 节内是否出现"修改稿正文"的形态。命中任一条即认为本节带修改稿。
fn has_claim_draft_text(body: &str) -> bool {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| {
        // ① `修改后的权利要求1：` 引出标签；② `1.一种…` / `2.根据…` 编号权项行；
        // ③ `（3）一种…` 括号编号（复核发现：漏这条会把真修改稿整节跳过——宁可误红）
        regex::Regex::new(
            r"(?m)^[\s#>*\-•]*(?:修改后的?权利要求\s*[0-9一二三四五六七八九十]{0,4}\s*[：:]|[0-9]{1,2}\s*[.．、]\s*(?:一种|根据)|[（(][0-9一二三四五六七八九十]{1,3}[）)]\s*(?:一种|根据))",
        )
        .unwrap_or_else(|e| panic!("修改稿形态正则常量失败即 bug: {e}"))
    });
    re.is_match(body)
}

/// "本次不修改权利要求"类声明的词面。只用于①，必须与 [`has_claim_draft_text`] 一起判。
const NO_AMENDMENT_DECLARATIONS: &[&str] = &[
    "不修改权利要求",
    "未修改权利要求",
    "不作修改",
    "不进行修改",
    "不对权利要求进行修改",
    "不涉及权利要求修改",
];

/// 引号内权项全文的正则（单一出处，`amendment_blocks` 与 `extract_claims` 共用）。
///
/// 两条放宽都是真实答复书逼出来的（issue #142）：
/// ①**允许冒号后换行**——模型常写 `具体修改为：` 换行再给 `“一种……”`；
/// ②**长度上限 300 → 1200**——独立权项正文普遍超过 300 字，旧上限会把权项全文截没。
fn regex_quoted_amendment() -> &'static regex::Regex {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        regex::Regex::new(
            r#"(?:修改为|改为|修改文本为|修改后内容为|修改如下)\s*[：:]?\s*(?:[\r\n]+\s*)?[「“"]([^」”"]{2,1200})[」”"]"#,
        )
        .unwrap_or_else(|e| panic!("引号权项正则常量失败即 bug: {e}"))
    })
}

/// 文本中所有「引号内的修改稿正文」区间。
fn quoted_amendment_ranges(text: &str) -> Vec<(usize, usize)> {
    regex_quoted_amendment()
        .captures_iter(text)
        .filter_map(|c| c.get(1).map(|m| (m.start(), m.end())))
        .collect()
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

/// marker 之后的行内残留若短于此且不含句读，视为标签（`修改后的权利要求1：`）；
/// 超过此长度或带句读，说明这句话在**叙述**而不是在给修改稿（`……1-3已克服缺陷，理由如下：`）。
const LABEL_REST_MAX_CHARS: usize = 12;

/// 判定 `marker` 在 `abs` 处的出现是否处于「修改稿段落标签位」。
///
/// 三条同时成立才算：
/// ① 该行去掉行首装饰（`#`/`*`/`>`/列表符/空白）与节序号（`二、`/`(二)`/`3.`）后**以 marker 起始**；
/// ② marker 之后的行内残留 ≤ [`LABEL_REST_MAX_CHARS`] 字符，且不含 `，。；,;！？` 任一句子标点
///    （放行 `修改后的权利要求1-3：`，拦住 `……1-3已克服缺陷，理由如下。`）；
/// ③ 该行**要么是纯标题（残留为空），要么以冒号结尾**——即它在"引出修改稿正文"，
///    而不是在"给一段论述起小标题"。
///
/// ③ 是关键：真实答复书里 `（一）修改后的权利要求1` 是**创造性论述**的小标题，下面跟的是
/// 区别特征比对与论证，不是权项文本；把它当修改稿起点会让整节论述被逐句核验、全部零命中、
/// 被打满标记。只有 `二、关于权利要求的修改`（纯标题）与 `修改后的权利要求1：`（冒号引出）
/// 才算修改稿起点。
fn is_amendment_label(text: &str, abs: usize, marker: &str) -> bool {
    let line_start = text
        .get(..abs)
        .and_then(|head| head.rfind('\n'))
        .map(|i| i + 1)
        .unwrap_or(0);
    let line_end = text
        .get(abs..)
        .and_then(|tail| tail.find('\n'))
        .map(|i| abs + i)
        .unwrap_or(text.len());
    let line = match text.get(line_start..line_end) {
        Some(l) => l,
        None => return false,
    };
    let content = strip_label_decor(line);
    let after_marker = match content.strip_prefix(marker) {
        Some(rest) => rest,
        None => return false,
    };
    let residue = after_marker.trim_end_matches(['：', ':', '*', ' ', '\t', '、']);
    if residue.chars().count() > LABEL_REST_MAX_CHARS {
        return false;
    }
    if residue
        .chars()
        .any(|c| matches!(c, '，' | '。' | '；' | ',' | ';' | '！' | '？'))
    {
        return false;
    }
    // ③ 纯标题（`二、关于权利要求的修改`）或冒号引出（`修改后的权利要求1：`）才算起点
    let bare_heading = residue.is_empty();
    let introduces_text = content
        .trim_end_matches(['*', ' ', '\t'])
        .ends_with(['：', ':']);
    bare_heading || introduces_text
}

/// 剥掉行首装饰与节序号，返回真正的内容起点。
fn strip_label_decor(line: &str) -> &str {
    let mut t = line.trim_start_matches(['#', '*', '>', '-', '•', ' ', '\t', ':', '：']);
    // 中文序号：`二、` / `（二）` / `(二)`
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| {
        regex::Regex::new(
            r"^(?:[一二三四五六七八九十百]{1,3}[、.．]|[（(][一二三四五六七八九十百]{1,3}[）)])",
        )
        .unwrap_or_else(|e| panic!("节序号正则常量失败即 bug: {e}"))
    });
    if let Some(m) = re.find(t) {
        t = t.get(m.end()..).unwrap_or("");
    } else {
        // 阿拉伯序号：`3.` / `3、` / `(3)`
        static RE_NUM: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
        let ren = RE_NUM.get_or_init(|| {
            regex::Regex::new(r"^(?:\d{1,2}[、.．]|[（(]\d{1,2}[）)])")
                .unwrap_or_else(|e| panic!("数字节序号正则常量失败即 bug: {e}"))
        });
        if let Some(m) = ren.find(t) {
            t = t.get(m.end()..).unwrap_or("");
        }
    }
    t.trim_start_matches(['#', '*', '>', ' ', '\t'])
}

fn regex_section_heading() -> &'static regex::Regex {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        regex::Regex::new(r"^\*{0,2}[一二三四五六七八九十]{1,3}[、.．]")
            .unwrap_or_else(|e| panic!("section heading 正则常量失败即 bug: {e}"))
    })
}

#[derive(Clone)]
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
        // 没有行首「权利要求N」标题 ⇒ 先看块内有没有**引号内的权项全文**：
        // 有 ⇒ 只扫引号段。修改稿节里的叙述句（"为进一步突出本发明的实质性技术贡献"
        // "缩短审查周期"）不是权项正文，逐句核验只会产出误报（issue #142 实测 26 处）。
        // 无 ⇒ 保留旧回退（整块当一个权项），宁可粗筛也不静默放过。
        let quotes: Vec<String> = quoted_amendment_ranges(block)
            .into_iter()
            .filter_map(|(s, e)| block.get(s..e).map(|t| t.to_string()))
            .collect();
        if !quotes.is_empty() {
            return quotes
                .into_iter()
                .map(|text| AmendedClaim {
                    claim_no: "引号内".to_string(),
                    text,
                })
                .collect();
        }
        return vec![AmendedClaim {
            claim_no: "0".to_string(),
            text: block.to_string(),
        }];
    }
    let mut out = Vec::new();
    // 第一个「权利要求N」标题**之前**的内容也必须扫：真实答复书常写成
    // `1. 权利要求1的修改：\n在保留原权利要求1全部特征（含……）的基础上，从原说明书……`
    // ——这种带前导序号的写法不匹配权项标题正则，若只从第一个命中标题开始扫，
    // 33 条真正要抓的那一段反而被跳过（v4 复跑实测：136 收敛到 2，但那 2 处都不是真缺陷，
    // 真缺陷段落被静默略过＝把过火换成了假绿）。块自身的小标题行不是权项文本，剥掉。
    let prefix = block.get(..hits[0].0).unwrap_or("");
    let prefix_body = prefix.split_once('\n').map(|(_, rest)| rest).unwrap_or("");
    if prefix_body.trim().chars().count() >= MIN_FEATURE_CHARS {
        out.push(AmendedClaim {
            claim_no: "未编号".to_string(),
            text: prefix_body.to_string(),
        });
    }
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
        .filter(|s| !s.is_empty() && !is_declarative_meta(s) && !is_heading_fragment(s))
        .collect()
}

/// 形如 `1. 权利要求1的修改：` 的行是**小标题**而不是技术特征句：短且以冒号结尾。
/// 与块起点判据（[`is_amendment_label`]：纯标题或冒号引出）同源，不另造第二套概念——
/// 区别只在于：块起点看"这行是否引出修改稿"，这里看"这句是不是修改稿里的一句标题"。
///
/// 2026-10-30 补：小节标题本身**没有冒号**（`二、关于权利要求的修改`），却会被当成特征句
/// 核验并就地标注，产物里出现 `二、【未证实…】关于权利要求的修改`——标题被破坏，且计入
/// 零命中数。模板小节名是固定措辞，按结构词直接排除。
fn is_heading_fragment(fragment: &str) -> bool {
    let t = fragment.trim_end_matches(['*', ' ', '\t']);
    if t.ends_with(['：', ':']) && t.chars().count() <= 20 {
        return true;
    }
    let bare = t.trim_start_matches(|c: char| c.is_ascii_digit() || matches!(c, '、' | '．' | '.'));
    matches!(
        bare.trim(),
        "关于权利要求的修改" | "关于权利要求的修改（如有）" | "权利要求的修改"
    ) || is_claim_label_line(bare.trim())
}

/// `修改后的权利要求1` / `修改后的权利要求` 这种**不带冒号的引出标签**也不是特征句。
/// 判据从严：必须以"修改后的权利要求"开头、只允许跟随数字，且总长 ≤ 12 字——
/// 真实特征句后面一定还有技术内容，不会长成这个样子。
fn is_claim_label_line(line: &str) -> bool {
    let Some(rest) = line.strip_prefix("修改后的权利要求") else {
        return false;
    };
    let rest = rest.trim_matches(['*', ' ', '\t']);
    rest.chars().count() <= 4 && rest.chars().all(|c| c.is_ascii_digit())
}

fn is_declarative_meta(fragment: &str) -> bool {
    ASSERTION_MARKERS.iter().any(|m| fragment.contains(m))
        || fragment.contains("未超出")
        || fragment.contains("修改依据")
        || fragment.contains("修改理由")
        || fragment.contains("修改要点")
        || is_amendment_act(fragment)
}

/// 叙述"本次做了哪种修改"的句子不是技术特征句。
///
/// 真实产物形态：`本次修改系将原权利要求2的附加技术特征并入权利要求1` —— 它描述的是
/// **修改动作本身**（把哪一权项并进来），不是并入后权利要求里的技术内容；拿去和原始
/// 申请文本做逐字比对必然零命中，于是把一份本来达标的稿子压成 `submission_ready=false`。
///
/// 判据要求三件事同时出现，避免误放行真特征句：
/// ①修改叙述主语（本次/上述/如下修改）；②合并类动词（并入/合并）；③权项指代（权利要求）。
/// 纯技术特征句不会自带"本次修改系将…"这种主语。
fn is_amendment_act(fragment: &str) -> bool {
    let says_amend = fragment.contains("本次修改")
        || fragment.contains("上述修改")
        || fragment.contains("如下修改");
    let says_merge = fragment.contains("并入") || fragment.contains("合并");
    let refers_claim = fragment.contains("权利要求");
    says_amend && says_merge && refers_claim
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
///
/// 输入形态有两种，都必须认得（issue #140）：
/// ①**标题式纯文本**——含「权利要求书 / 说明书 / 具体实施方式」这类小节名；
/// ②**本应用自己的规范形态**——`my_patent` 是一段 JSON 字符串
/// （`title / abstract_text / claims / description`）。主 UI 与答复书网关传的就是这种。
/// 形态②先经 [`flatten_json_application`] 摊平成形态①再走同一套分节逻辑，
/// **不另造第二套抽取**；摊平失败（不是 JSON、或没有权项字段）时按原文继续，
/// 由 `corpus_claims==0` 这条可见性把"语料里没有权项"暴露出来。
pub fn build_own_application_corpus(original: &str) -> SourceCorpus {
    let flattened = flatten_json_application(original);
    let text = match flattened.as_deref() {
        Some(f) => f.trim(),
        None => original.trim(),
    };
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

/// 把本应用的规范 JSON 形态摊平成「权利要求书 / 说明书」标题式文本。
///
/// 认得 `claims`（或 `claims_under_review`）与 `description`（或 `specification`）四类键；
/// 只要有一个可用字段就返回摊平结果。返回 `None` 表示"这不是 JSON 或没有可用字段"，
/// 调用方按原文继续——**不在此处猜测、也不静默造内容**。
///
/// 摊平顺序刻意与人工撰写的原始申请文本一致（权项在前、说明书在后），
/// 使 `claims_region` / `spec_region` 的既有语义原样复用。
pub(crate) fn flatten_json_application(text: &str) -> Option<String> {
    let trimmed = text.trim_start();
    if !trimmed.starts_with('{') {
        return None;
    }
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    let field = |keys: &[&str]| -> Option<String> {
        keys.iter().find_map(|k| {
            v.get(*k)
                .and_then(|x| x.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        })
    };
    let claims = field(&["claims", "claims_under_review"]);
    let spec = field(&["description", "specification", "description_text"]);
    let abstract_text = field(&["abstract_text", "abstract"]);
    if claims.is_none() && spec.is_none() {
        return None;
    }
    let mut out = String::new();
    if let Some(c) = &claims {
        out.push_str("权利要求书\n");
        out.push_str(c);
        out.push('\n');
    }
    out.push_str("说明书\n");
    if let Some(a) = &abstract_text {
        out.push_str("技术领域\n");
        out.push_str(a);
        out.push('\n');
    }
    if let Some(s) = &spec {
        out.push_str("具体实施方式\n");
        out.push_str(s);
        out.push('\n');
    }
    Some(out)
}

/// 答复书提示词里的「原始申请文本」块。
///
/// 与核验门**共用同一份归一化**（[`flatten_json_application`]）：门拿什么去比对，
/// 模型就必须看到什么——否则模型"逐字引用"的是另一种排版，必然被判零命中。
///
/// 空输入返回**可见的**未提供说明：提示词里留空，模型就会自己编修改稿
/// （v7 实测 22 处特征句原文零命中，绝大多数是模型凭空补的技术措辞）。
pub fn original_application_block(text: &str) -> String {
    let flat = flatten_json_application(text).unwrap_or_else(|| text.trim().to_string());
    if flat.trim().is_empty() {
        "（未提供原始申请文本：本次不得输出任何「修改后的权利要求」正文，只能给出修改方向并标注【需申请人确认】）"
            .to_string()
    } else {
        flat
    }
}

/// 语料里的权项三元组条数（可见性用：0 表示原始权利要求书没进语料）。
fn claim_section_count(corpus: &SourceCorpus) -> usize {
    corpus
        .docs
        .first()
        .map(|d| {
            d.sections
                .iter()
                .filter(|s| s.kind == LocatorKind::Claim)
                .count()
        })
        .unwrap_or(0)
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
    fn amendment_act_narration_is_meta_but_real_feature_still_checked() {
        // 真实形态（B-1 整封复跑时抓到）：说明"本次把权2并入权1"的句子描述的是修改动作，
        // 不是修改后权利要求里的技术特征；它进了逐字核验就会把达标的稿子压成不可交。
        let letter = "\
## 二、关于权利要求的修改\n\n\
修改后的权利要求1：\n\
一种物料输送装置，包括机架（1），机架（1）上设有输送带（2）。\n\
本次修改系将原权利要求2的附加技术特征并入权利要求1\n\
机架侧面还固定有气动夹爪组件与压力传感器\n";
        let r = gate(letter);
        assert_eq!(
            r.unsupported_count,
            1,
            "叙述句不该计数、真编造特征句必须仍被抓到： {:?}",
            r.unsupported
                .iter()
                .map(|u| u.text.chars().count())
                .collect::<Vec<_>>()
        );
        assert!(
            !r.gated_text.contains("】本次修改系将"),
            "修改动作叙述句不得被就地标注: {}",
            r.gated_text
        );
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

    // ── 作用域：marker 只在行首标签位才开块（过火回归守卫）──

    #[test]
    fn prose_mention_of_marker_does_not_open_block() {
        // 真实过火形态①：正文叙述句里提到"修改后的权利要求1-3"，句中含逗号句号
        let text = "申请人认为修改后的权利要求1-3已克服审查意见指出的全部缺陷，具体理由如下。\n";
        assert!(
            amendment_blocks(text).is_empty(),
            "叙述句里的 marker 不得开块: {:?}",
            amendment_blocks(text)
        );
    }

    #[test]
    fn argumentation_subheading_does_not_open_block() {
        // 真实过火形态②（v4 实测的主因）：`（一）修改后的权利要求1` 是**创造性论述**的小标题，
        // 下面跟的是区别特征比对与论证，不是权项文本；当起点会把整节论述打满标记。
        let text = "\
三、关于创造性的论述

（一）修改后的权利要求1

1. 区别特征确认
将修改后的权利要求1与D1对比，D1公开了管道与电动执行器。
区别特征A：计算机控制器内设有PLC程序，D1未公开。
";
        assert!(
            amendment_blocks(text).is_empty(),
            "论述小标题不得开块: {:?}",
            amendment_blocks(text)
        );
    }

    #[test]
    fn label_forms_still_open_block() {
        // 四种真标签写法都必须继续开块（否则就是将过火修成漏检）
        let cases = [
            "修改后的权利要求1：\n一种物料输送装置，包括机架。\n",
            "**修改后的权利要求 1**：\n一种物料输送装置，包括机架。\n",
            "（二）关于权利要求的修改\n修改后的权利要求书：\n1. 一种物料输送装置，包括机架。\n",
            "## 修改后的权利要求1-3：\n一种物料输送装置，包括机架。\n",
        ];
        for c in cases {
            assert!(!amendment_blocks(c).is_empty(), "标签写法未被识别: {c:?}");
        }
    }

    #[test]
    fn banner_and_body_marker_counts_agree() {
        // 交付文本 = 横幅 + gated；整段里 UNVERIFIED_MARK 的字面量数必须等于 unsupported_count。
        // （修复前横幅自带一次字面量，读者数出 N+1——真实案件实测 136 vs 137）
        let letter = "\
## 二、关于权利要求的修改\n\n\
修改后的权利要求1：\n\
一种物料输送装置，包括机架（1），其特征在于，还包括设置在机架上的隔离罩。\n";
        let r = gate(letter);
        assert_eq!(r.unsupported_count, 1, "{:?}", r.unsupported);
        let delivered = format!("{}{}", r.banner(), r.gated_text);
        assert_eq!(
            delivered.matches(UNVERIFIED_MARK).count(),
            r.marked_count,
            "横幅计数与正文标记数必须一致: {delivered}"
        );
        assert_eq!(
            r.marked_count,
            r.gated_text.matches(UNVERIFIED_MARK).count(),
            "marked_count 必须是从成品里数出来的"
        );
    }

    #[test]
    fn marked_count_is_the_number_a_reader_can_count() {
        // 真实案件实测：24 条零命中特征句，正文只有 17 处标注（同位置重叠合并）。
        // 只报条数时读者数一遍就对不上，会直接不信整张表。
        let letter = "\
## 二、关于权利要求的修改\n\n\
修改后的权利要求1：\n\
一种物料输送装置，包括机架（1），还包括虚构的磁悬浮导轨与虚构的量子阀。\n";
        let r = gate(letter);
        assert!(r.unsupported_count > 0);
        assert_eq!(
            r.marked_count,
            r.gated_text.matches(UNVERIFIED_MARK).count(),
            "marked_count 必须等于正文里能数出来的处数"
        );
        let delivered = format!("{}{}", r.banner(), r.gated_text);
        assert_eq!(
            delivered.matches(UNVERIFIED_MARK).count(),
            r.marked_count,
            "横幅里不得再带一次字面量"
        );
        if r.unsupported_count != r.marked_count {
            assert!(
                r.banner().contains("正文实际标注"),
                "条数与处数不等时横幅必须说明: {}",
                r.banner()
            );
        }
    }

    // ── issue #142：真实版式的修改稿（带前导序号小标题 + 跨行引号全文）──

    /// 跨行引号必须被接住：`具体修改为：` 换行后给 `“一种……”`。
    /// 节内叙述句（"为进一步突出…""缩短审查周期"）**不得**被当特征句打标；
    /// 引号内权项全文才是核验对象，其中真正无原文依据的措辞必须被抓到。
    #[test]
    fn multiline_quoted_claim_is_scanned_and_narrative_is_not() {
        let letter = "\
=== 意见陈述书 ===

二、关于权利要求的修改
1. 权利要求1的修改说明：
为进一步突出本发明的实质性技术贡献，缩短审查周期，申请人对权利要求书进行了限定性修改，具体修改为：
“一种物料输送装置，其特征在于，包括机架，机架上设有输送带，机架侧面固定有驱动电机，且设有隔离罩。”

三、关于创造性的论述
本申请具备创造性。
";
        let r = gate_amendment_text(letter, SPEC);
        // 叙述句零标记：整封信里只允许出现引号段内部的标记
        let narrative_zone = r
            .gated_text
            .split_once('“')
            .map(|(head, _)| head)
            .unwrap_or("");
        assert_eq!(
            narrative_zone.matches(UNVERIFIED_MARK).count(),
            0,
            "引号之前的叙述句不得被标注:\n{}",
            r.gated_text
        );
        assert!(
            r.unsupported.iter().any(|u| u.text.contains("隔离罩")),
            "引号内权项全文的零命中措辞必须被抓到: {:?}",
            r.unsupported
        );
        assert!(
            r.unsupported.iter().all(|u| u.claim_no == "引号内"),
            "本例核验对象应全部来自引号段: {:?}",
            r.unsupported
                .iter()
                .map(|u| u.claim_no.clone())
                .collect::<Vec<_>>()
        );
    }

    /// 既无行首权项标题、也无引号段 ⇒ **保留**旧回退（整块当一个权项继续粗筛）。
    /// 这是刻意选择：宁可多标也不静默放过——修改稿哪怕写得不成样子，33 条核验也不能
    /// 变成"没找到正文所以通过"。与 issue #142 原文里"返回 skipped"的设想相反，
    /// 已在 issue 下注明取舍理由。
    #[test]
    fn block_without_header_or_quote_still_falls_back() {
        let letter = "## 二、关于权利要求的修改\n\n在保留原权利要求1全部特征的基础上，增加设置于机架上的隔离罩。\n";
        let r = gate_amendment_text(letter, SPEC);
        assert!(
            !r.unsupported.is_empty(),
            "无标题无引号时仍须回退整块核验，不得静默放过"
        );
        assert!(
            r.unsupported.iter().any(|u| u.text.contains("隔离罩")),
            "回退路径必须仍能抓到零命中措辞: {:?}",
            r.unsupported
        );
    }

    /// 引号段长度上限从 300 提到 1200：真实独立权项普遍超过 300 字，
    /// 旧上限会把权项全文截断成半句，导致核验对象错位。
    #[test]
    fn long_quoted_claim_is_not_truncated() {
        let long_claim = format!(
            "一种物料输送装置，其特征在于，包括机架，机架上设有输送带，机架侧面固定有驱动电机，{}，并设有隔离罩。",
            "导料板与输送带之间的间距为20至30厘米，".repeat(12)
        );
        assert!(
            long_claim.chars().count() > 300,
            "夹具须超过旧上限 300 字才有意义"
        );
        let letter = format!(
            "## 二、关于权利要求的修改\n\n修改后的权利要求书如下：\n具体修改为：\n“{long_claim}”\n"
        );
        let ranges = quoted_amendment_ranges(&letter);
        assert_eq!(ranges.len(), 1, "长引号段应被完整捕获为一处");
        let captured = letter.get(ranges[0].0..ranges[0].1).unwrap_or("");
        assert!(
            captured.ends_with("并设有隔离罩。"),
            "捕获内容必须到引号结尾，实际尾部: …{}",
            captured.chars().rev().take(12).collect::<String>()
        );
    }

    // ── issue #140：语料必须认得本应用自己的 JSON 形态 my_patent ──

    /// JSON 形态（主 UI 与答复书网关实际传的就是这种）：权项正文必须进语料。
    #[test]
    fn json_shaped_input_indexes_claims() {
        let json = concat!(
            r#"{"title":"一种物料输送装置","abstract_text":"本发明涉及输送设备。","#,
            r#""claims":"1.一种物料输送装置，其特征在于，包括机架，机架上设有输送带，机架侧面固定有驱动电机。\n"#,
            r#"2.根据权利要求1所述的装置，其特征在于，输送带上方设有导料板。","#,
            r#""description":"具体实施方式\n[0001] 机架上设有导料板，导料板与输送带之间的间距为20～30厘米。"}"#
        );
        let corpus = build_own_application_corpus(json);
        let claims = claim_section_count(&corpus);
        assert!(
            claims >= 2,
            "JSON 的 claims 应拆出 ≥2 条权项三元组，实际 {claims}"
        );
        // 逐字取自 claims 的修改稿不得被判零命中（v5 实测误报 30 处的根因）
        let letter = "## 二、关于权利要求的修改\n\n修改后的权利要求1：\n一种物料输送装置，其特征在于，包括机架，机架上设有输送带，机架侧面固定有驱动电机。\n";
        let r = gate_amendment_text(letter, json);
        assert_eq!(r.corpus_claims, claims, "corpus_claims 必须与语料一致");
        assert_eq!(
            r.unsupported_count,
            0,
            "逐字来自原权利要求书的修改稿不得被误判: {:?} / 标记 {}",
            r.unsupported,
            r.gated_text.matches(UNVERIFIED_MARK).count()
        );
        assert!(
            r.skip_note().is_empty(),
            "有权项语料时不得再挂缺基准说明: {}",
            r.skip_note()
        );
    }

    /// 只有说明书、没有权项 ⇒ 必须显式否认"已核验"，不得让读者以为比过了原文。
    #[test]
    fn corpus_without_claims_is_visible_not_silent() {
        let spec_only =
            "说明书\n技术领域\n本发明涉及输送设备。\n具体实施方式\n[0001] 机架上设有输送带。\n";
        let letter = "## 二、关于权利要求的修改\n\n修改后的权利要求1：\n一种物料输送装置，其特征在于，机架上设有输送带，且设有隔离罩。\n";
        let r = gate_amendment_text(letter, spec_only);
        assert_eq!(r.corpus_claims, 0, "纯说明书输入不应有权项三元组");
        assert!(
            r.skip_note().contains("未索引到任何权利要求"),
            "语料缺权项时必须显式说明，实际: {:?}",
            r.skip_note()
        );
    }

    /// `claims_under_review` 是别名键，也必须认得。
    #[test]
    fn json_alias_claims_under_review_is_recognised() {
        let json = r#"{"claims_under_review":"1.一种物料输送装置，包括机架。","description":"具体实施方式\n[0001] 机架上设有输送带。"}"#;
        let corpus = build_own_application_corpus(json);
        assert!(
            claim_section_count(&corpus) >= 1,
            "别名键 claims_under_review 必须被认得"
        );
    }

    /// 不是 JSON（或 JSON 里没有可用字段）时不得瞎猜，按原文继续。
    #[test]
    fn non_json_input_falls_back_unchanged() {
        assert!(
            flatten_json_application(SPEC).is_none(),
            "标题式纯文本不该被当 JSON"
        );
        assert!(
            flatten_json_application(r#"{"title":"只有标题"}"#).is_none(),
            "没有 claims/description 的 JSON 不得凭空造语料"
        );
    }

    #[test]
    fn skip_note_only_for_unverified_statuses() {
        let no_section = gate("## 一、关于审查意见的答复\n\n申请人认为对比文件未公开该特征。\n");
        assert_eq!(no_section.status, GateStatus::NoAmendment);
        assert!(no_section.skip_note().contains("未定位到"));
        assert_eq!(no_section.status.as_str(), "skipped_no_amendment_section");

        let no_original = gate_amendment_text(
            "## 二、关于权利要求的修改\n\n修改后的权利要求1：\n一种物料输送装置，包括机架。\n",
            "",
        );
        assert!(!no_original.skip_note().is_empty());

        let verified = gate(
            "## 二、关于权利要求的修改\n\n修改后的权利要求1：\n一种物料输送装置，包括机架。\n",
        );
        assert_eq!(verified.status, GateStatus::Verified);
        // SPEC 夹具只有说明书、没有「权利要求书」分节 ⇒ 语料无权项，
        // 新行为要求此时挂"语料缺权项"说明（#140），但**不得**挂"未定位到修改稿"那种
        // 与事实相反的说明——两者是不同的可见性通道，这里分别钉住。
        assert!(
            !verified.skip_note().contains("未定位到"),
            "已找到修改稿时不得声称未定位到: {}",
            verified.skip_note()
        );
        assert_eq!(
            verified.corpus_claims, 0,
            "SPEC 夹具无权利要求书分节，corpus_claims 应为 0"
        );
        assert!(
            verified.skip_note().contains("未索引到任何权利要求"),
            "语料缺权项时即使判 verified 也必须挂缺基准说明: {:?}",
            verified.skip_note()
        );
    }

    #[test]
    fn prompt_block_flattens_json_so_model_sees_what_the_gate_compares() {
        let json = serde_json::json!({
            "claims": "1. 一种物料输送装置，包括机架。",
            "description": "[0001] 机架上设有输送带。"
        })
        .to_string();
        let block = original_application_block(&json);
        assert!(block.contains("权利要求书"), "{block}");
        assert!(block.contains("一种物料输送装置"), "{block}");
        assert!(block.contains("[0001] 机架上设有输送带。"), "{block}");
        // 与门比对用的归一化必须是同一份
        assert_eq!(
            block,
            flatten_json_application(&json).expect("JSON 形态应能摊平")
        );
    }

    #[test]
    fn prompt_block_keeps_plain_text_as_is() {
        let plain =
            "权利要求书\n1. 一种物料输送装置，包括机架。\n说明书\n[0001] 机架上设有输送带。\n";
        assert_eq!(original_application_block(plain), plain.trim());
    }

    #[test]
    fn prompt_block_missing_original_is_visible_not_empty() {
        // 提示词里留空，模型就会自己编修改稿；必须是看得见的"未提供"
        let block = original_application_block("   \n  ");
        assert!(block.contains("未提供原始申请文本"), "{block}");
        assert!(
            block.contains("不得输出任何「修改后的权利要求」正文"),
            "缺失时必须明令禁止编修改稿: {block}"
        );
    }

    // ── issue #160：不修改答复书被整节当修改稿核验（真实案件撞出）──

    #[test]
    fn no_amendment_declaration_is_not_treated_as_amendment_draft() {
        // 真实形态：标题按模板写「二、关于权利要求的修改」，本节讲的是"为什么不改"。
        // 修复前：本节整段被当修改稿切句，论述句逐句零命中，合法稿被压成 graded/degraded。
        let letter = "\
## 二、关于权利要求的修改

本次答复不修改权利要求。

申请人认为，现行权利要求1已以电缸直行程限定与D1的区别。
通知书认定的三项区别特征并非彼此独立的单项替换。
";
        let r = gate(letter);
        assert_eq!(r.status, GateStatus::NoAmendment);
        assert_eq!(r.unsupported_count, 0, "论述句不该被计成零命中特征");
        assert_eq!(
            r.gated_text.matches(UNVERIFIED_MARK).count(),
            0,
            "正文不得出现标注：{}",
            r.gated_text
        );
        assert!(!r.submission_ready, "没核验就不能说可交");
    }

    #[test]
    fn declaration_of_no_amendment_does_not_hide_an_actual_claim_draft() {
        // 负控：声明"不修改权利要求2"但确实贴了修改后的权利要求1 ⇒ 必须继续核验，
        // 不能因为句子里有"不修改"就整节跳过。
        let letter = "\
## 二、关于权利要求的修改

本次不修改权利要求2，修改后的权利要求1：

1.一种物料输送装置，机架（1）上设有输送带（2），机架（1）侧面固定有驱动电机（3）。
";
        let r = gate(letter);
        assert_ne!(
            r.status,
            GateStatus::NoAmendment,
            "带修改稿的节不得被判为无修改稿"
        );
        assert!(r.checked_feature_count > 0, "必须真的核验了特征句");
    }

    #[test]
    fn unverified_statuses_are_never_submission_ready() {
        // 假绿灯：`submission_ready` 曾经写成 `status != Degraded`，
        // 于是"未定位到修改稿""缺原始文本"这两种**没核验**的状态都返回 true。
        let no_section = gate("## 一、关于审查意见的答复\n\n申请人认为对比文件未公开该特征。\n");
        assert_eq!(no_section.status, GateStatus::NoAmendment);
        assert!(!no_section.submission_ready, "未定位到修改稿 ≠ 可交");

        let no_original = gate_amendment_text(
            "## 二、关于权利要求的修改\n\n修改后的权利要求1：\n一种物料输送装置，包括机架（1）。\n",
            "",
        );
        assert_eq!(no_original.status, GateStatus::NoOriginal);
        assert!(!no_original.submission_ready, "缺原始文本 ≠ 可交");

        let verified = gate(
            "## 二、关于权利要求的修改\n\n修改后的权利要求1：\n一种物料输送装置，机架（1）上设有输送带（2）。\n",
        );
        assert_eq!(verified.status, GateStatus::Verified);
        assert!(verified.submission_ready, "核验通过才可交");
    }

    // ── issue #158：门不幂等（重跑把标注本身算成新零命中；旧横幅留在产物里）──

    #[test]
    fn gate_is_idempotent_over_its_own_output() {
        let letter = "## 二、关于权利要求的修改\n\n修改后的权利要求1：\n一种物料输送装置，机架上设有虚构的磁悬浮导轨。\n";
        let once = gate(letter);
        assert!(once.unsupported_count > 0, "这个夹具本来就该零命中");
        let twice = gate(&once.gated_text);
        assert_eq!(
            twice.unsupported_count, once.unsupported_count,
            "重跑不得把上一轮的标注算成新的零命中特征"
        );
        assert_eq!(
            twice.gated_text.matches(UNVERIFIED_MARK).count(),
            once.gated_text.matches(UNVERIFIED_MARK).count(),
            "标注处数必须一致（旧行为是 1 → 2）"
        );
    }

    #[test]
    fn stale_banner_is_stripped_so_the_artifact_stops_contradicting_itself() {
        // 真实形态：手改过的稿子正文已干净，首行旧横幅还在，写着"核验阻断 28 处"。
        let clean = "## 二、关于权利要求的修改\n\n修改后的权利要求1：\n一种物料输送装置，机架（1）上设有输送带（2）。\n";
        let stale =
            format!("> ⚠️ 修改稿核验阻断（amendment_unsupported=28）：旧的一次。\n\n{clean}");
        let r = gate(&stale);
        assert_eq!(r.status, GateStatus::Verified);
        assert_eq!(r.unsupported_count, 0);
        assert!(
            !r.gated_text.contains("修改稿核验阻断"),
            "旧横幅必须剥掉，否则产物与新结论自相矛盾：{}",
            r.gated_text
        );
    }

    // ── 复核补漏（第三方复核命中）：括号编号的权项行也必须算"有修改稿"──

    #[test]
    fn no_amendment_declaration_with_parenthesised_claim_draft_still_verified() {
        // 声明"不作修改"，但正文确实贴了（3）一种… 的修改稿。
        // 修复前：编号只认 `3.一种`，括号编号不命中 ⇒ 整节被当"无修改稿"跳过（静默漏检）。
        let letter = "## 二、关于权利要求的修改

本次对独立权利要求1不作修改。

（3）一种物料输送装置，机架（1）上设有输送带（2），机架（1）侧面固定有驱动电机（3）。
";
        let r = gate(letter);
        assert_ne!(
            r.status,
            GateStatus::NoAmendment,
            "括号编号的修改稿不得被当成\"无修改稿\"跳过：{}",
            r.gated_text
        );
        assert!(r.checked_feature_count > 0, "必须真的核验了特征句");
    }

    #[test]
    fn plain_numbered_claim_draft_is_still_recognised() {
        // 负控的另一半：阿拉伯点号编号仍要认（改动不能把常见形态弄坏）。
        let letter = "## 二、关于权利要求的修改

本次对独立权利要求1不作修改。

3.一种物料输送装置，机架（1）上设有输送带（2）。
";
        let r = gate(letter);
        assert_ne!(
            r.status,
            GateStatus::NoAmendment,
            "3.一种… 必须仍被识别为修改稿"
        );
    }

    // ── issue #159：删限定＝扩大保护范围（现权项当基准）──

    #[test]
    fn deleting_a_limitation_is_flagged_as_broadening() {
        let current = "1.一种物料输送装置，其特征在于，包括机架（1），机架（1）上设有输送带（2），机架（1）侧面固定有防护罩（3）。
";
        let letter = "## 二、关于权利要求的修改

修改后的权利要求1：
一种物料输送装置，包括机架（1），机架（1）上设有输送带（2）。
";
        let r = gate_amendment_text_with_current_claims(letter, SPEC, current);
        assert!(
            !r.broadened.is_empty(),
            "删掉「防护罩」必须被报成扩范围，实际 broadened={:?}",
            r.broadened
        );
        assert_eq!(r.status, GateStatus::Degraded);
        assert!(!r.submission_ready, "扩范围不得判可交");
        assert!(
            r.direction_note().contains("扩范围"),
            "{}",
            r.direction_note()
        );
    }

    #[test]
    fn adding_a_limitation_alone_is_not_broadening() {
        let current = "1.一种物料输送装置，其特征在于，包括机架（1），机架（1）上设有输送带（2）。
";
        let letter = "## 二、关于权利要求的修改

修改后的权利要求1：
一种物料输送装置，包括机架（1），机架（1）上设有输送带（2），机架（1）侧面固定有驱动电机（3）。
";
        let r = gate_amendment_text_with_current_claims(letter, SPEC, current);
        assert!(
            r.broadened.is_empty(),
            "只增加限定不算扩范围：{:?}",
            r.broadened
        );
        assert!(r.current_claims_checked);
        assert!(
            !r.direction_note().contains("扩范围"),
            "只加不减不该报扩范围"
        );
    }

    #[test]
    fn missing_current_claims_is_visible_not_silent() {
        let letter = "## 二、关于权利要求的修改

修改后的权利要求1：
一种物料输送装置，机架（1）上设有输送带（2）。
";
        let r = gate(letter);
        assert!(r.broadened.is_empty());
        assert!(
            !r.current_claims_checked,
            "没给现权项时必须是未核验，而不是悄悄当通过"
        );
        assert!(
            r.direction_note().contains("未核验"),
            "缺输入的可见性说明缺失: {}",
            r.direction_note()
        );
    }
}
