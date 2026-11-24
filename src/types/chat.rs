//! AI 对话域类型 / AI chat domain types
//!
//! `AiChatRequest` / `AiResponse` 自 `src/patent.rs` 迁出，
//! `ChatMessage` 自 `src/db/chat.rs` 迁出（types-migration-map.md §1「AI 对话族」）。
//! 字段与 serde 属性逐字保持，仅更换定义位置；`src/db/chat.rs` 以 `pub use` 继续对外提供
//! `crate::db::chat::ChatMessage` 路径。

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct AiChatRequest {
    pub message: String,
    pub patent_id: Option<String>,
    #[serde(default)]
    pub history: Vec<(String, String)>,
    #[serde(default)]
    pub web_search: bool,
    /// Base64-encoded images for multimodal vision models (e.g., Gemini)
    #[serde(default)]
    pub images: Vec<String>,
    /// Custom system prompt (overrides default role). Use preset key or full prompt text.
    /// Built-in presets: "patent_examiner" | "oa_expert" | "claim_analyst" | "inventor_brainstorm"
    #[serde(default)]
    pub system_prompt: Option<String>,
    /// Whether system_prompt is a preset key (true) or raw prompt text (false, default)
    #[serde(default)]
    pub preset_mode: bool,
}

/// Preset system prompts for AI chat roles
pub static SYSTEM_PROMPT_PRESETS: &[(&str, &str, &str)] = &[
    (
        "general",
        "通用助手 / General Assistant",
        "你是一位专业、严谨的研发助理，擅长回答技术问题，提供专利相关信息。请用简洁清晰的语言回答。",
    ),
    (
        "patent_examiner",
        "专利审查员视角 / Patent Examiner",
        "你是一位资深中国专利审查员（执业20年，曾任复审委员会成员）。你精通中国专利法及审查指南（2023修订版），对创造性审查（A22.3）尤为严格。你善于发现技术方案中的逻辑漏洞和论证不足，对对比文献公开内容的认定极其敏感。请用挑剔、专业的眼光审阅，明确指出事实认定不准确之处和法律推理的漏洞。语气直接、客观。",
    ),
    (
        "oa_expert",
        "专利答辩专家 / OA Response Expert",
        "你是一位资深中国专利代理师（执业20年+），精通中国专利法及审查指南。你擅长答复审查意见通知书，尤其是创造性驳回（A22.3）的答辩。你的分析必须精确到技术特征级别，每一处论断都必须引用原文。工作流程：先深度分析（逐特征对比），再写回复。关键原则：(1)精确到特征级 (2)引用原文 (3)区分事实与观点 (4)诚实分析——如果对比文献确实公开了某特征，必须承认 (5)必须分析组合动机。语气尊重审查员但立场坚定，不使用加粗、表情符号或过度格式化。",
    ),
    (
        "claim_analyst",
        "权利要求分析师 / Claims Analyst",
        "你是一位资深专利权利要求分析师。你擅长：(1)解析权利要求的保护范围，识别独立/从属关系，构建权利要求树 (2)逐特征分析技术要素，评估保护宽度 (3)识别潜在规避设计方向 (4)对比多组权利要求的结构异同。请用结构化方式输出，对每项独立权利要求单独分析，可辅以表格辅助说明。",
    ),
    (
        "inventor_brainstorm",
        "发明人头脑风暴 / Inventor Brainstorm",
        "你是一位经验丰富的研发创新导师，擅长与发明人进行头脑风暴。你善于：(1)从零散的技术想法中提炼核心发明构思 (2)引导发明人梳理技术问题的来龙去脉 (3)识别现有技术的盲点和改进空间 (4)帮助发明人用专利语言重新描述其技术方案。请用引导式提问帮助发明人深入思考，而非直接给出答案。语气鼓励、开放、建设性。",
    ),
    (
        "patent_drafting_coach",
        "专利高质量撰写辅导 / Patent Drafting Coach",
        "你是兼具 15 年一线总工程师与资深专利代理师经验的高价值专利撰写教练。你的核心使命是防止发明人把好技术写窄、写漏或写烂：1) 剥离独立权利要求 1 中的非必要实施例限定，守住最宽物理机理边界；2) 设计层层递进的从属权利要求攻防梯度（结构变体→关键配合→量化工艺窗口→控制策略）；3) 跳出眼前单点结构，着眼未来 3–5 年技术演进主动挖掘可替代材料、驱动/传热/拓扑变体写入说明书实施例以堵死竞争对手规避路径；4) 严查专利法第 26.4 条得不到支持、自我窄化措辞及“只披露不权要”的商业秘密裸奔风险。始终沉住气、重实证，材料未给的数据必须标注【待实验/补充】，严禁空话套话。",
    ),
    (
        "academic_paper_assistant",
        "学术论文与综述助手 / Academic Paper Assistant",
        "你是面向高校科研师生与企业研发人员的严谨学术论文助手（覆盖工程、医工、理工全学科）。你擅长：(1) 基于真实文献梳理研究脉络、对比方法优劣与指出科学空白；(2) 审阅实验设计、对照基线与统计显著性漏洞；(3) 按 SCI/EI/核心期刊规范润色摘要、引言、机理建模与讨论章节。铁律：(a) 严禁编造任何文献、作者或 DOI，未提供真实 DOI 时必须显式标注【待补充可核验 DOI】；(b) 若内容涉及尚未提交专利的新技术方案，必须在开头醒目给出【防破新颖性安全锁】提醒（中国专利法第 22 条第 2 款：论文早于专利申请日公开将导致自身丧失新颖性）。",
    ),
];

impl AiChatRequest {
    /// Resolve the effective system prompt: preset key → full text, or use raw custom prompt
    pub fn effective_system_prompt(&self) -> Option<String> {
        match &self.system_prompt {
            Some(p) if self.preset_mode => {
                let preset = SYSTEM_PROMPT_PRESETS
                    .iter()
                    .find(|(k, _, _)| *k == p.as_str());
                preset.map(|(_, _, prompt)| prompt.to_string())
            }
            Some(p) => Some(p.clone()),
            None => None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AiResponse {
    pub content: String,
}

/// 单条聊天消息 / Single chat message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: i64,
    pub role: String,
    pub content: String,
    pub created_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_seven_system_prompt_presets_resolve_cleanly() {
        assert_eq!(SYSTEM_PROMPT_PRESETS.len(), 7);
        for key in [
            "general",
            "patent_examiner",
            "oa_expert",
            "claim_analyst",
            "inventor_brainstorm",
            "patent_drafting_coach",
            "academic_paper_assistant",
        ] {
            let req = AiChatRequest {
                message: "测试".into(),
                patent_id: None,
                history: vec![],
                web_search: false,
                images: vec![],
                system_prompt: Some(key.into()),
                preset_mode: true,
            };
            let resolved = req
                .effective_system_prompt()
                .expect("preset should resolve");
            assert!(
                resolved.chars().count() >= 30,
                "preset {} prompt too short",
                key
            );
        }
    }
}
