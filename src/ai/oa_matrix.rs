//! 多对比文件（D1/D2/D3）三维特征对比矩阵引擎。
//!
//! 将本申请权利要求拆解为原子技术特征，定位本案说明书段落锚点（A33 合规），
//! 并分别在 D1、D2、D3 原文中匹配公开状态、段落号及结合障碍。

use crate::ai::case_input::ReferenceDocument;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MatrixCell {
    /// `"公开"` | `"部分公开"` | `"未公开"` | `"未提供"`
    pub status: String,
    pub anchor: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MatrixRow {
    pub feature_id: String,
    pub feature_text: String,
    pub app_anchor: String,
    pub d1: MatrixCell,
    pub d2: MatrixCell,
    pub d3: MatrixCell,
    pub combination_barrier: String,
    pub is_distinguishing: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiDocFeatureMatrix {
    pub rows: Vec<MatrixRow>,
    pub distinguishing_count: usize,
    pub summary: String,
}

/// 判断拆解片段是否为非技术特征的章节标题或附图标记说明行（避免把“附图标记说明：1-阀体”误列为 F1）
fn is_non_feature_heading_or_legend(seg: &str) -> bool {
    let trimmed = seg.trim();
    if trimmed.is_empty() {
        return true;
    }
    const HEADING_PREFIXES: &[&str] = &[
        "附图标记说明",
        "附图标号说明",
        "附图说明",
        "标号说明",
        "具体实施方式",
        "背景技术",
        "技术领域",
        "发明内容",
        "实用新型内容",
        "权利要求书",
        "摘要",
    ];
    if HEADING_PREFIXES.iter().any(|p| {
        trimmed == *p
            || trimmed.starts_with(&format!("{p}："))
            || trimmed.starts_with(&format!("{p}:"))
    }) {
        return true;
    }
    // 纯“1-阀体、2-线圈、3-壳体”型标号图例列表
    let legend_parts: Vec<&str> = trimmed
        .split(['、', '，', ',', ' '])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    if legend_parts.len() >= 2
        && legend_parts.iter().all(|part| {
            part.split_once('-')
                .or_else(|| part.split_once('—'))
                .or_else(|| part.split_once('－'))
                .is_some_and(|(num, name)| {
                    !num.is_empty()
                        && num.chars().all(|c| c.is_ascii_digit())
                        && !name.trim().is_empty()
                })
        })
    {
        return true;
    }
    false
}

/// 从权利要求文本拆解原子技术特征（按分号/句号/“其特征在于”切分并去重，过滤附图标记说明等非特征行）。
pub fn split_atomic_features(claims_text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for raw_line in claims_text.lines() {
        let line = raw_line.trim();
        if line.is_empty() || is_non_feature_heading_or_legend(line) {
            continue;
        }
        let replaced = line
            .replace("其特征在于：", "；")
            .replace("其特征在于:", "；")
            .replace("其特征在于", "；")
            .replace(['。', ';'], "；");
        for seg in replaced.split('；') {
            if is_non_feature_heading_or_legend(seg) {
                continue;
            }
            let s = seg
                .trim()
                .trim_start_matches(|c: char| {
                    c.is_ascii_digit() || c == '.' || c == '、' || c == ' '
                })
                .trim_end_matches(['，', ',', '、', '：', ':']);
            let clean = s.trim();
            if clean.chars().count() >= 6
                && !is_non_feature_heading_or_legend(clean)
                && seen.insert(clean.to_string())
            {
                out.push(clean.to_string());
            }
        }
    }
    out
}

/// 在说明书原文中查找最匹配的段落锚点（如 `[0024]`），严守 A33 锚点要求。
pub fn find_spec_anchor(feature: &str, spec_text: &str) -> String {
    let tokens = extract_cjk_bigrams_or_words(feature);
    if tokens.is_empty() || spec_text.trim().is_empty() {
        return String::new();
    }

    let mut best_anchor = String::new();
    let mut best_hits = 0usize;
    let mut current_para = String::new();

    for line in spec_text.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if let Some(para_tag) = extract_leading_para_tag(t) {
            current_para = para_tag;
        }
        let hits = tokens.iter().filter(|tok| t.contains(tok.as_str())).count();
        if hits > best_hits && hits >= 2 {
            best_hits = hits;
            best_anchor = if current_para.is_empty() {
                "说明书正文".to_string()
            } else {
                current_para.clone()
            };
        }
    }
    best_anchor
}

fn extract_leading_para_tag(line: &str) -> Option<String> {
    let rest = line.strip_prefix('[')?;
    let (inner, _) = rest.split_once(']')?;
    if !inner.is_empty() && inner.chars().all(|c| c.is_ascii_digit()) {
        Some(format!("[{inner}]"))
    } else {
        None
    }
}

fn extract_cjk_bigrams_or_words(text: &str) -> Vec<String> {
    let chars: Vec<char> = text
        .chars()
        .filter(|c| !c.is_whitespace() && !is_punct(*c))
        .collect();
    let mut toks = Vec::new();
    let mut seen = HashSet::new();
    for w in chars.windows(2) {
        let s: String = w.iter().collect();
        if seen.insert(s.clone()) {
            toks.push(s);
        }
    }
    toks
}

fn is_punct(c: char) -> bool {
    matches!(
        c,
        '，' | '。' | '；' | '：' | '、' | '（' | '）' | ',' | '.' | ';' | ':' | '(' | ')'
    )
}

fn evaluate_doc_cell(feature: &str, doc: Option<&ReferenceDocument>) -> MatrixCell {
    let Some(d) = doc else {
        return MatrixCell {
            status: "未提供".to_string(),
            anchor: String::new(),
        };
    };
    let tokens = extract_cjk_bigrams_or_words(feature);
    if tokens.is_empty() || d.full_text.trim().is_empty() {
        return MatrixCell {
            status: "未公开".to_string(),
            anchor: String::new(),
        };
    }

    let mut best_ratio = 0.0_f64;
    let mut best_anchor = String::new();
    let mut current_para = String::new();

    for line in d.full_text.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if let Some(p) = extract_leading_para_tag(t) {
            current_para = p;
        }
        let hits = tokens.iter().filter(|tok| t.contains(tok.as_str())).count();
        let ratio = (hits as f64) / (tokens.len() as f64);
        if ratio > best_ratio {
            best_ratio = ratio;
            best_anchor = current_para.clone();
        }
    }

    let status = if best_ratio >= 0.65 {
        "公开"
    } else if best_ratio >= 0.35 {
        "部分公开"
    } else {
        "未公开"
    };

    MatrixCell {
        status: status.to_string(),
        anchor: if status == "未公开" {
            String::new()
        } else if best_anchor.is_empty() {
            d.label.clone()
        } else {
            format!("{} {}", d.label, best_anchor)
        },
    }
}

/// 构建三维对比矩阵（本案原子特征 × D1 × D2 × D3）。
pub fn build_multi_doc_feature_matrix(
    claims_text: &str,
    spec_text: &str,
    refs: &[ReferenceDocument],
) -> MultiDocFeatureMatrix {
    let features = split_atomic_features(claims_text);
    let d1_ref = refs.first();
    let d2_ref = refs.get(1);
    let d3_ref = refs.get(2);

    let mut rows = Vec::with_capacity(features.len());
    let mut distinguishing_count = 0usize;

    for (idx, feat) in features.into_iter().enumerate() {
        let app_anchor = find_spec_anchor(&feat, spec_text);
        let d1 = evaluate_doc_cell(&feat, d1_ref);
        let d2 = evaluate_doc_cell(&feat, d2_ref);
        let d3 = evaluate_doc_cell(&feat, d3_ref);

        let all_unexposed = [d1.status.as_str(), d2.status.as_str(), d3.status.as_str()]
            .iter()
            .all(|s| *s == "未公开" || *s == "未提供");

        let barrier = if all_unexposed {
            "D1/D2/D3均未公开该技术手段，存在结合障碍与机理差异".to_string()
        } else if d1.status == "公开" && (d2.status == "部分公开" || d3.status == "部分公开")
        {
            "需核查结构连接或工作介质是否存在反向教导".to_string()
        } else {
            "常规技术比对项".to_string()
        };

        if all_unexposed {
            distinguishing_count += 1;
        }

        rows.push(MatrixRow {
            feature_id: format!("F{}", idx + 1),
            feature_text: feat,
            app_anchor,
            d1,
            d2,
            d3,
            combination_barrier: barrier,
            is_distinguishing: all_unexposed,
        });
    }

    let total = rows.len();
    MultiDocFeatureMatrix {
        rows,
        distinguishing_count,
        summary: format!(
            "共拆解 {} 项原子技术特征，锁定 {} 项潜在区别特征（未在 D1/D2/D3 公开）。",
            total, distinguishing_count
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matrix_identifies_distinguishing_features_and_anchors() {
        let claims = "1. 一种双波纹管隔离电磁阀，包括电磁驱动线圈；其特征在于：波纹管组件将流体介质与动铁芯完全隔离。";
        let spec = "[0012] 电磁驱动线圈固定于阀体顶部。\n[0024] 波纹管组件将流体介质与动铁芯完全隔离，防止腐蚀。";
        let d1 = ReferenceDocument {
            label: "D1".into(),
            publication_number: "CN10001A".into(),
            full_text: "[0008] 公开了一种电磁驱动线圈固定在阀体顶部。".into(),
            extraction_method: None,
            page_count: None,
            technical_field: None,
        };
        let matrix = build_multi_doc_feature_matrix(claims, spec, &[d1]);
        assert!(matrix.rows.len() >= 2);
        assert!(matrix.distinguishing_count >= 1);
        let iso_row = matrix
            .rows
            .iter()
            .find(|r| r.feature_text.contains("完全隔离"))
            .expect("isolation feature");
        assert!(iso_row.is_distinguishing);
        assert_eq!(iso_row.app_anchor, "[0024]");
        assert_eq!(iso_row.d1.status, "未公开");
    }

    #[test]
    fn split_atomic_features_skips_figure_legend_and_section_headers() {
        let mixed = "权利要求书\n附图标记说明：1-阀体、2-动铁芯、3-波纹管\n1-阀体、2-动铁芯、3-波纹管\n1. 一种双波纹管隔离电磁阀，包括电磁驱动线圈；其特征在于：波纹管组件将流体介质与动铁芯完全隔离。";
        let feats = split_atomic_features(mixed);
        assert!(
            !feats.iter().any(|f| f.contains("附图标记说明")),
            "不应将附图标记说明当成原子技术特征: {:?}",
            feats
        );
        assert!(
            !feats.iter().any(|f| f.contains("1-阀体")),
            "不应将纯标号图例列表当成原子技术特征: {:?}",
            feats
        );
        assert!(
            feats.iter().any(|f| f.contains("波纹管组件")),
            "应正常保留真实权利要求技术特征: {:?}",
            feats
        );
    }
}
