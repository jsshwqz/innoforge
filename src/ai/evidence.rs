//! 证据自动收集与整理 / Evidence Collection and Organization
//!
//! 从专利全文、对比文献、OA 文本中自动提取证据段落，
//! 按类型分类整理，生成证据清单。

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::client::AiClient;

/// 证据类型 / Evidence type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum EvidenceType {
    /// 说明书支持段落
    SpecSupport,
    /// 对比文件区别点
    Distinction,
    /// 技术效果证据
    TechnicalEffect,
    /// 现有技术缺陷
    PriorArtDeficiency,
    /// 实验数据
    ExperimentalData,
}

impl EvidenceType {
    /// 返回用于前端分组的字符串 key
    pub fn as_key(&self) -> &'static str {
        match self {
            Self::SpecSupport => "specSupport",
            Self::Distinction => "distinction",
            Self::TechnicalEffect => "techEffect",
            Self::PriorArtDeficiency => "priorArt",
            Self::ExperimentalData => "experimental",
        }
    }

    /// 从字符串 key 解析
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "specSupport" => Some(Self::SpecSupport),
            "distinction" => Some(Self::Distinction),
            "techEffect" => Some(Self::TechnicalEffect),
            "priorArt" => Some(Self::PriorArtDeficiency),
            "experimental" => Some(Self::ExperimentalData),
            _ => None,
        }
    }
}

/// 证据条目 / Evidence item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    /// 证据类型
    pub evidence_type: EvidenceType,
    /// 来源（说明书第X段 / D1第Y页 / OA第Z段）
    pub source: String,
    /// 证据内容
    pub content: String,
    /// 相关性 0-1
    pub relevance: f32,
    /// 是否已用于答复
    pub used_in_response: bool,
}

/// 构建证据收集 prompt（系统提示, 用户提示）
fn build_evidence_collect_prompt(patent: &str, refs: &str, oa: &str) -> (String, String) {
    let system = "你是一位专利证据收集专家，精通中国专利法及审查指南。\n\
        你的任务是从以下材料中提取所有可用于答复审查意见的证据段落，按类型分类整理。\n\n\
        ## 证据类型\n\
        1. **specSupport**（说明书支持）：说明书/附图中哪些段落支持权利要求中的技术特征？\n\
        2. **distinction**（区别点）：本申请与对比文件在技术上的具体区别是什么？\n\
        3. **techEffect**（技术效果）：说明书提到的效果、数据、优势有哪些？\n\
        4. **priorArt**（现有技术缺陷）：对比文件不能解决什么问题？有什么不足？\n\
        5. **experimental**（实验数据）：实施例、对比实验、测试数据有哪些？\n\n\
        ## 输出格式（严格按此 JSON 结构，不要加 markdown 代码块标记）\n\
        {\n\
          \"evidence\": [\n\
            {\n\
              \"evidenceType\": \"specSupport\",\n\
              \"source\": \"说明书第3段\",\n\
              \"content\": \"...\",\n\
              \"relevance\": 0.85\n\
            },\n\
            ...\n\
          ],\n\
          \"summary\": \"共收集到 N 条证据，其中说明书支持 X 条、区别点 Y 条...\"\n\
        }\n\n\
        ## 要求\n\
        - 每条证据必须标注来源（说明书第X段 / D1第Y页 / OA第Z段）\n\
        - relevance 取值 0-1，表示该证据对答复的相关性程度\n\
        - 证据内容必须来自材料原文，不得编造\n\
        - 如果某类型无证据，该类型不出现即可\n\
        - 请严格按 JSON 格式输出，不要加任何其他文字\n\n\
        ## 事实纪律\n\
        - 只能依据上方提供的材料作答，禁止引入材料之外的信息\n\
        - 材料中没有的信息，必须标注【材料未提供】\n\
        - 任何数字、日期、百分比必须来自材料原文";

    let user = format!("## 我的专利\n{patent}\n\n## 对比文献\n{refs}\n\n## 审查意见\n{oa}");

    (system.to_string(), user)
}

/// 从 AI 返回文本中解析证据列表
fn parse_evidence_response(raw: &str) -> (Vec<Evidence>, String) {
    // 尝试直接解析 JSON
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(raw) {
        return extract_evidence_from_json(&v);
    }

    // 尝试从 markdown 代码块中提取 JSON
    if let Some(json_str) = extract_json_from_markdown(raw) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&json_str) {
            return extract_evidence_from_json(&v);
        }
    }

    // 解析失败，返回原始文本作为 summary
    (Vec::new(), format!("证据解析失败，AI 原始返回：\n{}", raw))
}

/// 从 JSON Value 中提取证据列表
fn extract_evidence_from_json(v: &serde_json::Value) -> (Vec<Evidence>, String) {
    let summary = v["summary"].as_str().unwrap_or("").to_string();

    let mut items = Vec::new();
    if let Some(arr) = v["evidence"].as_array() {
        for item in arr {
            let type_key = item["evidenceType"]
                .as_str()
                .or_else(|| item["evidence_type"].as_str())
                .unwrap_or("specSupport");
            let evidence_type =
                EvidenceType::from_key(type_key).unwrap_or(EvidenceType::SpecSupport);

            let source = item["source"].as_str().unwrap_or("未知来源").to_string();

            let content = item["content"].as_str().unwrap_or("").to_string();

            let relevance = item["relevance"]
                .as_f64()
                .map(|f| f as f32)
                .unwrap_or(0.5)
                .clamp(0.0, 1.0);

            if !content.is_empty() {
                items.push(Evidence {
                    evidence_type,
                    source,
                    content,
                    relevance,
                    used_in_response: false,
                });
            }
        }
    }

    let summary = if summary.is_empty() {
        format!("共收集到 {} 条证据", items.len())
    } else {
        summary
    };

    (items, summary)
}

/// 从 markdown 代码块中提取 JSON 文本
fn extract_json_from_markdown(raw: &str) -> Option<String> {
    let start = raw.find("```")?;
    let after_start = &raw[start + 3..];
    // 跳过语言标识行（如 json）
    let code_start = after_start.find('\n')?;
    let code = &after_start[code_start + 1..];
    let end = code.rfind("```")?;
    Some(code[..end].trim().to_string())
}

/// 收集证据 / Collect evidence
///
/// 从专利全文、对比文献、OA 文本中自动提取证据段落。
pub async fn collect_evidence(
    ai: &AiClient,
    patent: &str,
    refs: &str,
    oa: &str,
) -> Result<(Vec<Evidence>, String)> {
    let (system, user) = build_evidence_collect_prompt(patent, refs, oa);

    let raw = ai.chat_with_system(&system, &user, 0.3).await?;

    let (evidence, summary) = parse_evidence_response(&raw);

    Ok((evidence, summary))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_evidence_type_as_key() {
        assert_eq!(EvidenceType::SpecSupport.as_key(), "specSupport");
        assert_eq!(EvidenceType::Distinction.as_key(), "distinction");
        assert_eq!(EvidenceType::TechnicalEffect.as_key(), "techEffect");
        assert_eq!(EvidenceType::PriorArtDeficiency.as_key(), "priorArt");
        assert_eq!(EvidenceType::ExperimentalData.as_key(), "experimental");
    }

    #[test]
    fn test_evidence_type_from_key() {
        assert_eq!(
            EvidenceType::from_key("specSupport"),
            Some(EvidenceType::SpecSupport)
        );
        assert_eq!(EvidenceType::from_key("unknown"), None);
    }

    #[test]
    fn test_parse_evidence_response_valid_json() {
        let raw = r#"{
            "evidence": [
                {
                    "evidenceType": "specSupport",
                    "source": "说明书第3段",
                    "content": "本发明采用辅助火焰",
                    "relevance": 0.9
                },
                {
                    "evidenceType": "distinction",
                    "source": "D1第5页",
                    "content": "D1未公开辅助火焰结构",
                    "relevance": 0.8
                }
            ],
            "summary": "共收集到 2 条证据"
        }"#;

        let (items, summary) = parse_evidence_response(raw);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].evidence_type, EvidenceType::SpecSupport);
        assert_eq!(items[0].source, "说明书第3段");
        assert!((items[0].relevance - 0.9).abs() < 0.01);
        assert!(!items[0].used_in_response);
        assert_eq!(items[1].evidence_type, EvidenceType::Distinction);
        assert_eq!(summary, "共收集到 2 条证据");
    }

    #[test]
    fn test_parse_evidence_response_markdown_wrapped() {
        let raw = r#"```json
        {
            "evidence": [
                {
                    "evidenceType": "techEffect",
                    "source": "说明书第7段",
                    "content": "燃烧效率提升30%",
                    "relevance": 0.95
                }
            ],
            "summary": "共1条"
        }
        ```"#;

        let (items, summary) = parse_evidence_response(raw);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].evidence_type, EvidenceType::TechnicalEffect);
        assert_eq!(summary, "共1条");
    }

    #[test]
    fn test_parse_evidence_response_invalid() {
        let raw = "这不是JSON";
        let (items, summary) = parse_evidence_response(raw);
        assert!(items.is_empty());
        assert!(summary.contains("证据解析失败"));
    }

    #[test]
    fn test_parse_evidence_response_empty_content_skipped() {
        let raw = r#"{
            "evidence": [
                {
                    "evidenceType": "specSupport",
                    "source": "说明书第1段",
                    "content": "",
                    "relevance": 0.5
                },
                {
                    "evidenceType": "distinction",
                    "source": "D1第2页",
                    "content": "有内容",
                    "relevance": 0.7
                }
            ],
            "summary": "测试"
        }"#;

        let (items, _) = parse_evidence_response(raw);
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn test_parse_evidence_response_relevance_clamped() {
        let raw = r#"{
            "evidence": [
                {
                    "evidenceType": "specSupport",
                    "source": "测试",
                    "content": "内容",
                    "relevance": 1.5
                }
            ],
            "summary": ""
        }"#;

        let (items, summary) = parse_evidence_response(raw);
        assert_eq!(items.len(), 1);
        assert!((items[0].relevance - 1.0).abs() < 0.01);
        assert!(summary.contains("1 条证据"));
    }

    #[test]
    fn test_parse_evidence_response_default_relevance() {
        let raw = r#"{
            "evidence": [
                {
                    "evidenceType": "experimental",
                    "source": "实施例1",
                    "content": "实验数据"
                }
            ],
            "summary": ""
        }"#;

        let (items, _) = parse_evidence_response(raw);
        assert_eq!(items.len(), 1);
        assert!((items[0].relevance - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_build_evidence_collect_prompt() {
        let (sys, user) = build_evidence_collect_prompt("专利文本", "对比文献", "OA文本");
        assert!(sys.contains("证据收集专家"));
        assert!(sys.contains("specSupport"));
        assert!(sys.contains("distinction"));
        assert!(user.contains("专利文本"));
        assert!(user.contains("对比文献"));
        assert!(user.contains("OA文本"));
    }

    #[test]
    fn test_extract_json_from_markdown() {
        let raw = "```json\n{\"key\": \"value\"}\n```";
        let result = extract_json_from_markdown(raw);
        assert!(result.is_some());
        assert!(result.unwrap().contains("key"));
    }

    #[test]
    fn test_extract_json_from_markdown_no_block() {
        let raw = "no code block here";
        assert!(extract_json_from_markdown(raw).is_none());
    }

    #[test]
    fn test_evidence_serialize_deserialize() {
        let e = Evidence {
            evidence_type: EvidenceType::SpecSupport,
            source: "测试".to_string(),
            content: "内容".to_string(),
            relevance: 0.8,
            used_in_response: false,
        };
        let json = serde_json::to_string(&e).unwrap();
        let de: Evidence = serde_json::from_str(&json).unwrap();
        assert_eq!(de.source, "测试");
        assert_eq!(de.evidence_type, EvidenceType::SpecSupport);
    }
}
