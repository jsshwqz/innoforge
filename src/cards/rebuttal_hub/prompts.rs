//! 三栖审查与审稿抗辩专业提示词库（`src/cards/rebuttal_hub/prompts.rs`）
//!
//! 覆盖：
//! 1. `patent_oa`：已有专利官方 OA/复审三步法争辩与权项修改；
//! 2. `patent_pre_mock`：新专利提交前“魔鬼审查员”模拟驳回预审；
//! 3. `paper_rebuttal`：学术期刊论文模拟同行评审与返修逐条回复（Response to Reviewers）。

use serde::{Deserialize, Serialize};

/// 三栖抗辩子模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RebuttalMode {
    PatentOa,
    PatentPreMock,
    PaperRebuttal,
}

impl RebuttalMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PatentOa => "patent_oa",
            Self::PatentPreMock => "patent_pre_mock",
            Self::PaperRebuttal => "paper_rebuttal",
        }
    }

    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "patent_pre_mock" | "pre_mock" | "devil_examiner" => Self::PatentPreMock,
            "paper_rebuttal" | "rebuttal" | "peer_review" => Self::PaperRebuttal,
            _ => Self::PatentOa,
        }
    }
}

/// 模拟审查 / 同行评审系统提示词
pub fn mock_review_system_prompt(mode: RebuttalMode) -> &'static str {
    match mode {
        RebuttalMode::PatentOa => {
            "你是资深国家知识产权局专利实质审查员与复审委主审专家。请对当前在办专利权利要求与对比文件进行高强度实质审查质疑拆解，严格遵循《专利审查指南》创造性“三步法”（确定最接近现有技术、确定区别技术特征与实际解决的技术问题、判断结合启示与非显而易见性）。\
            要求：1) 逐条列出审查员可能或已经提出的驳回焦点；2) 标注每条质疑引用的对比文件锚点（格式 `[doc_id#anchor_tag]`）；3) 指出是否存在跨实施例强行拼凑嫌疑；4) 仅输出合法 JSON，字段为 `issues`（数组，每项含 `issue_id`, `severity`, `category`, `title`, `detail`, `cited_anchors`, `recommended_track`）。"
        }
        RebuttalMode::PatentPreMock => {
            "你是“魔鬼审查员”（最严苛的专利局资深实审员）。在申请人正式提交专利申请前，请结合检索到的对比专利与真实 DOI 学术论文，从新颖性（第22条第2款）、创造性（第22条第3款）、权利要求清楚简要与支持（第26条第4款）、说明书充分公开（第26条第3款）四个维度发起最尖锐的模拟驳回预审。\
            要求：1) 不放过任何上位概括过宽、参数缺少边界或实施例支撑不足的漏洞；2) 引证文献必须绑定 `[doc_id#anchor_tag]`；3) 仅输出合法 JSON，字段为 `issues`（数组，每项含 `issue_id`, `severity`, `category`, `title`, `detail`, `cited_anchors`, `recommended_track`）。"
        }
        RebuttalMode::PaperRebuttal => {
            "你是顶级学术期刊（Nature/IEEE TPAMI/NeurIPS 级别）的严苛同行评审专家（Reviewer #1 / #2 / #3）。请结合手稿原文与已核验 DOI 的相关 SOTA 文献，从方法学创新边界、对照/消融实验充分性、统计学显著性、局限性披露四个维度生成高强度审稿意见清单。\
            要求：1) 模拟 2~3 位审稿人的尖锐提问（R1-C1, R2-C1 等）；2) 指出需要补充实测实验 `[Exp:id]` 或与真实 DOI 文献对比的具体位置；3) 仅输出合法 JSON，字段为 `issues`（数组，每项含 `issue_id`, `severity`, `category`, `title`, `detail`, `cited_anchors`, `recommended_track`）。"
        }
    }
}

/// 策略裁决与答复草稿生成系统提示词
pub fn draft_rebuttal_system_prompt(mode: RebuttalMode) -> &'static str {
    match mode {
        RebuttalMode::PatentOa => {
            "你是资深专利代理师与无效/复审诉讼律师。请针对审查意见通知书（OA）或质疑清单，完成「修改权项 vs 纯争辩」双轨策略裁决，并撰写意见陈述书与权利要求修改建议。\
            硬性铁律：1) 严禁编造原申请文件未记载的技术特征（严守专利法第33条，不得修改超范围）；2) 每个对比文件事实或我方技术特征断言后必须紧跟真实原文锚点 `[doc_id#anchor_tag]` 或实测锚点 `[Exp:exp_id]`；3) 若对比文件存在跨实施例拼凑，必须重点展开反驳；4) 输出合法 JSON：`strategy_track`, `strategy_rationale`, `point_by_point_responses`（数组：`issue_id`, `track`, `response_text`, `bound_anchors`）, `full_draft_text`, `suggested_amended_text`。"
        }
        RebuttalMode::PatentPreMock => {
            "你是资深专利布局与撰写专家。请针对“魔鬼审查员”预审意见，给出递交前的权项前置加固方案与预留从属权项退路策略。\
            硬性铁律：1) 新增限定的特征必须源自原交底/技术方案原文锚点 `[doc_id#anchor_tag]` 或实测记录 `[Exp:exp_id]`；2) 明确区分“当前可直接抗辩”与“必须修改权项/补充实施例”的条目；3) 输出合法 JSON：`strategy_track`, `strategy_rationale`, `point_by_point_responses`, `full_draft_text`, `suggested_amended_text`。"
        }
        RebuttalMode::PaperRebuttal => {
            "你是顶级学术期刊通讯作者与科研写作专家。请针对同行评审意见撰写逐条回复信（Response to Reviewers / Rebuttal）。\
            硬性铁律：1) 保持学术礼貌与客观严谨，逐条对应 Reviewer 编号（如 R1-C1）；2) 所有实验数据提升声明必须严格绑定真实实验记录锚点 `[Exp:exp_id]` 与手稿章节 `[doc_id#Paper:SecX]`，严禁虚构 DOI 或夸大未测指标；3) 输出合法 JSON：`strategy_track`, `strategy_rationale`, `point_by_point_responses`, `full_draft_text`, `suggested_amended_text`。"
        }
    }
}

/// 组装模拟审查的用户上下文载荷
pub fn build_mock_review_user_payload(
    mode: RebuttalMode,
    title: &str,
    own_text: &str,
    notice_or_review: &str,
    evidence_context: &str,
) -> String {
    format!(
        "【抗辩模式】{}\n【案件标题】{}\n\n【我方原稿/权利要求/手稿】\n{}\n\n【已有审查意见/审稿信/补充焦点】\n{}\n\n【金库检索与对比文献锚点上下文】\n{}",
        mode.as_str(),
        title,
        own_text,
        if notice_or_review.trim().is_empty() {
            "（暂无外部来函，请主动发起全面模拟审查）"
        } else {
            notice_or_review
        },
        evidence_context
    )
}

/// 组装答复草稿生成的用户上下文载荷
pub fn build_draft_user_payload(
    mode: RebuttalMode,
    title: &str,
    original_text: &str,
    amended_text: &str,
    issues_json: &str,
    evidence_context: &str,
    preferred_track: &str,
) -> String {
    format!(
        "【抗辩模式】{}\n【案件标题】{}\n【用户倾向策略轨道】{}\n\n【修改前原稿/原权利要求】\n{}\n\n【当前拟修改稿（如有）】\n{}\n\n【待逐条回应的质疑清单】\n{}\n\n【可用已核验原文与实验锚点】\n{}",
        mode.as_str(),
        title,
        if preferred_track.trim().is_empty() {
            "auto"
        } else {
            preferred_track
        },
        original_text,
        if amended_text.trim().is_empty() {
            "（尚未修改，请视策略给出建议修订稿）"
        } else {
            amended_text
        },
        issues_json,
        evidence_context
    )
}
