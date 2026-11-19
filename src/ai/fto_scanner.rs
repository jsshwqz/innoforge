//! 批量 FTO（自由实施）侵权风险扫描与自动规避设计（Design-Around）引擎。
//!
//! 遵循“全面覆盖原则”与“等同原则”，并严格区分专利法律状态：
//! - 已失效/撤回/终止/无效专利直接标绿（可自由实施）；
//! - 全部覆盖有效独立权利要求特征 -> 红色高危 (`high_red`)；
//! - 仅 1 项特征为近似等同替换 -> 黄色等同预警 (`warn_yellow`)；
//! - 缺少至少 1 项必要技术特征 -> 绿色安全 (`safe_green`)。

use crate::ai::oa_matrix::split_atomic_features;
use crate::patent::{FeatureCard, Patent};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesignAroundOption {
    pub title: String,
    pub approach: String,
    pub bom_cost_impact: String,
    pub performance_impact: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FtoHit {
    pub patent_number: String,
    pub title: String,
    pub legal_status: String,
    /// `"high_red"` | `"warn_yellow"` | `"safe_green"`
    pub risk_level: String,
    pub matched_features: Vec<String>,
    pub equivalent_features: Vec<String>,
    pub missing_features: Vec<String>,
    pub rationale: String,
    pub design_arounds: Vec<DesignAroundOption>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FtoScanReport {
    pub status: String,
    pub idea_id: String,
    pub scanned_count: usize,
    pub high_risk_count: usize,
    pub warn_count: usize,
    pub safe_count: usize,
    pub hits: Vec<FtoHit>,
    pub summary: String,
}

/// 判断专利法律状态是否为“已失效/可自由实施”。
pub fn is_patent_inactive(legal_status: &str) -> bool {
    if matches!(
        crate::search::providers::epo_ops::LegalStatus::from_str_lossy(legal_status),
        crate::search::providers::epo_ops::LegalStatus::Expired
            | crate::search::providers::epo_ops::LegalStatus::Rejected
    ) {
        return true;
    }
    let lower = legal_status.trim().to_ascii_lowercase();
    if lower.is_empty() {
        return false;
    }
    const INACTIVE_MARKERS: &[&str] = &[
        "失效",
        "撤回",
        "终止",
        "无效",
        "驳回",
        "视撤",
        "过期",
        "期满",
        "expired",
        "expiration",
        "withdrawn",
        "rejected",
        "invalid",
        "lapsed",
        "revoked",
        "abandoned",
        "cease",
    ];
    INACTIVE_MARKERS.iter().any(|m| lower.contains(m))
}

/// 提取权利要求 1（独立权利要求）片段。
pub fn extract_claim_one(claims: &str) -> String {
    let trimmed = claims.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let mut lines = Vec::new();
    for line in trimmed.lines() {
        let t = line.trim();
        if !lines.is_empty()
            && (t.starts_with("2.") || t.starts_with("2、") || t.starts_with("根据权利要求"))
        {
            break;
        }
        lines.push(t);
    }
    lines.join("\n")
}

fn cjk_bigrams(text: &str) -> HashSet<String> {
    let chars: Vec<char> = text
        .chars()
        .filter(|c| !c.is_whitespace() && !c.is_ascii_punctuation())
        .collect();
    let mut set = HashSet::new();
    for w in chars.windows(2) {
        set.insert(w.iter().collect());
    }
    set
}

fn feature_overlap_ratio(feature: &str, solution_bigrams: &HashSet<String>) -> f64 {
    let f_bg = cjk_bigrams(feature);
    if f_bg.is_empty() {
        return 0.0;
    }
    let hits = f_bg
        .iter()
        .filter(|g| solution_bigrams.contains(*g))
        .count();
    (hits as f64) / (f_bg.len() as f64)
}

/// 为标红/标黄碰撞专利生成 3 套可落地的工程规避替代方案。
pub fn build_design_around_options(
    patent_number: &str,
    key_features: &[String],
) -> Vec<DesignAroundOption> {
    let target = key_features
        .first()
        .cloned()
        .unwrap_or_else(|| "核心耦合机构".to_string());
    vec![
        DesignAroundOption {
            title: format!("方案 A（传动机构替换 — 规避 {patent_number}）"),
            approach: format!(
                "取消「{target}」中的刚性直驱结构，改用柔性联轴或凸轮-连杆分步传动，使技术手段与效果产生实质性差异。"
            ),
            bom_cost_impact: "中等（增加标准连杆/联轴件约 +5%~8%）".to_string(),
            performance_impact: "响应略柔顺，抗冲击与寿命提升".to_string(),
        },
        DesignAroundOption {
            title: format!("方案 B（非接触隔离改型 — 规避 {patent_number}）"),
            approach: format!(
                "将「{target}」涉及的接触式密封/导向改为磁耦合或气隙非接触隔离，消除权利要求中的必要接触限定。"
            ),
            bom_cost_impact: "略增（磁耦合组件成本 +10%~15%）".to_string(),
            performance_impact: "零磨损、免维护，显著降低泄漏风险".to_string(),
        },
        DesignAroundOption {
            title: format!("方案 C（控制闭环条件重构 — 规避 {patent_number}）"),
            approach: format!(
                "修改控制算法触发条件与传感器反馈路径，采用分时双闭环前馈补偿替代「{target}」的单一阈值反馈。"
            ),
            bom_cost_impact: "极低（仅软件/ST控制逻辑调整，BOM 零新增）".to_string(),
            performance_impact: "稳态精度相当，超调量可进一步压缩".to_string(),
        },
    ]
}

/// 对单个在先专利执行 FTO 全面覆盖/等同判定。
pub fn evaluate_patent_fto(patent: &Patent, solution_text: &str) -> FtoHit {
    let legal = if patent.legal_status.trim().is_empty() {
        "有效/状态未知".to_string()
    } else {
        patent.legal_status.clone()
    };

    if is_patent_inactive(&legal) {
        return FtoHit {
            patent_number: patent.patent_number.clone(),
            title: patent.title.clone(),
            legal_status: legal,
            risk_level: "safe_green".to_string(),
            matched_features: vec![],
            equivalent_features: vec![],
            missing_features: vec![],
            rationale: "该对比专利法律状态为已失效/撤回，可自由实施。".to_string(),
            design_arounds: vec![],
        };
    }

    let claim1 = extract_claim_one(&patent.claims);
    let source_text = if claim1.trim().is_empty() {
        format!("{} {}", patent.title, patent.abstract_text)
    } else {
        claim1
    };
    let features = split_atomic_features(&source_text);
    if features.is_empty() {
        return FtoHit {
            patent_number: patent.patent_number.clone(),
            title: patent.title.clone(),
            legal_status: legal,
            risk_level: "safe_green".to_string(),
            matched_features: vec![],
            equivalent_features: vec![],
            missing_features: vec![],
            rationale: "未提取到明确权利要求特征，暂判定低风险（建议人工复核原文）。".to_string(),
            design_arounds: vec![],
        };
    }

    let sol_bg = cjk_bigrams(solution_text);
    let mut matched = Vec::new();
    let mut equivalent = Vec::new();
    let mut missing = Vec::new();

    for feat in &features {
        let ratio = feature_overlap_ratio(feat, &sol_bg);
        if ratio >= 0.65 {
            matched.push(feat.clone());
        } else if ratio >= 0.42 {
            equivalent.push(feat.clone());
        } else {
            missing.push(feat.clone());
        }
    }

    let (risk_level, rationale, design_arounds) = if missing.is_empty() && equivalent.is_empty() {
        (
            "high_red".to_string(),
            format!(
                "全面覆盖风险：我方方案命中该专利全部 {} 项独立权利要求特征。",
                features.len()
            ),
            build_design_around_options(&patent.patent_number, &matched),
        )
    } else if missing.is_empty() && equivalent.len() == 1 {
        (
            "warn_yellow".to_string(),
            format!(
                "等同侵权预警：命中 {} 项特征，且有 1 项特征（「{}」）存在手段/功能/效果近似替换风险。",
                matched.len(),
                equivalent[0]
            ),
            build_design_around_options(&patent.patent_number, &equivalent),
        )
    } else {
        (
            "safe_green".to_string(),
            format!(
                "绿色安全：缺少 {} 项必要技术特征（不构成全面覆盖）。",
                missing.len().max(1)
            ),
            vec![],
        )
    };

    FtoHit {
        patent_number: patent.patent_number.clone(),
        title: patent.title.clone(),
        legal_status: legal,
        risk_level,
        matched_features: matched,
        equivalent_features: equivalent,
        missing_features: missing,
        rationale,
        design_arounds,
    }
}

/// 批量扫描项目与本地/检索专利库生成完整 FTO 报告。
pub fn run_batch_fto_scan(
    idea_id: &str,
    idea_desc: &str,
    cards: &[FeatureCard],
    patents: &[Patent],
) -> FtoScanReport {
    let mut solution_buf = String::from(idea_desc);
    for c in cards {
        solution_buf.push('\n');
        solution_buf.push_str(&c.title);
        solution_buf.push(' ');
        solution_buf.push_str(&c.description);
    }

    let mut hits = Vec::with_capacity(patents.len());
    let mut high_risk_count = 0usize;
    let mut warn_count = 0usize;
    let mut safe_count = 0usize;

    for p in patents {
        let hit = evaluate_patent_fto(p, &solution_buf);
        match hit.risk_level.as_str() {
            "high_red" => high_risk_count += 1,
            "warn_yellow" => warn_count += 1,
            _ => safe_count += 1,
        }
        hits.push(hit);
    }

    // 高危置前
    hits.sort_by_key(|h| match h.risk_level.as_str() {
        "high_red" => 0,
        "warn_yellow" => 1,
        _ => 2,
    });

    let scanned_count = hits.len();
    FtoScanReport {
        status: "ok".to_string(),
        idea_id: idea_id.to_string(),
        scanned_count,
        high_risk_count,
        warn_count,
        safe_count,
        hits,
        summary: format!(
            "共扫描 {} 篇同领域专利：红色高危 {} 篇、黄色等同预警 {} 篇、绿色安全 {} 篇。",
            scanned_count, high_risk_count, warn_count, safe_count
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fto_judges_missing_green_full_red_and_inactive_safe() {
        let active_full = Patent {
            patent_number: "CN111111111B".into(),
            title: "双波纹管隔离电磁阀".into(),
            claims: "1. 一种电磁阀，包括电磁驱动线圈；其特征在于：采用双层波纹管组件将流体介质与动铁芯完全隔离。".into(),
            legal_status: "有效授权".into(),
            ..Default::default()
        };
        let active_missing = Patent {
            patent_number: "CN222222222B".into(),
            title: "超声波流量阀".into(),
            claims: "1. 一种流量阀，包括阀体；其特征在于：安装超声波相控阵探头和温度补偿晶体。"
                .into(),
            legal_status: "有效授权".into(),
            ..Default::default()
        };
        let inactive_patent = Patent {
            patent_number: "CN333333333B".into(),
            title: "过期电磁阀".into(),
            claims: "1. 一种电磁阀，包括电磁驱动线圈；其特征在于：采用双层波纹管组件将流体介质与动铁芯完全隔离。".into(),
            legal_status: "已失效（未缴年费）".into(),
            ..Default::default()
        };

        let solution = "本方案为一种电磁阀，包括电磁驱动线圈，并采用双层波纹管组件将流体介质与动铁芯完全隔离。";
        let report = run_batch_fto_scan(
            "idea-fto-1",
            solution,
            &[],
            &[active_full, active_missing, inactive_patent],
        );

        assert_eq!(report.scanned_count, 3);
        assert_eq!(report.high_risk_count, 1);
        assert_eq!(report.safe_count, 2);

        let red = report
            .hits
            .iter()
            .find(|h| h.patent_number == "CN111111111B")
            .expect("red hit");
        assert_eq!(red.risk_level, "high_red");
        assert_eq!(red.design_arounds.len(), 3);

        let missing_safe = report
            .hits
            .iter()
            .find(|h| h.patent_number == "CN222222222B")
            .expect("missing hit");
        assert_eq!(missing_safe.risk_level, "safe_green");

        let expired_safe = report
            .hits
            .iter()
            .find(|h| h.patent_number == "CN333333333B")
            .expect("inactive hit");
        assert_eq!(expired_safe.risk_level, "safe_green");
        assert!(expired_safe.rationale.contains("已失效"));
    }

    #[test]
    fn fto_treats_google_patents_expired_and_withdrawn_variants_as_safe_green() {
        for status in ["Expired", "Withdrawn", "Rejected", "Abandoned", "Lapsed"] {
            assert!(
                is_patent_inactive(status),
                "{status} should be treated as inactive"
            );
            let p = Patent {
                patent_number: format!("US-{status}"),
                title: "电磁阀".into(),
                claims: "1. 一种电磁阀，包括电磁驱动线圈。".into(),
                legal_status: status.into(),
                ..Default::default()
            };
            let hit = evaluate_patent_fto(&p, "一种电磁阀，包括电磁驱动线圈。");
            assert_eq!(hit.risk_level, "safe_green");
        }
    }
}
