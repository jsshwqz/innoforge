//! 模块职责：v0.7.0 研发问题结构化四元组拆解器（v0.9.69）。
//! 上游调用者：`POST /api/idea/decompose`（`src/routes/idea_reports.rs`）与前端 `templates/idea.html`。
//! 模块专属禁区：离线或无 AI Key 时必须确定性拆解并返回可用四元组，绝不编造原文未提及的定量指标。

use crate::ai::AiClient;
use serde::{Deserialize, Serialize};

/// 研发问题结构化四元组：技术对象 + 量化目标效果 + 工程边界约束 + 核心矛盾对。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProblemQuadruple {
    /// 1. 技术对象（核心装置/材料/算法/工艺载体）
    pub technical_object: String,
    /// 2. 量化目标效果（原文提取或标明待补充量化的目标指标）
    pub quantitative_target_effect: String,
    /// 3. 工程边界约束（工况、成本、空间、寿命、法规或接口限制）
    pub engineering_constraints: Vec<String>,
    /// 4. 核心矛盾对（如“高散热能力 vs 密封防漏/低体积成本”）
    pub core_contradiction_pair: String,
    /// 推荐检索种子词（可直接用于后续查新）
    pub suggested_queries: Vec<String>,
    /// 拆解来源：`"ai"` 或 `"deterministic"`
    pub source: String,
}

/// 确定性四元组拆解器（零联网、100% 稳定，作为无 Key 降级基线与 AI 缺失字段兜底）。
pub fn decompose_deterministic(title: &str, description: &str) -> ProblemQuadruple {
    let t = title.trim();
    let d = description.trim();
    let full = format!("{t}。{d}");

    // 1. 技术对象：优先取标题核心主语
    let technical_object = if !t.is_empty() {
        t.to_string()
    } else {
        d.split(['，', '。', '；', '\n'])
            .next()
            .unwrap_or("待定义技术对象")
            .trim()
            .chars()
            .take(40)
            .collect()
    };

    // 2. 量化目标效果：抽取含数字+物理单位/百分比的句子
    let quant_units = [
        "℃",
        "°C",
        "MPa",
        "kPa",
        "kW",
        "MW",
        "mm",
        "cm",
        "μm",
        "nm",
        "%",
        "rpm",
        "Hz",
        "小时",
        "提升",
        "降低",
        "减少",
        "达到",
        "优于",
        "控制在",
        "nH",
        "mΩ",
        "kV",
        "K/W",
        "W/m",
    ];
    let target_sents: Vec<String> = full
        .split(['。', '；', '\n'])
        .map(str::trim)
        .filter(|s| {
            s.chars().count() >= 4
                && s.chars().any(|c| c.is_ascii_digit())
                && quant_units.iter().any(|u| s.contains(u))
        })
        .take(2)
        .map(|s| s.chars().take(220).collect())
        .collect();
    let quantitative_target_effect = if !target_sents.is_empty() {
        target_sents.join("；")
    } else {
        "原文尚未给出具体定量指标（建议补充目标温升ΔT / 效率提升% / 寿命小时数 / 成本上限）"
            .to_string()
    };

    // 3. 工程边界约束：抽取含约束关键词的短句
    let constraint_cues = [
        "不破坏",
        "原有",
        "空间",
        "成本",
        "体积",
        "重量",
        "密封",
        "防漏",
        "寿命",
        "高温",
        "低温",
        "兼容",
        "免改造",
        "无毒",
        "安全",
        "约束",
        "限制",
        "必须",
        "不能",
        "不得",
    ];
    let mut engineering_constraints: Vec<String> = full
        .split(['。', '；', '，', '\n'])
        .map(str::trim)
        .filter(|s| s.chars().count() >= 4 && constraint_cues.iter().any(|cue| s.contains(cue)))
        .take(4)
        .map(|s| s.chars().take(50).collect())
        .collect();
    if engineering_constraints.is_empty() {
        engineering_constraints
            .push("待补充：工况载荷、接口兼容性、量产成本与寿命边界".to_string());
    }

    // 4. 核心矛盾对：检测常见工程矛盾关键词
    let core_contradiction_pair = if full.contains("散热") || full.contains("冷却") {
        if full.contains("漏") || full.contains("密封") || full.contains("负压") {
            "提升直接换热散热能力 vs 防止冷却介质沿轴封/接口外泄与绝缘劣化".to_string()
        } else {
            "提升峰值散热通量 vs 控制系统体积、泵功耗与改造成本".to_string()
        }
    } else if full.contains("燃烧") || full.contains("锅炉") || full.contains("煤粉") {
        "提高燃烧效率与稳燃范围 vs 抑制 NOx 排放与防止结焦过热".to_string()
    } else if full.contains("电池") || full.contains("电解质") {
        "提升能量密度与快充倍率 vs 保持循环界面稳定性与热失控安全".to_string()
    } else if full.contains("过滤") || full.contains("分离") {
        "提高过滤分离精度 vs 降低流体压降与延长滤芯维护周期".to_string()
    } else {
        format!(
            "提升「{}」核心性能指标 vs 控制结构复杂度、可靠性风险与制造成本",
            technical_object.chars().take(18).collect::<String>()
        )
    };

    let mut suggested_queries = Vec::new();
    if !t.is_empty() {
        suggested_queries.push(t.to_string());
    }
    let extra_terms: Vec<&str> = full
        .split(['，', '。', '；', '、', ' ', '\n'])
        .map(str::trim)
        .filter(|s| s.chars().count() >= 3 && s.chars().count() <= 16)
        .take(4)
        .collect();
    for term in extra_terms {
        if !suggested_queries.iter().any(|q| q == term) {
            suggested_queries.push(term.to_string());
        }
    }

    ProblemQuadruple {
        technical_object,
        quantitative_target_effect,
        engineering_constraints,
        core_contradiction_pair,
        suggested_queries,
        source: "deterministic".to_string(),
    }
}

/// 尝试从 AI 响应 JSON 中解析四元组；破损或缺失字段时回退至确定性基线。
pub fn parse_ai_quadruple(raw: &str, fallback: &ProblemQuadruple) -> Option<ProblemQuadruple> {
    let trimmed = raw.trim();
    let s = trimmed.find('{')?;
    let e = trimmed.rfind('}')?;
    let json_slice = if e > s {
        trimmed.get(s..=e)?
    } else {
        return None;
    };
    let val: serde_json::Value = serde_json::from_str(json_slice).ok()?;
    let technical_object = val["technical_object"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| fallback.technical_object.clone());
    let quantitative_target_effect = val["quantitative_target_effect"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| fallback.quantitative_target_effect.clone());
    let engineering_constraints: Vec<String> = val["engineering_constraints"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .take(6)
                .map(str::to_string)
                .collect()
        })
        .filter(|v: &Vec<String>| !v.is_empty())
        .unwrap_or_else(|| fallback.engineering_constraints.clone());
    let core_contradiction_pair = val["core_contradiction_pair"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| fallback.core_contradiction_pair.clone());
    let suggested_queries: Vec<String> = val["suggested_queries"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .take(6)
                .map(str::to_string)
                .collect()
        })
        .filter(|v: &Vec<String>| !v.is_empty())
        .unwrap_or_else(|| fallback.suggested_queries.clone());

    Some(ProblemQuadruple {
        technical_object,
        quantitative_target_effect,
        engineering_constraints,
        core_contradiction_pair,
        suggested_queries,
        source: "ai".to_string(),
    })
}

/// 使用快模型通道（`ModelRole::FastRewrite`）执行四元组拆解，失败自动无损回退确定性引擎。
pub async fn decompose_with_ai(
    title: &str,
    description: &str,
    ai: Option<&AiClient>,
) -> ProblemQuadruple {
    let baseline = decompose_deterministic(title, description);
    let Some(client) = ai else {
        return baseline;
    };
    let prompt = format!(
        "请将以下研发创意拆解为严格 JSON 格式的「研发问题四元组」，用于在专利查新前精准锚定问题本质（严禁编造原文未给出的具体数值，若原文缺数值则如实标明「待补充量化」）：\n\n\
         【标题】{title}\n【描述】{description}\n\n\
         仅输出合法 JSON（不要多余解释）：\n\
         {{\n\
           \"technical_object\": \"核心技术对象（装置/系统/材料/工艺）\",\n\
           \"quantitative_target_effect\": \"量化目标效果（保留原文指标或标明待量化维度）\",\n\
           \"engineering_constraints\": [\"工程边界约束1\", \"工程边界约束2\"],\n\
           \"core_contradiction_pair\": \"核心物理/工程矛盾对（改善A vs 恶化B）\",\n\
           \"suggested_queries\": [\"检索词1\", \"检索词2\", \"英文检索词3\"]\n\
         }}"
    );
    let sys = "你是资深首席研发工程师与专利检索专家，擅长把模糊创意拆解为精准的工程四元组与核心物理矛盾。只输出 JSON。";
    match tokio::time::timeout(
        std::time::Duration::from_secs(25),
        client.chat_with_system(sys, &prompt, 0.2),
    )
    .await
    {
        Ok(Ok(raw)) => parse_ai_quadruple(&raw, &baseline).unwrap_or(baseline),
        _ => baseline,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_decomposer_extracts_quant_target_constraints_and_contradiction() {
        let q = decompose_deterministic(
            "永磁电机微负压浸没冷却系统",
            "在不破坏电机原有轴封结构前提下，采用绝缘硅油浸没绕组并通过微负压泵吸防止冷却液外泄，将绕组峰值温度从145℃降至85℃以下，温升降低40%。",
        );
        assert_eq!(q.source, "deterministic");
        assert!(q.technical_object.contains("永磁电机"));
        assert!(
            q.quantitative_target_effect.contains("145℃")
                || q.quantitative_target_effect.contains("40%"),
            "应提取原文真实定量指标: {:?}",
            q
        );
        assert!(
            q.engineering_constraints
                .iter()
                .any(|c| c.contains("不破坏") || c.contains("轴封")),
            "应提取工程约束: {:?}",
            q.engineering_constraints
        );
        assert!(q.core_contradiction_pair.contains("vs"));
    }

    #[test]
    fn parse_ai_quadruple_falls_back_cleanly_on_invalid_json() {
        let fb = decompose_deterministic("电机散热", "提升散热效率");
        assert!(parse_ai_quadruple("这不是JSON", &fb).is_none());
        let ok = parse_ai_quadruple(
            r#"```json
            {"technical_object":"定子浸没液冷系统","quantitative_target_effect":"峰值温升降低30%","engineering_constraints":["复用原润滑油口"],"core_contradiction_pair":"直接换热面积 vs 轴承密封防泄","suggested_queries":["motor immersion cooling"]}
            ```"#,
            &fb,
        )
        .expect("应成功解析带围栏 JSON");
        assert_eq!(ok.source, "ai");
        assert_eq!(ok.technical_object, "定子浸没液冷系统");
    }
}
