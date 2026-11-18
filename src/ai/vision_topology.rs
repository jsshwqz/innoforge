//! 专利附图多模态看图联动与部件标号视觉核对模块。
//!
//! 结合多模态视觉模型提取专利附图中的部件标号、连接关系、驱动方向与介质隔离特征，
//! 并与 `part_topology` 从说明书原文提取的文本拓扑表交叉核对，
//! 输出图文一致项、图文矛盾/遗漏项，以及供 OA Stage D 使用的反驳上下文。

use super::client::{safe_truncate_chars, AiClient};
use super::strategy_arbitration::part_topology::{
    extract_part_designators, extract_topology_relations,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 附图中视觉识别出的单个部件标号与构件形态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FigurePartItem {
    pub number: String,
    pub name: String,
    #[serde(default)]
    pub morphology: String,
}

/// 附图中视觉识别出的连接关系、驱动方向或流道走势
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FigureRelation {
    pub part_a: String,
    pub part_b: String,
    pub relation: String,
    #[serde(default)]
    pub drive_or_flow: String,
}

/// 图与文交叉核对发现的矛盾或遗漏项
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FigureDiscrepancy {
    pub designator: String,
    /// `missing_in_text` | `missing_in_figure` | `name_mismatch` | `topology_risk`
    pub kind: String,
    pub figure_desc: String,
    pub text_desc: String,
    pub detail: String,
}

/// 专利附图视觉分析与图文核对完整结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FigureAnalysisResult {
    /// `"ok"` | `"degraded"`
    pub status: String,
    pub vision_model_used: String,
    pub summary: String,
    pub parts: Vec<FigurePartItem>,
    pub relations: Vec<FigureRelation>,
    pub interference_or_isolation_risks: Vec<String>,
    pub consistent_pairs: Vec<String>,
    pub discrepancies: Vec<FigureDiscrepancy>,
    pub stage_d_rebuttal_context: String,
}

#[derive(Deserialize, Default)]
struct RawVisionPayload {
    #[serde(default)]
    summary: String,
    #[serde(default)]
    parts: Vec<FigurePartItem>,
    #[serde(default)]
    relations: Vec<FigureRelation>,
    #[serde(default)]
    interference_or_isolation_risks: Vec<String>,
}

/// 构造专利附图多模态视觉提取 Prompt（带 `<user_input>` 隔离防注入）。
pub fn build_figure_vision_prompt(spec_text: &str) -> (String, String) {
    let system = "你是资深机械/电气专利审查与结构分析专家。请严格只依据提供的专利附图视觉内容提取结构化 JSON，严禁臆测图中不存在的标号或关系。".to_string();
    let excerpt = safe_truncate_chars(spec_text.trim(), 2500);
    let prompt = format!(
        "请分析附图中的全部可见部件标号、构件形态、连接关系、驱动方向及介质隔离/流道走向，\
并重点检查是否存在运动干涉或未隔离介质风险。\n\
请严格返回合法 JSON（不要附加解释文字），格式如下：\n\
{{\n\
  \"summary\": \"附图整体结构简述\",\n\
  \"parts\": [{{\"number\": \"1\", \"name\": \"部件名称\", \"morphology\": \"形状与位置特征\"}}],\n\
  \"relations\": [{{\"part_a\": \"1\", \"part_b\": \"2\", \"relation\": \"连接/驱动方式\", \"drive_or_flow\": \"方向或流道走势\"}}],\n\
  \"interference_or_isolation_risks\": [\"运动干涉或介质未隔离风险说明（若无则空数组）\"]\n\
}}\n\n\
以下是供对照参考的说明书片段（可能存在与附图不一致之处，请以附图实际可见结构为准）：\n\
<user_input>\n{excerpt}\n</user_input>"
    );
    (system, prompt)
}

fn strip_markdown_json(raw: &str) -> &str {
    let trimmed = raw.trim();
    if let Some(rest) = trimmed.strip_prefix("```json") {
        if let Some(inner) = rest.strip_suffix("```") {
            return inner.trim();
        }
    }
    if let Some(rest) = trimmed.strip_prefix("```") {
        if let Some(inner) = rest.strip_suffix("```") {
            return inner.trim();
        }
    }
    trimmed
}

/// 将视觉模型输出与说明书文本部件拓扑进行交叉核对。
pub fn cross_check_figure_with_spec(
    vision_json_or_raw: &str,
    spec_text: &str,
    vision_model: &str,
) -> FigureAnalysisResult {
    let cleaned = strip_markdown_json(vision_json_or_raw);
    let parsed: RawVisionPayload = match serde_json::from_str(cleaned) {
        Ok(p) => p,
        Err(_) => {
            return FigureAnalysisResult {
                status: "degraded".to_string(),
                vision_model_used: vision_model.to_string(),
                summary: format!(
                    "视觉模型返回非标准 JSON，已保留原始摘要：{}",
                    safe_truncate_chars(vision_json_or_raw.trim(), 300)
                ),
                parts: vec![],
                relations: vec![],
                interference_or_isolation_risks: vec![],
                consistent_pairs: vec![],
                discrepancies: vec![],
                stage_d_rebuttal_context: String::new(),
            };
        }
    };

    let text_parts = extract_part_designators(spec_text);
    let text_relations = extract_topology_relations(spec_text, &text_parts);

    let mut text_map: HashMap<String, String> = HashMap::new();
    for tp in &text_parts {
        text_map.insert(tp.number.clone(), tp.name.clone());
    }

    let mut fig_map: HashMap<String, &FigurePartItem> = HashMap::new();
    for fp in &parsed.parts {
        fig_map.insert(fp.number.trim().to_string(), fp);
    }

    let mut consistent_pairs = Vec::new();
    let mut discrepancies = Vec::new();

    for fp in &parsed.parts {
        let num = fp.number.trim();
        if num.is_empty() {
            continue;
        }
        if let Some(text_name) = text_map.get(num) {
            let fn_norm = fp.name.trim();
            if fn_norm == text_name.trim()
                || fn_norm.contains(text_name.trim())
                || text_name.trim().contains(fn_norm)
            {
                consistent_pairs.push(format!("{}: {}（图文一致）", num, text_name));
            } else {
                discrepancies.push(FigureDiscrepancy {
                    designator: num.to_string(),
                    kind: "name_mismatch".to_string(),
                    figure_desc: fn_norm.to_string(),
                    text_desc: text_name.clone(),
                    detail: format!(
                        "标号 {} 图文名称不一致：附图识别为「{}」，说明书称为「{}」",
                        num, fn_norm, text_name
                    ),
                });
            }
        } else {
            discrepancies.push(FigureDiscrepancy {
                designator: num.to_string(),
                kind: "missing_in_text".to_string(),
                figure_desc: format!("{}（{}）", fp.name, fp.morphology),
                text_desc: String::new(),
                detail: format!(
                    "标号 {}（{}）在附图中可见，但说明书文本未提取到对应标号定义",
                    num, fp.name
                ),
            });
        }
    }

    for tp in &text_parts {
        if !fig_map.contains_key(tp.number.trim()) {
            discrepancies.push(FigureDiscrepancy {
                designator: tp.number.clone(),
                kind: "missing_in_figure".to_string(),
                figure_desc: String::new(),
                text_desc: tp.name.clone(),
                detail: format!(
                    "标号 {}（{}）在说明书中出现，但本次附图视觉识别未检出",
                    tp.number, tp.name
                ),
            });
        }
    }

    for risk in &parsed.interference_or_isolation_risks {
        if !risk.trim().is_empty() {
            discrepancies.push(FigureDiscrepancy {
                designator: "拓扑/干涉".to_string(),
                kind: "topology_risk".to_string(),
                figure_desc: risk.clone(),
                text_desc: format!("文本共提取 {} 条连接关系", text_relations.len()),
                detail: risk.clone(),
            });
        }
    }

    let stage_d_rebuttal_context = build_stage_d_context(
        &parsed.summary,
        &parsed.relations,
        &parsed.interference_or_isolation_risks,
        &discrepancies,
    );

    FigureAnalysisResult {
        status: "ok".to_string(),
        vision_model_used: vision_model.to_string(),
        summary: parsed.summary,
        parts: parsed.parts,
        relations: parsed.relations,
        interference_or_isolation_risks: parsed.interference_or_isolation_risks,
        consistent_pairs,
        discrepancies,
        stage_d_rebuttal_context,
    }
}

fn build_stage_d_context(
    summary: &str,
    relations: &[FigureRelation],
    risks: &[String],
    discrepancies: &[FigureDiscrepancy],
) -> String {
    let mut lines = Vec::new();
    if !summary.trim().is_empty() {
        lines.push(format!("【附图视觉结构】{}", summary.trim()));
    }
    if !relations.is_empty() {
        let rel_str: Vec<String> = relations
            .iter()
            .take(6)
            .map(|r| {
                if r.drive_or_flow.is_empty() {
                    format!("{} -[{}]-> {}", r.part_a, r.relation, r.part_b)
                } else {
                    format!(
                        "{} -[{} / {}]-> {}",
                        r.part_a, r.relation, r.drive_or_flow, r.part_b
                    )
                }
            })
            .collect();
        lines.push(format!("【附图连接与流向】{}", rel_str.join("；")));
    }
    if !risks.is_empty() {
        lines.push(format!(
            "【附图运动干涉/介质未隔离隐患】{}",
            risks.join("；")
        ));
    }
    if !discrepancies.is_empty() {
        let diff_str: Vec<String> = discrepancies
            .iter()
            .take(5)
            .map(|d| d.detail.clone())
            .collect();
        lines.push(format!("【图文核对差异】{}", diff_str.join("；")));
    }
    lines.join("\n")
}

/// 无看图模型或视觉调用失败时的优雅降级结果（仅基于确定性文本拓扑提取）。
pub fn degraded_figure_analysis(spec_text: &str, reason: &str) -> FigureAnalysisResult {
    let text_parts = extract_part_designators(spec_text);
    let text_relations = extract_topology_relations(spec_text, &text_parts);
    FigureAnalysisResult {
        status: "degraded".to_string(),
        vision_model_used: "none (deterministic text fallback)".to_string(),
        summary: format!(
            "视觉分析已优雅降级（{}）。已从说明书文本确定性提取 {} 个部件标号与 {} 条连接关系。",
            reason,
            text_parts.len(),
            text_relations.len()
        ),
        parts: text_parts
            .iter()
            .map(|p| FigurePartItem {
                number: p.number.clone(),
                name: p.name.clone(),
                morphology: "源自说明书文本标号".to_string(),
            })
            .collect(),
        relations: text_relations
            .iter()
            .map(|r| FigureRelation {
                part_a: r.part_a.clone(),
                part_b: r.part_b.clone(),
                relation: r.relation_verb.clone(),
                drive_or_flow: String::new(),
            })
            .collect(),
        interference_or_isolation_risks: vec![],
        consistent_pairs: vec![],
        discrepancies: vec![],
        stage_d_rebuttal_context: String::new(),
    }
}

/// 执行专利附图视觉核对（自动处理无客户端/空图片/调用异常的优雅降级）。
pub async fn analyze_figure_with_vision(
    client: Option<&AiClient>,
    image_base64_urls: &[String],
    spec_text: &str,
    model_label: &str,
) -> FigureAnalysisResult {
    let Some(ai) = client else {
        return degraded_figure_analysis(
            spec_text,
            "未配置支持看图的多模态 AI 服务商（可在设置中配置商汤 SenseNova、小米 MiMo 或 Gemini）",
        );
    };
    if image_base64_urls.is_empty() {
        return degraded_figure_analysis(spec_text, "未提供附图图片");
    }

    let (system, prompt) = build_figure_vision_prompt(spec_text);
    match ai
        .chat_with_images(&system, &prompt, image_base64_urls, 0.2)
        .await
    {
        Ok(raw) => cross_check_figure_with_spec(&raw, spec_text, model_label),
        Err(e) => degraded_figure_analysis(spec_text, &format!("看图模型调用失败：{}", e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cross_check_detects_consistent_and_discrepancies() {
        let spec = "如图1所示，本装置包括固定机架1、直线电缸2和隔离阀芯3。直线电缸2连接固定机架1并驱动隔离阀芯3。";
        let mock_vision_json = r#"```json
{
  "summary": "图1展示机架1支撑电缸2并驱动阀芯3，另见未标注的旁通流道4",
  "parts": [
    {"number": "1", "name": "固定机架", "morphology": "底部矩形底座"},
    {"number": "2", "name": "旋转电机", "morphology": "顶部圆形驱动单元"},
    {"number": "4", "name": "旁通流道", "morphology": "右侧连通槽"}
  ],
  "relations": [
    {"part_a": "2", "part_b": "3", "relation": "驱动", "drive_or_flow": "向下往复"}
  ],
  "interference_or_isolation_risks": [
    "对比文件附图中阀芯与旁通流道直接连通，存在介质未隔离风险"
  ]
}
```"#;

        let res = cross_check_figure_with_spec(mock_vision_json, spec, "sensenova-6.8-flash-lite");
        assert_eq!(res.status, "ok");
        assert_eq!(res.vision_model_used, "sensenova-6.8-flash-lite");
        assert!(res.consistent_pairs.iter().any(|s| s.starts_with("1:")));
        assert!(res
            .discrepancies
            .iter()
            .any(|d| d.designator == "2" && d.kind == "name_mismatch"));
        assert!(res
            .discrepancies
            .iter()
            .any(|d| d.designator == "4" && d.kind == "missing_in_text"));
        assert!(res
            .discrepancies
            .iter()
            .any(|d| d.designator == "3" && d.kind == "missing_in_figure"));
        assert!(res.stage_d_rebuttal_context.contains("介质未隔离风险"));
    }

    #[tokio::test]
    async fn analyze_figure_degrades_gracefully_without_vision_client() {
        let spec = "包括机架1和阀芯2，机架1固定连接阀芯2。";
        let res = analyze_figure_with_vision(None, &[], spec, "none").await;
        assert_eq!(res.status, "degraded");
        assert!(!res.parts.is_empty());
        assert!(res.summary.contains("优雅降级"));
    }
}
