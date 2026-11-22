//! 模块职责：IPC 子空间自动锚定与跨领域同词噪声过滤（v0.9.67）。
//! 上游调用者：`src/routes/search.rs`（本地/向量/在线检索）与 `src/search/agent.rs`。
//! 模块专属禁区：严禁编造不存在的 IPC 分类号；未匹配到 IPC 信号的文献只降权/标明缺失，绝不伪造分类码。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 自动推导出的 IPC 主分类子空间锚点。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IpcAnchor {
    /// 4 位 IPC 小类码（如 `H02K`、`F28D`、`H01M`）
    pub code: String,
    /// 中文工程领域简称
    pub label_zh: String,
    /// 锚定置信度（0.0 ~ 1.0）
    pub confidence: f64,
    /// 在候选集或本地语料中观测到的命中篇数
    pub hit_count: usize,
    /// 锚定来源：`"corpus"`（语料实测分布）或 `"lexicon"`（确定性工程词典）或 `"hybrid"`
    pub source: String,
}

/// 确定性工程领域 → IPC 小类映射表（覆盖机电/热管理/能源/材料/医工/AI/燃烧/流体等高频学科）。
struct DomainIpcRule {
    code: &'static str,
    label_zh: &'static str,
    keywords: &'static [&'static str],
}

const DOMAIN_IPC_RULES: &[DomainIpcRule] = &[
    DomainIpcRule {
        code: "H02K",
        label_zh: "电机与电机冷却",
        keywords: &[
            "电机",
            "定子",
            "转子",
            "绕组",
            "永磁",
            "驱动电机",
            "motor",
            "stator",
            "rotor",
            "winding",
        ],
    },
    DomainIpcRule {
        code: "F28D",
        label_zh: "热交换装置与散热",
        keywords: &[
            "散热",
            "换热",
            "导热",
            "液冷",
            "水冷",
            "冷却液",
            "均温板",
            "热管",
            "散热器",
            "heat exchanger",
            "cooling",
            "thermal",
            "radiator",
            "heat pipe",
        ],
    },
    DomainIpcRule {
        code: "F28F",
        label_zh: "传热翅片与隔热结构",
        keywords: &[
            "翅片",
            "隔热",
            "保温",
            "流道",
            "冷板",
            "fin",
            "insulation",
            "cold plate",
        ],
    },
    DomainIpcRule {
        code: "H01M",
        label_zh: "电池与电化学储能",
        keywords: &[
            "电池",
            "固态电池",
            "锂电",
            "电解质",
            "正极",
            "负极",
            "电芯",
            "储能",
            "燃料电池",
            "battery",
            "solid-state",
            "electrolyte",
            "cathode",
            "anode",
            "lithium",
        ],
    },
    DomainIpcRule {
        code: "F23C",
        label_zh: "燃烧方法与流化床燃烧",
        keywords: &[
            "燃烧",
            "煤粉",
            "锅炉",
            "低氮",
            "助燃",
            "combustion",
            "boiler",
            "burner",
        ],
    },
    DomainIpcRule {
        code: "F23D",
        label_zh: "燃烧器与喷嘴结构",
        keywords: &["燃烧器", "喷嘴", "火嘴", "燃气", "nozzle", "injector"],
    },
    DomainIpcRule {
        code: "F22B",
        label_zh: "蒸汽与余热锅炉",
        keywords: &[
            "余热锅炉",
            "蒸汽",
            "受热面",
            "过热器",
            "steam",
            "waste heat",
        ],
    },
    DomainIpcRule {
        code: "H01L",
        label_zh: "半导体与芯片封装散热",
        keywords: &[
            "芯片",
            "半导体",
            "晶圆",
            "封装",
            "功率模块",
            "igbt",
            "sic",
            "semiconductor",
            "chip",
            "wafer",
        ],
    },
    DomainIpcRule {
        code: "H05K",
        label_zh: "印刷电路与机箱热管理",
        keywords: &[
            "pcb",
            "电路板",
            "机箱",
            "服务器",
            "逆变器",
            "控制器",
            "enclosure",
        ],
    },
    DomainIpcRule {
        code: "F16K",
        label_zh: "阀门与流体控制",
        keywords: &["阀门", "电磁阀", "节流阀", "单向阀", "valve", "solenoid"],
    },
    DomainIpcRule {
        code: "F16L",
        label_zh: "管道连接与密封",
        keywords: &[
            "管道",
            "法兰",
            "快插",
            "接头",
            "密封圈",
            "pipe",
            "flange",
            "seal",
        ],
    },
    DomainIpcRule {
        code: "F04D",
        label_zh: "离心泵与风机叶轮",
        keywords: &[
            "油泵",
            "水泵",
            "叶轮",
            "风机",
            "离心泵",
            "pump",
            "impeller",
            "blower",
        ],
    },
    DomainIpcRule {
        code: "B01D",
        label_zh: "过滤分离与脱硫除尘",
        keywords: &[
            "过滤",
            "滤芯",
            "滤罐",
            "脱硫",
            "除尘",
            "膜分离",
            "吸附",
            "filter",
            "membrane",
            "separation",
        ],
    },
    DomainIpcRule {
        code: "B25J",
        label_zh: "工业机器人与机械臂",
        keywords: &[
            "机器人",
            "机械臂",
            "抓取",
            "灵巧手",
            "末端执行器",
            "robot",
            "manipulator",
        ],
    },
    DomainIpcRule {
        code: "B60L",
        label_zh: "电动车辆电驱与热管理",
        keywords: &[
            "新能源车",
            "电动车",
            "电驱",
            "整车热管理",
            "充电桩",
            "ev",
            "electric vehicle",
        ],
    },
    DomainIpcRule {
        code: "G06N",
        label_zh: "人工智能与机器学习",
        keywords: &[
            "神经网络",
            "大模型",
            "深度学习",
            "强化学习",
            "算法",
            "transformer",
            "neural",
            "machine learning",
        ],
    },
    DomainIpcRule {
        code: "G06F",
        label_zh: "电子数字数据处理",
        keywords: &[
            "检索",
            "数据库",
            "向量",
            "索引",
            "编译器",
            "软件",
            "computing",
            "retrieval",
        ],
    },
    DomainIpcRule {
        code: "G01N",
        label_zh: "材料测试与成分检测",
        keywords: &[
            "含水率",
            "传感器",
            "检测",
            "光谱",
            "色谱",
            "探头",
            "sensor",
            "detection",
            "spectroscopy",
        ],
    },
    DomainIpcRule {
        code: "C08L",
        label_zh: "高分子复合材料与硅油",
        keywords: &[
            "硅油",
            "树脂",
            "聚合物",
            "橡胶",
            "复合材料",
            "polymer",
            "silicone",
            "resin",
        ],
    },
    DomainIpcRule {
        code: "C22C",
        label_zh: "金属合金材料",
        keywords: &[
            "合金",
            "铝合金",
            "钛合金",
            "不锈钢",
            "高温合金",
            "alloy",
            "titanium",
        ],
    },
    DomainIpcRule {
        code: "A61B",
        label_zh: "医疗诊断与手术器械",
        keywords: &[
            "医疗",
            "内窥镜",
            "手术",
            "影像",
            "诊断",
            "监护",
            "超声",
            "medical",
            "surgical",
            "diagnostic",
        ],
    },
    DomainIpcRule {
        code: "A61K",
        label_zh: "医药制剂与药物递送",
        keywords: &[
            "药物",
            "制剂",
            "疫苗",
            "纳米载体",
            "靶向",
            "pharmaceutical",
            "drug",
            "clinical",
        ],
    },
    DomainIpcRule {
        code: "C12N",
        label_zh: "生物工程与基因细胞",
        keywords: &[
            "细胞",
            "基因",
            "酶",
            "抗体",
            "微生物",
            "crispr",
            "gene",
            "cell",
            "protein",
        ],
    },
];

/// 把原始 IPC 串（如 `"H02K 9/19"`, `"F28D15/02"`, `["H02K9/00"]`）规整为 4 位小类码（如 `"H02K"`）。
pub fn normalize_ipc_subclass(raw: &str) -> Option<String> {
    let cleaned: String = raw
        .trim()
        .to_ascii_uppercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    let bytes = cleaned.as_bytes();
    if bytes.len() < 4 {
        return None;
    }
    let valid = matches!(bytes[0], b'A'..=b'H')
        && bytes[1].is_ascii_digit()
        && bytes[2].is_ascii_digit()
        && bytes[3].is_ascii_uppercase();
    if valid {
        Some(cleaned.chars().take(4).collect())
    } else {
        None
    }
}

/// 从 JSON 数组或逗号/分号分隔字符串中抽取去重后的 4 位 IPC 小类列表。
pub fn parse_ipc_subclasses(raw: &str) -> Vec<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let push_unique = |s: &str, acc: &mut Vec<String>| {
        if let Some(code) = normalize_ipc_subclass(s) {
            if !acc.contains(&code) {
                acc.push(code);
            }
        }
    };

    if trimmed.starts_with('[') {
        if let Ok(arr) = serde_json::from_str::<Vec<String>>(trimmed) {
            for item in arr {
                push_unique(&item, &mut out);
            }
            return out;
        }
    }
    for part in trimmed.split([',', ';', '|', ' ', '\n', '\t', '、']) {
        push_unique(part, &mut out);
    }
    out
}

/// 查询 IPC 4 位小类的中文领域标签；未收录时回退通用分部说明。
pub fn ipc_label_zh(subclass: &str) -> String {
    let upper = subclass.trim().to_ascii_uppercase();
    if let Some(rule) = DOMAIN_IPC_RULES.iter().find(|r| r.code == upper) {
        return rule.label_zh.to_string();
    }
    match upper.as_bytes().first() {
        Some(b'A') => "人类生活必需/医疗".to_string(),
        Some(b'B') => "作业与运输/制造".to_string(),
        Some(b'C') => "化学与冶金材料".to_string(),
        Some(b'D') => "纺织与造纸".to_string(),
        Some(b'E') => "固定建筑物与土木".to_string(),
        Some(b'F') => "机械工程/热能/流体".to_string(),
        Some(b'G') => "物理/检测/计算".to_string(),
        Some(b'H') => "电学/电机/半导体".to_string(),
        _ => "技术分类子空间".to_string(),
    }
}

/// 推导 Top-1~3 IPC 子空间锚点：融合「候选集真实 IPC 分布」与「查询词确定性领域词典」。
///
/// `observed_ipc_blobs` 为每篇相关候选专利的 `ipc_codes` 原文字符串切片。
pub fn derive_ipc_anchors(
    query: &str,
    observed_ipc_blobs: &[String],
    top_k: usize,
) -> Vec<IpcAnchor> {
    let top_k = top_k.clamp(1, 5);
    let q_lower = query.to_ascii_lowercase();

    // 1. 统计语料中真实观测到的 IPC 小类频次
    let mut corpus_counts: HashMap<String, usize> = HashMap::new();
    let mut total_ipc_docs = 0usize;
    for blob in observed_ipc_blobs {
        let codes = parse_ipc_subclasses(blob);
        if !codes.is_empty() {
            total_ipc_docs += 1;
            for c in codes {
                *corpus_counts.entry(c).or_insert(0) += 1;
            }
        }
    }

    // 2. 确定性词典匹配（同时支持用户直接输入 IPC 码如 "H02K"）
    let mut lexicon_scores: HashMap<String, f64> = HashMap::new();
    for token in query.split(|c: char| !c.is_ascii_alphanumeric()) {
        if let Some(code) = normalize_ipc_subclass(token) {
            *lexicon_scores.entry(code).or_insert(0.0) += 1.0;
        }
    }
    for rule in DOMAIN_IPC_RULES {
        let mut hits = 0usize;
        for kw in rule.keywords {
            if q_lower.contains(&kw.to_ascii_lowercase()) {
                hits += 1;
            }
        }
        if hits > 0 {
            let score = (0.55 + 0.15 * (hits.saturating_sub(1) as f64)).min(0.95);
            let entry = lexicon_scores.entry(rule.code.to_string()).or_insert(0.0);
            if score > *entry {
                *entry = score;
            }
        }
    }

    // 3. 融合排序
    let mut all_codes: Vec<String> = corpus_counts
        .keys()
        .chain(lexicon_scores.keys())
        .cloned()
        .collect();
    all_codes.sort();
    all_codes.dedup();

    let mut anchors: Vec<IpcAnchor> = all_codes
        .into_iter()
        .map(|code| {
            let hit_count = *corpus_counts.get(&code).unwrap_or(&0);
            let lex = *lexicon_scores.get(&code).unwrap_or(&0.0);
            let corpus_ratio = if total_ipc_docs > 0 {
                (hit_count as f64 / total_ipc_docs as f64).min(1.0)
            } else {
                0.0
            };
            let (confidence, source) = match (hit_count > 0, lex > 0.0) {
                (true, true) => (
                    (0.55 * lex + 0.45 * corpus_ratio + 0.12).min(0.99),
                    "hybrid",
                ),
                (true, false) => ((0.35 + 0.60 * corpus_ratio).min(0.95), "corpus"),
                (false, true) => (lex, "lexicon"),
                (false, false) => (0.0, "none"),
            };
            IpcAnchor {
                label_zh: ipc_label_zh(&code),
                code,
                confidence: (confidence * 100.0).round() / 100.0,
                hit_count,
                source: source.to_string(),
            }
        })
        .filter(|a| a.confidence >= 0.40)
        .collect();

    anchors.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.hit_count.cmp(&a.hit_count))
            .then_with(|| a.code.cmp(&b.code))
    });
    anchors.truncate(top_k);
    anchors
}

/// 解析用户锁定的 IPC 过滤串（支持逗号/空格分隔多个前缀，如 `"H02K,F28D"`）。
pub fn parse_locked_ipc_prefixes(raw: Option<&str>) -> Vec<String> {
    let Some(s) = raw else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for part in s.split([',', ';', ' ', '|']) {
        let up: String = part
            .trim()
            .to_ascii_uppercase()
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '/')
            .collect();
        if up.len() >= 3 && !out.contains(&up) {
            out.push(up);
        }
    }
    out
}

/// 判定一篇文献是否匹配用户锁定的 IPC 子空间前缀集合。
///
/// - 若该文献带有 IPC 分类号，必须至少命中一个锁定前缀（防止跨领域同词噪声混入）；
/// - 若该文献无 IPC 元数据（如学术论文），则按文本关键词是否属于目标 IPC 领域做二次核验。
pub fn matches_locked_ipc_subspace(
    item_ipc_blob: &str,
    item_title_and_abstract: &str,
    locked_prefixes: &[String],
) -> bool {
    if locked_prefixes.is_empty() {
        return true;
    }
    let item_codes = parse_ipc_subclasses(item_ipc_blob);
    let raw_upper = item_ipc_blob.to_ascii_uppercase();
    if !item_codes.is_empty() {
        return locked_prefixes.iter().any(|pref| {
            item_codes
                .iter()
                .any(|c| c.starts_with(pref) || pref.starts_with(c))
                || raw_upper.contains(pref)
        });
    }

    // 无 IPC 元数据（如学术文献）：仅在标题/摘要命中对应 IPC 领域关键词时放行，绝不盲目全放行
    let text_lower = item_title_and_abstract.to_ascii_lowercase();
    locked_prefixes.iter().any(|pref| {
        let sub = normalize_ipc_subclass(pref).unwrap_or_else(|| pref.clone());
        DOMAIN_IPC_RULES
            .iter()
            .find(|r| r.code == sub)
            .map(|rule| {
                rule.keywords
                    .iter()
                    .any(|kw| text_lower.contains(&kw.to_ascii_lowercase()))
            })
            .unwrap_or(false)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_top_ipc_anchors_from_query_and_corpus() {
        let corpus = vec![
            r#"["H02K 9/19", "F28D 15/02"]"#.to_string(),
            r#"["H02K 5/20"]"#.to_string(),
            r#"["F28D 1/053"]"#.to_string(),
        ];
        let anchors = derive_ipc_anchors("电机 浸没式 液冷 散热", &corpus, 3);
        assert!(!anchors.is_empty());
        let codes: Vec<&str> = anchors.iter().map(|a| a.code.as_str()).collect();
        assert!(
            codes.contains(&"H02K"),
            "应锚定电机小类 H02K: {:?}",
            anchors
        );
        assert!(
            codes.contains(&"F28D"),
            "应锚定换热小类 F28D: {:?}",
            anchors
        );
        assert_eq!(anchors[0].source, "hybrid");
    }

    /// 负面反作弊测试：锁定 IPC 子空间后，跨领域同词噪声（如把医疗“冷却敷贴”混入电机散热检索）必须被拦截
    #[test]
    fn anti_cheat_locked_ipc_subspace_blocks_cross_domain_homonym_noise() {
        let locked = parse_locked_ipc_prefixes(Some("H02K,F28D"));
        assert_eq!(locked, vec!["H02K", "F28D"]);

        // 真正同领域的电机液冷专利 → 放行
        assert!(matches_locked_ipc_subspace(
            r#"["H02K9/19"]"#,
            "一种永磁同步电机浸没式冷却结构",
            &locked
        ));
        // 跨领域同词噪声（A61F 医用退热冷却贴）→ 必须拦截！
        assert!(!matches_locked_ipc_subspace(
            r#"["A61F7/02"]"#,
            "一种医用物理退热冷却降温凝胶贴",
            &locked
        ));
        // 无 IPC 元数据的不相关医美论文 → 必须拦截！
        assert!(!matches_locked_ipc_subspace(
            "",
            "Clinical evaluation of facial cosmetic gel sheet",
            &locked
        ));
        // 无 IPC 元数据但确实是电机冷却学术论文 → 放行
        assert!(matches_locked_ipc_subspace(
            "",
            "Direct oil immersion cooling for high power density electric motor stator winding",
            &locked
        ));
    }

    #[test]
    fn normalize_ipc_rejects_invalid_strings() {
        assert_eq!(normalize_ipc_subclass("H02K 9/19"), Some("H02K".into()));
        assert_eq!(normalize_ipc_subclass("f28d15/02"), Some("F28D".into()));
        assert_eq!(normalize_ipc_subclass("12345"), None);
        assert_eq!(normalize_ipc_subclass("Z99Z"), None);
    }
}
