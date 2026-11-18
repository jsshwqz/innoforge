//! 部件标号提取与拓扑连接关系校验模块。
//!
//! 从本申请与对比文件原文中确定性提取机械/电气部件标号（图例式与行内式），
//! 构建部件间拓扑连接边，并对比识别对比文件未公开的部件及连接关系，
//! 生成符合 `output_guard` 规范的专利答复反驳句。

use serde::{Deserialize, Serialize};

/// 部件标号实体
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PartDesignator {
    pub number: String,
    pub name: String,
}

/// 部件间拓扑连接关系
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TopologyRelation {
    pub part_a: String,
    pub part_b: String,
    pub relation_verb: String,
}

/// 常见非部件前缀（行内式提取时剥离）
const INLINE_PREFIXES: &[&str] = &[
    "至少一个",
    "所述",
    "上述",
    "其中",
    "包括",
    "包含",
    "设有",
    "具有",
    "通过",
    "经由",
    "以及",
    "参见",
    "如图",
    "一个",
    "一种",
    "多个",
    "各个",
    "若干",
    "该",
    "其",
    "与",
    "和",
    "及",
    "或",
    "在",
    "由",
    "将",
    "把",
    "对",
    "向",
    "从",
    "为",
    "设",
    "经",
    "使",
    "令",
    "见",
];

/// 非部件关键词（命中则过滤）
const NON_PART_EXACT: &[&str] = &[
    "图",
    "附图",
    "权利要求",
    "对比文件",
    "文献",
    "实施例",
    "步骤",
    "方法",
    "阶段",
    "方案",
    "方面",
    "特征",
    "公式",
    "模式",
    "问题",
    "缺陷",
    "效果",
    "时间",
    "温度",
    "压力",
    "电压",
    "电流",
    "频率",
    "速度",
    "角度",
    "距离",
    "长度",
    "宽度",
    "高度",
    "厚度",
    "直径",
    "半径",
    "面积",
    "体积",
    "重量",
    "质量",
    "浓度",
    "比例",
    "数量",
    "次数",
];

/// 非部件后缀（以这些词结尾通常不是机械/电气部件）
const NON_PART_SUFFIXES: &[&str] = &[
    "附图",
    "权利要求",
    "对比文件",
    "实施例",
    "步骤",
    "阶段",
    "公式",
    "图",
    "表",
    "项",
    "段",
    "页",
    "行",
    "第",
];

/// 图例式名称后可能黏连的动词或助词，遇到时在此截断
const LEGEND_STOP_TOKENS: &[&str] = &[
    "滑动配合",
    "电连接",
    "固定连接",
    "转动连接",
    "活动连接",
    "安装",
    "连接",
    "连通",
    "啮合",
    "套接",
    "抵接",
    "铰接",
    "固定",
    "同轴",
    "设置",
    "设于",
    "位于",
    "包括",
    "包含",
    "具有",
    "用于",
    "通过",
    "以及",
    "和",
    "与",
    "及",
    "的",
    "在",
    "为",
    "由",
];

/// 拓扑连接关系动词（按长度降序，优先匹配更具体的关系）
const TOPOLOGY_VERBS: &[&str] = &[
    "可拆卸连接",
    "滑动配合",
    "固定连接",
    "转动连接",
    "活动连接",
    "螺纹连接",
    "焊接连接",
    "电连接",
    "连接",
    "连通",
    "啮合",
    "套接",
    "抵接",
    "铰接",
    "固定",
    "同轴",
    "嵌合",
    "卡接",
    "插接",
    "耦合",
    "贴合",
    "支承",
    "支撑",
];

fn strip_inline_prefixes(raw: &str) -> String {
    let mut current = raw.trim();
    // 若行内匹配把动词/介词短语一起吃进来了（如 "固定连接在机架"），截断到最后一个分隔词之后
    const SPLIT_MARKERS: &[&str] = &[
        "连接在",
        "固定在",
        "安装在",
        "设置于",
        "设于",
        "位于",
        "包括",
        "包含",
        "设有",
        "具有",
        "通过",
        "经由",
        "所述",
        "上述",
        "其中",
        "以及",
        "在",
        "于",
        "与",
        "和",
        "及",
        "或",
        "由",
        "将",
        "把",
        "对",
        "向",
        "从",
        "为",
        "的",
    ];
    for marker in SPLIT_MARKERS {
        if let Some(pos) = current.rfind(marker) {
            let after = current.get(pos + marker.len()..).unwrap_or("").trim();
            if after.chars().count() >= 2 {
                current = after;
            }
        }
    }
    loop {
        let mut changed = false;
        for prefix in INLINE_PREFIXES {
            if let Some(rest) = current.strip_prefix(prefix) {
                current = rest.trim();
                changed = true;
                break;
            }
        }
        if !changed {
            break;
        }
    }
    current.to_string()
}

fn trim_legend_name(raw: &str) -> String {
    let mut best_end = raw.len();
    for token in LEGEND_STOP_TOKENS {
        if let Some(idx) = raw.find(token) {
            if idx > 0 && idx < best_end {
                best_end = idx;
            }
        }
    }
    raw.get(0..best_end).unwrap_or(raw).trim().to_string()
}

fn is_valid_part_name(name: &str) -> bool {
    let char_count = name.chars().count();
    if !(2..=12).contains(&char_count) {
        return false;
    }
    if NON_PART_EXACT.contains(&name) {
        return false;
    }
    if NON_PART_SUFFIXES.iter().any(|suf| name.ends_with(suf)) {
        return false;
    }
    // 必须包含至少一个中文字符
    name.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c))
}

fn push_unique_part(parts: &mut Vec<PartDesignator>, number: String, name: String) {
    if !is_valid_part_name(&name) {
        return;
    }
    if parts.iter().any(|p| p.number == number && p.name == name) {
        return;
    }
    parts.push(PartDesignator { number, name });
}

/// 从文本中提取部件标号（支持图例式与行内式，过滤非部件词，去重保序）
pub fn extract_part_designators(text: &str) -> Vec<PartDesignator> {
    let mut parts = Vec::new();
    if text.trim().is_empty() {
        return parts;
    }

    // 1. 图例式：如 "1-机架"、"1、电缸"、"10：阀体"（名称部分不吞掉后续标号数字）
    let Ok(legend_re) = regex::Regex::new(
        r"(?:^|[^\dA-Za-z])(\d{1,3})\s*[-—－、：:]\s*([\u4e00-\u9fa5A-Za-z]{2,16})",
    ) else {
        return parts;
    };

    for cap in legend_re.captures_iter(text) {
        let Some(num_m) = cap.get(1) else {
            continue;
        };
        let Some(name_m) = cap.get(2) else {
            continue;
        };
        let number = num_m.as_str().trim().to_string();
        let name = trim_legend_name(name_m.as_str());
        push_unique_part(&mut parts, number, name);
    }

    // 2. 行内式：如 "阀芯3"、"第一壳体12"、"驱动电机（2）"
    let Ok(inline_re) = regex::Regex::new(r"([\u4e00-\u9fa5]{2,12})\s*[（(]?\s*(\d{1,3})\s*[）)]?")
    else {
        return parts;
    };

    for cap in inline_re.captures_iter(text) {
        let Some(name_m) = cap.get(1) else {
            continue;
        };
        let Some(num_m) = cap.get(2) else {
            continue;
        };
        let cleaned_name = strip_inline_prefixes(name_m.as_str());
        let number = num_m.as_str().trim().to_string();
        push_unique_part(&mut parts, number, cleaned_name);
    }

    parts
}

fn find_parts_in_segment<'a>(segment: &str, parts: &'a [PartDesignator]) -> Vec<(usize, &'a str)> {
    let mut found: Vec<(usize, &str)> = Vec::new();
    for part in parts {
        if let Some(pos) = segment.find(&part.name) {
            if !found.iter().any(|(_, existing)| *existing == part.name) {
                found.push((pos, part.name.as_str()));
            }
        }
    }
    found.sort_by_key(|(pos, _)| *pos);
    found
}

fn find_topology_verb(segment: &str) -> Option<&'static str> {
    TOPOLOGY_VERBS
        .iter()
        .copied()
        .find(|verb| segment.contains(verb))
}

fn push_unique_relation(
    relations: &mut Vec<TopologyRelation>,
    part_a: &str,
    part_b: &str,
    verb: &str,
) {
    if part_a == part_b {
        return;
    }
    let already_exists = relations.iter().any(|r| {
        r.relation_verb == verb
            && ((r.part_a == part_a && r.part_b == part_b)
                || (r.part_a == part_b && r.part_b == part_a))
    });
    if !already_exists {
        relations.push(TopologyRelation {
            part_a: part_a.to_string(),
            part_b: part_b.to_string(),
            relation_verb: verb.to_string(),
        });
    }
}

/// 从文本中提取同一句内已知部件间的拓扑连接关系
pub fn extract_topology_relations(text: &str, parts: &[PartDesignator]) -> Vec<TopologyRelation> {
    let mut relations = Vec::new();
    if text.trim().is_empty() || parts.len() < 2 {
        return relations;
    }

    for sentence in text.split(['。', '；', '！', '？', '\n']) {
        let trimmed = sentence.trim();
        if trimmed.is_empty() {
            continue;
        }

        // 先按逗号子句尝试精确配对
        let mut matched_in_clause = false;
        for clause in trimmed.split(['，', ',']) {
            let Some(verb) = find_topology_verb(clause) else {
                continue;
            };
            let clause_parts = find_parts_in_segment(clause, parts);
            if clause_parts.len() >= 2 {
                matched_in_clause = true;
                for pair in clause_parts.windows(2) {
                    if let [(_, a), (_, b)] = pair {
                        push_unique_relation(&mut relations, a, b, verb);
                    }
                }
            }
        }

        // 若逗号切分未配对成功，回退到整句配对（处理"所述A，与B滑动配合"等跨逗号结构）
        if !matched_in_clause {
            if let Some(verb) = find_topology_verb(trimmed) {
                let sentence_parts = find_parts_in_segment(trimmed, parts);
                if sentence_parts.len() >= 2 {
                    for pair in sentence_parts.windows(2) {
                        if let [(_, a), (_, b)] = pair {
                            push_unique_relation(&mut relations, a, b, verb);
                        }
                    }
                }
            }
        }
    }

    relations
}

fn ref_has_topology_edge(ref_text: &str, rel: &TopologyRelation) -> bool {
    const NEGATION_MARKERS: &[&str] = &[
        "未公开",
        "未提及",
        "未记载",
        "没有公开",
        "不包含",
        "不含",
        "缺少",
        "缺乏",
        "并未",
    ];
    for sentence in ref_text.split(['。', '；', '！', '？', '\n']) {
        if NEGATION_MARKERS.iter().any(|neg| sentence.contains(neg)) {
            continue;
        }
        if sentence.contains(&rel.part_a)
            && sentence.contains(&rel.part_b)
            && sentence.contains(&rel.relation_verb)
        {
            return true;
        }
    }
    false
}

/// 对比本申请部件拓扑边与对比文件原文，输出严谨的专利答复反驳句
pub fn diff_topology_rebuttals(app_text: &str, ref_label: &str, ref_text: &str) -> Vec<String> {
    const MAX_REBUTTALS_PER_REF: usize = 5;

    let app_parts = extract_part_designators(app_text);
    let app_relations = extract_topology_relations(app_text, &app_parts);
    if app_relations.is_empty() || ref_text.trim().is_empty() {
        return Vec::new();
    }

    let mut rebuttals = Vec::new();
    for rel in &app_relations {
        if rebuttals.len() >= MAX_REBUTTALS_PER_REF {
            break;
        }
        let has_both_parts = ref_text.contains(&rel.part_a) && ref_text.contains(&rel.part_b);
        if !has_both_parts {
            rebuttals.push(format!(
                "本申请中“{}”与“{}”之间存在“{}”的拓扑连接关系，而对比文件{}原文未公开上述全部部件及其“{}”连接关系，审查员未就该连接结构的差异给出技术启示证据。",
                rel.part_a, rel.part_b, rel.relation_verb, ref_label, rel.relation_verb
            ));
        } else if !ref_has_topology_edge(ref_text, rel) {
            rebuttals.push(format!(
                "本申请中“{}”与“{}”之间构成“{}”的连接拓扑，对比文件{}原文虽提及相关部件名称，但未在上下文中公开两者之间存在“{}”的连接关系，不能直接得出两者结构相同的结论。",
                rel.part_a, rel.part_b, rel.relation_verb, ref_label, rel.relation_verb
            ));
        }
    }

    rebuttals
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_legend_and_inline_parts_and_filters_noise() {
        let text = "如图1所示，实施例1中的权利要求1记载了一种装置：1-机架，2、电缸，10：阀体。\
                    工作时，阀芯3与第一壳体12滑动配合，对比文件1及步骤1不构成部件。";
        let parts = extract_part_designators(text);
        let pairs: Vec<(&str, &str)> = parts
            .iter()
            .map(|p| (p.number.as_str(), p.name.as_str()))
            .collect();

        assert!(pairs.contains(&("1", "机架")), "got: {:?}", pairs);
        assert!(pairs.contains(&("2", "电缸")), "got: {:?}", pairs);
        assert!(pairs.contains(&("10", "阀体")), "got: {:?}", pairs);
        assert!(pairs.contains(&("3", "阀芯")), "got: {:?}", pairs);
        assert!(pairs.contains(&("12", "第一壳体")), "got: {:?}", pairs);

        assert!(!parts.iter().any(|p| p.name.contains("图")));
        assert!(!parts.iter().any(|p| p.name.contains("权利要求")));
        assert!(!parts.iter().any(|p| p.name.contains("对比文件")));
        assert!(!parts.iter().any(|p| p.name.contains("实施例")));
        assert!(!parts.iter().any(|p| p.name.contains("步骤")));
    }

    #[test]
    fn extracts_topology_relations_and_diffs_ref() {
        let app = "1-机架，2-阀体，3-阀芯。阀芯3与阀体2滑动配合，阀体2固定连接在机架1上。";
        let parts = extract_part_designators(app);
        let rels = extract_topology_relations(app, &parts);
        assert!(rels.iter().any(|r| {
            r.part_a == "阀芯" && r.part_b == "阀体" && r.relation_verb == "滑动配合"
        }));
        assert!(rels.iter().any(|r| {
            r.part_a == "阀体" && r.part_b == "机架" && r.relation_verb == "固定连接"
        }));

        let ref_text = "对比文件公开了一种机架和阀体，但未提及阀芯。";
        let rebuttals = diff_topology_rebuttals(app, "D1", ref_text);
        assert!(!rebuttals.is_empty());
        assert!(rebuttals
            .iter()
            .any(|s| s.contains("阀芯") && s.contains("D1")));
    }
}
