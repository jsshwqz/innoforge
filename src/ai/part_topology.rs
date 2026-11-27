//! 模块职责：部件标号提取、拓扑连接关系校验与交底书/权要/附图 V-RAG 标号一致性静态核查。
//! 上游调用者：`src/ai/strategy_arbitration.rs`、`src/ai/vision_topology.rs`、`src/routes/patent.rs`、`/api/ai/numeral-consistency-check`。
//! 模块专属禁区：严禁在标号核查中将「图1」「权利要求1」「步骤1」「实施例1」误判为机械部件标号；严禁使用裸字节切片截取中文上下文片段。

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
    // 若行内匹配把动词/介词短语一起吃进来了（如 "固定连接在机架" / "并安装电磁线圈"），截断到最后一个分隔词之后
    const SPLIT_MARKERS: &[&str] = &[
        "固定设置有",
        "连通设置有",
        "内部安装有",
        "顶部设置有",
        "一侧设置有",
        "一端设置有",
        "上设置有",
        "固定连接有",
        "固定安装有",
        "并安装有",
        "还设置有",
        "连接在",
        "固定在",
        "安装在",
        "设置于",
        "设于",
        "位于",
        "安装有",
        "并安装",
        "还设置",
        "设置有",
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
        "安装",
        "设置",
        "驱动",
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
        "并",
        "且",
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
                let candidate = rest.trim();
                if candidate.chars().count() >= 2 {
                    current = candidate;
                    changed = true;
                    break;
                }
            }
        }
        if !changed {
            break;
        }
    }
    current.to_string()
}

/// 判断数字上下文是否属于温度、百分比、日期、申请号、物理量或动作动词短语（非机械部件标号）
fn is_measurement_or_non_part_context(
    text: &str,
    num_start: usize,
    num_end: usize,
    number: &str,
    raw_name: &str,
) -> bool {
    // 专利附图标记一般从 1 开始，绝不为 0 或带前导零
    if number.is_empty() || number == "0" || number.starts_with('0') {
        return true;
    }

    // 检查数字紧前字符：若紧邻数字或小数点（如长编号后缀 .1 或小数 3.5）直接拒绝
    if let Some(prev_ch) = text.get(..num_start).and_then(|s| s.chars().next_back()) {
        if prev_ch.is_ascii_digit() || prev_ch == '.' {
            return true;
        }
    }

    // 检查数字紧后文本：若紧跟物理单位、量词、日期、百分比、小数点或范围符号，直接拒绝
    let tail = text.get(num_end..).unwrap_or("");
    let tail_trimmed_space = tail.trim_start_matches([' ', '\t']);
    // 若数字前有左括号但后无右括号，或紧后是小数/范围符
    if let Some(next_ch) = tail_trimmed_space.chars().next() {
        if next_ch.is_ascii_digit()
            || matches!(
                next_ch,
                '.' | '%'
                    | '％'
                    | '℃'
                    | '°'
                    | '~'
                    | '～'
                    | '年'
                    | '月'
                    | '日'
                    | '时'
                    | '分'
                    | '秒'
                    | '个'
                    | '组'
                    | '台'
                    | '套'
                    | '条'
                    | '根'
                    | '块'
                    | '片'
                    | '层'
                    | '级'
                    | '次'
                    | '倍'
                    | '份'
                    | '种'
                    | '类'
                    | '项'
                    | '页'
                    | '段'
                    | '点'
                    | '天'
                    | '周'
                    | '米'
                    | '克'
                    | '升'
                    | '度'
                    | '瓦'
                    | '伏'
                    | '安'
                    | '赫'
            )
        {
            return true;
        }
    }
    let tail_lower: String = tail_trimmed_space
        .chars()
        .take(5)
        .map(|c| c.to_ascii_lowercase())
        .collect();
    const ASCII_UNIT_PREFIXES: &[&str] = &[
        "mpa", "kpa", "pa", "mm", "cm", "km", "kg", "mg", "kw", "mw", "kv", "ma", "hz", "rpm",
        "ml", "min", "sec", "hr", "°c",
    ];
    if ASCII_UNIT_PREFIXES
        .iter()
        .any(|u| tail_lower.starts_with(u))
    {
        return true;
    }
    // 单字母单位（如 5m、10g、20s、30h、12V、5A、10W、2L），要求紧后非字母
    if let Some(first) = tail_lower.chars().next() {
        if matches!(first, 'm' | 'g' | 's' | 'h' | 'v' | 'a' | 'w' | 'l') {
            let second = tail_lower.chars().nth(1);
            if second.is_none_or(|c| !c.is_ascii_alphabetic()) {
                return true;
            }
        }
    }

    // 检查前文是否包含典型数值/非部件触发语
    const NUMERIC_CUE_SUBSTRINGS: &[&str] = &[
        "降为",
        "降温到",
        "下降到",
        "上升到",
        "预热到",
        "加热到",
        "冷却到",
        "升温到",
        "温度为",
        "温度大概为",
        "温度高达",
        "温度约",
        "温度在",
        "温度达",
        "温度降",
        "温度升",
        "效率达到",
        "回收达到",
        "达到",
        "高达",
        "节能",
        "仅有",
        "持续",
        "约为",
        "大约",
        "大概",
        "超过",
        "大于",
        "小于",
        "不少于",
        "不超过",
        "至少为",
        "起点为",
        "距离为",
        "压力为",
        "转速为",
        "功率为",
        "流量为",
        "产生近",
        "吸收",
        "排出",
        "申请日",
        "申请号",
        "本公开",
        "本发明",
        "实用新",
        "实施例",
        "权利要求",
        "对比文件",
        "步骤",
        "附图",
        "阶段",
        "模式",
        "回路中",
        "测量值为",
        "最大值为",
        "最小值为",
        "平均值为",
        "理想值为",
        "设定值为",
        "目标值为",
        "标准值为",
        "除以",
        "乘以",
        "相加",
        "相减",
        "求和",
        "取值",
        "权重",
        "系数",
        "指标",
        "阈值",
        "评分为",
        "得分为",
        "比值为",
        "范围为",
        "区间为",
        "设为",
        "记为",
        "等于",
        "之和",
        "之差",
        "之积",
        "之商",
    ];
    if NUMERIC_CUE_SUBSTRINGS
        .iter()
        .any(|cue| raw_name.contains(cue))
    {
        return true;
    }
    let trimmed_raw = raw_name.trim();
    if trimmed_raw.ends_with('为')
        || trimmed_raw.ends_with("除以")
        || trimmed_raw.ends_with("乘以")
        || trimmed_raw.ends_with("等于")
    {
        return true;
    }

    false
}

/// 定位说明书中的「附图标记说明 / 图中，」专属段落（返回字节起止区间列表）
fn find_explicit_legend_blocks(text: &str) -> Vec<(usize, usize)> {
    const LEGEND_HEADERS: &[&str] = &[
        "附图标记说明",
        "附图标记的含义",
        "附图标记如下",
        "附图标记：",
        "附图标记:",
        "附图标记",
        "附图标号说明",
        "附图标号",
        "附图中，",
        "附图中:",
        "附图中：",
        "图中，",
        "图中：",
        "图中:",
    ];
    const STOP_HEADERS: &[&str] = &[
        "具体实施方式",
        "实施例1",
        "实施例 1",
        "下面结合附图",
        "为使本发明",
        "为了使本",
    ];

    let mut blocks = Vec::new();
    let mut search_from = 0usize;
    while search_from < text.len() {
        let Some(slice) = text.get(search_from..) else {
            break;
        };
        let mut best_match: Option<(usize, usize)> = None;
        for header in LEGEND_HEADERS {
            if let Some(rel) = slice.find(header) {
                let abs_start = search_from + rel;
                let abs_after = abs_start + header.len();
                if best_match.is_none_or(|(b_start, _)| abs_start < b_start) {
                    best_match = Some((abs_start, abs_after));
                }
            }
        }
        let Some((hdr_start, hdr_after)) = best_match else {
            break;
        };
        let rest = text.get(hdr_after..).unwrap_or("");
        // 判断是否为 Google Patents / CNIPA 抓取的双换行多列表格型图例（如「附图标号说明：\n\n标号\n\n名称\n\n10\n\n定子总成...」）
        let leading_lines: Vec<&str> = rest
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && *l != "：" && *l != ":")
            .take(6)
            .collect();
        let is_multiline_table_legend = leading_lines.len() >= 2
            && (matches!(
                leading_lines[0],
                "标号" | "名称" | "附图标记" | "部件名称" | "序号"
            ) || (leading_lines[0].chars().all(|c| c.is_ascii_digit())
                && leading_lines[0].len() <= 3
                && leading_lines[1].chars().count() <= 14
                && !leading_lines[1].contains('。')));

        // 寻找图例段结尾：表格型图例可跨 \n\n 直至遇到长叙述句或停止章节标题；普通图例遇双换行/停止标题即停
        let mut end_rel = rest.len().min(if is_multiline_table_legend {
            6000
        } else {
            2000
        });
        if is_multiline_table_legend {
            let mut offset = 0usize;
            for raw_line in rest.split_inclusive('\n') {
                let t = raw_line.trim();
                let is_narrative = t.chars().count() > 26 && (t.contains('。') || t.contains('，'));
                if is_narrative && offset > 16 {
                    end_rel = end_rel.min(offset);
                    break;
                }
                offset += raw_line.len();
                if offset >= end_rel {
                    break;
                }
            }
        } else {
            if let Some(dbl) = rest.find("\n\n") {
                if dbl > 8 {
                    end_rel = end_rel.min(dbl);
                }
            }
            if let Some(dbl_crlf) = rest.find("\r\n\r\n") {
                if dbl_crlf > 8 {
                    end_rel = end_rel.min(dbl_crlf);
                }
            }
        }
        for stop in STOP_HEADERS {
            if let Some(pos) = rest.find(stop) {
                if pos > 4 {
                    end_rel = end_rel.min(pos);
                }
            }
        }
        // 回退到字符边界
        while end_rel > 0 && !rest.is_char_boundary(end_rel) {
            end_rel -= 1;
        }
        let abs_end = hdr_after + end_rel;
        if abs_end > hdr_after {
            blocks.push((hdr_start, abs_end));
            search_from = abs_end;
        } else {
            search_from = hdr_after;
        }
    }
    blocks
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

/// 从文本中提取部件标号（支持图例式与行内式，过滤数值/单位/非部件词，去重保序）
pub fn extract_part_designators(text: &str) -> Vec<PartDesignator> {
    let mut parts = Vec::new();
    if text.trim().is_empty() {
        return parts;
    }

    // 1. 图例式（仅当标号前为行首/空白/标点，严禁把 "再生滚筒1、热风炉2" 错配为 "1、热风炉"！）
    let (legend_parts, legend_ranges) = extract_legend_designators_with_ranges(text);
    for p in legend_parts {
        push_unique_part(&mut parts, p.number, p.name);
    }

    // 2. 行内式（从剥离图例后的正文提取，过滤温度/百分比/日期/申请号/不匹配括号）
    let body = strip_legend_ranges(text, &legend_ranges);
    for p in extract_inline_designators(&body) {
        push_unique_part(&mut parts, p.number, p.name);
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

/// 单个标号的全文索引摘要（供 V-RAG 附图标号 ↔ 正文双向联动渲染）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NumeralPartSummary {
    pub number: String,
    pub name: String,
    pub all_names: Vec<String>,
    pub in_claims: bool,
    pub in_description: bool,
    pub in_legend: bool,
    pub in_drawing: bool,
    pub mention_count: usize,
    pub snippets: Vec<String>,
}

/// 标号一致性静态核查单条告警
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NumeralConsistencyIssue {
    /// `FIGURE_OR_LEGEND_ONLY` | `TEXT_ONLY_MISSING_IN_FIGURE` | `NAME_INCONSISTENT_FOR_SAME_NUMERAL`
    pub code: String,
    /// `error` | `warning`
    pub severity: String,
    pub numeral: String,
    pub title: String,
    pub message: String,
    pub suggestion: String,
}

/// 交底书 / 权利要求 / 附图标号一致性核查报告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NumeralConsistencyReport {
    pub passed: bool,
    pub total_numerals: usize,
    pub issue_count: usize,
    pub summary: String,
    pub parts: Vec<NumeralPartSummary>,
    pub issues: Vec<NumeralConsistencyIssue>,
}

/// 提取说明书中的全部三种标准 CNIPA 图例格式，并返回对应字节区间：
/// - 格式 A（数字-分隔符-名称）：`1-阀体；2、膜片；10：防尘盖`（标号前必为标点/空白/行首，严禁吞掉前一个中文部件名！）
/// - 格式 B（显式图例段内数字直连名称）：`附图标记的含义：1新润滑油脂加注装置；2转动设备轴承；3量具`
/// - 格式 C（显式图例段内名称直连数字列表）：`附图标记\n再生滚筒1、热风炉2、再生燃烧器3`
fn extract_legend_designators_with_ranges(
    text: &str,
) -> (Vec<PartDesignator>, Vec<(usize, usize)>) {
    let mut parts = Vec::new();
    let mut ranges = Vec::new();

    let blocks = find_explicit_legend_blocks(text);
    let legend_sep_re = regex::Regex::new(
        r"(?:^|[，,。；;\n\r\t ：:])(\d{1,3})\s*[-—－、：:]\s*([\u4e00-\u9fa5A-Za-z]{2,16})",
    )
    .ok();
    let legend_direct_num_name_re =
        regex::Regex::new(r"(?:^|[，,。；;\n\r\t ：:])(\d{1,3})([\u4e00-\u9fa5]{2,14})").ok();
    let legend_name_num_re =
        regex::Regex::new(r"^([\u4e00-\u9fa5]{2,14})\s*[（(]?(\d{1,3})[）)]?$").ok();

    for &(b_start, b_end) in &blocks {
        let Some(block_text) = text.get(b_start..b_end) else {
            continue;
        };
        let mut block_found = 0usize;

        // 格式 D（Google Patents / CNIPA 抓取的双换行多列表格：`标号\n\n名称\n\n10\n\n定子总成\n\n400\n\n槽绝缘纸`）
        let non_empty_lines: Vec<&str> = block_text
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect();
        if non_empty_lines.len() >= 4 {
            const TABLE_HEADERS: &[&str] = &[
                "标号",
                "名称",
                "附图标记",
                "部件名称",
                "序号",
                "说明",
                "附图标记说明",
                "附图标记说明：",
                "附图标号说明",
                "附图标号说明：",
            ];
            let mut idx = 0usize;
            while idx + 1 < non_empty_lines.len() {
                let cur = non_empty_lines[idx];
                if TABLE_HEADERS.contains(&cur) {
                    idx += 1;
                    continue;
                }
                let is_pure_num =
                    !cur.is_empty() && cur.len() <= 3 && cur.chars().all(|c| c.is_ascii_digit());
                if is_pure_num && cur != "0" && !cur.starts_with('0') {
                    let next_line = non_empty_lines[idx + 1];
                    if !TABLE_HEADERS.contains(&next_line)
                        && !next_line.chars().all(|c| c.is_ascii_digit())
                        && !next_line.contains('。')
                    {
                        let name = trim_legend_name(next_line);
                        if is_valid_part_name(&name) {
                            push_unique_part(&mut parts, cur.to_string(), name);
                            block_found += 1;
                            idx += 2;
                            continue;
                        }
                    }
                }
                idx += 1;
            }
        }

        if block_found == 0 {
            if let Some(ref re) = legend_sep_re {
                for cap in re.captures_iter(block_text) {
                    if let (Some(num_m), Some(name_m)) = (cap.get(1), cap.get(2)) {
                        let number = num_m.as_str().trim().to_string();
                        if number == "0" || number.starts_with('0') {
                            continue;
                        }
                        let name = trim_legend_name(name_m.as_str());
                        if is_valid_part_name(&name) {
                            push_unique_part(&mut parts, number, name);
                            block_found += 1;
                        }
                    }
                }
            }
        }

        if block_found == 0 {
            if let Some(ref re) = legend_direct_num_name_re {
                for cap in re.captures_iter(block_text) {
                    if let (Some(num_m), Some(name_m)) = (cap.get(1), cap.get(2)) {
                        let number = num_m.as_str().trim().to_string();
                        if number == "0" || number.starts_with('0') {
                            continue;
                        }
                        let name = trim_legend_name(name_m.as_str());
                        if is_valid_part_name(&name) {
                            push_unique_part(&mut parts, number, name);
                            block_found += 1;
                        }
                    }
                }
            }
        }

        if block_found == 0 {
            if let Some(ref re) = legend_name_num_re {
                for seg in
                    block_text.split(['，', ',', '。', '；', ';', '、', '\n', '\r', '：', ':'])
                {
                    let trimmed = seg.trim();
                    if let Some(cap) = re.captures(trimmed) {
                        if let (Some(name_m), Some(num_m)) = (cap.get(1), cap.get(2)) {
                            let number = num_m.as_str().trim().to_string();
                            if number == "0" || number.starts_with('0') {
                                continue;
                            }
                            let name = strip_inline_prefixes(name_m.as_str());
                            if is_valid_part_name(&name) {
                                push_unique_part(&mut parts, number, name);
                                block_found += 1;
                            }
                        }
                    }
                }
            }
        }

        if block_found > 0 {
            ranges.push((b_start, b_end));
        }
    }

    if parts.is_empty() {
        if let Some(ref re) = legend_sep_re {
            for cap in re.captures_iter(text) {
                let (Some(num_m), Some(name_m)) = (cap.get(1), cap.get(2)) else {
                    continue;
                };
                let number = num_m.as_str().trim().to_string();
                if number == "0" || number.starts_with('0') {
                    continue;
                }
                if is_measurement_or_non_part_context(
                    text,
                    num_m.start(),
                    num_m.end(),
                    &number,
                    name_m.as_str(),
                ) {
                    continue;
                }
                let name = trim_legend_name(name_m.as_str());
                if is_valid_part_name(&name) {
                    ranges.push((num_m.start(), name_m.end()));
                    push_unique_part(&mut parts, number, name);
                }
            }
        }
    }

    ranges.sort_by_key(|&(s, _)| s);
    (parts, ranges)
}

/// 仅提取行内式引用（如 `阀芯3`、`驱动电机（2）`），严禁跨换行错配多列表格单元格，严格过滤温度/百分比/日期/不匹配括号等伪标号
fn extract_inline_designators(text: &str) -> Vec<PartDesignator> {
    let mut parts = Vec::new();
    let Ok(inline_re) = regex::Regex::new(
        r"([\u4e00-\u9fa5]{2,14})[ \t]*(?:[（(][ \t]*(\d{1,3})[ \t]*[）)]|(\d{1,3}))",
    ) else {
        return parts;
    };
    for cap in inline_re.captures_iter(text) {
        let Some(name_m) = cap.get(1) else {
            continue;
        };
        let (num_m, is_parenthesized) = match (cap.get(2), cap.get(3)) {
            (Some(m), _) => (m, true),
            (None, Some(m)) => (m, false),
            _ => continue,
        };
        let number = num_m.as_str().trim().to_string();
        let raw_name = name_m.as_str();
        if is_measurement_or_non_part_context(text, num_m.start(), num_m.end(), &number, raw_name) {
            continue;
        }
        if !is_parenthesized {
            if let Ok(n_val) = number.parse::<u32>() {
                let is_typical_subpart = n_val >= 100 && (n_val % 100) <= 20;
                if n_val > 35 && !is_typical_subpart {
                    continue;
                }
            }
        }
        let cleaned_name = strip_inline_prefixes(raw_name);
        push_unique_part(&mut parts, number, cleaned_name);
    }
    parts
}

/// 将图例定义段从说明书正文中剥离，避免“图例中写过”误算作“正文具体实施方式已提及”
fn strip_legend_ranges(text: &str, ranges: &[(usize, usize)]) -> String {
    if ranges.is_empty() {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0usize;
    for &(start, end) in ranges {
        if let Some(chunk) = text.get(cursor..start) {
            out.push_str(chunk);
            out.push(' ');
            if text.is_char_boundary(end) {
                cursor = end;
            }
        }
    }
    if let Some(tail) = text.get(cursor..) {
        out.push_str(tail);
    }
    out
}

/// 判断两个部件名是否属于实质性异名（排除「散热板」vs「第一散热板」或前后缀包含关系）
fn names_conflict(a: &str, b: &str) -> bool {
    if a == b || a.contains(b) || b.contains(a) {
        return false;
    }
    // 共有至少 2 个汉字视为同一部件的简称/全称变体（如「导热散热片」vs「散热片结构」）
    let common_han = a
        .chars()
        .filter(|c| ('\u{4e00}'..='\u{9fff}').contains(c) && b.contains(*c))
        .count();
    common_han < 2
}

fn collect_numeral_snippets(claims: &str, desc: &str, number: &str, name: &str) -> Vec<String> {
    let mut snippets = Vec::new();
    let combined = [("[权要] ", claims), ("[说明书] ", desc)];
    let pattern_plain = format!("{name}{number}");
    let pattern_paren_cn = format!("{name}（{number}）");
    let pattern_paren_en = format!("{name}({number})");
    let pattern_dash = format!("{number}-{name}");
    let pattern_enum = format!("{number}、{name}");
    let pattern_colon = format!("{number}：{name}");

    for (prefix, src) in combined {
        for raw_seg in src.split(['。', '；', '\n']) {
            let seg = raw_seg.trim();
            if seg.len() < 4 {
                continue;
            }
            let matched = seg.contains(&pattern_plain)
                || seg.contains(&pattern_paren_cn)
                || seg.contains(&pattern_paren_en)
                || seg.contains(&pattern_dash)
                || seg.contains(&pattern_enum)
                || seg.contains(&pattern_colon)
                || (seg.contains(name)
                    && seg
                        .split(|c: char| !c.is_ascii_digit())
                        .any(|tok| tok == number));
            if matched {
                let preview: String = seg.chars().take(110).collect();
                let entry = format!("{prefix}{preview}");
                if !snippets.contains(&entry) {
                    snippets.push(entry);
                }
                if snippets.len() >= 5 {
                    return snippets;
                }
            }
        }
    }
    snippets
}

/// 交底书 / 权利要求 / 附图标号一致性静态核查器（v0.9.66）。
///
/// 确定性检出三类专利撰写低级错误：
/// 1. `FIGURE_OR_LEGEND_ONLY`：图中有号（附图或图例说明已列出），但权要与正文均未引用；
/// 2. `TEXT_ONLY_MISSING_IN_FIGURE`：正文或权要引用了标号，但附图或图例说明中缺失；
/// 3. `NAME_INCONSISTENT_FOR_SAME_NUMERAL`：同一标号在全文前后对应了互相冲突的部件名称。
pub fn check_numeral_consistency(
    claims_text: &str,
    description_text: &str,
    drawing_parts: Option<&[PartDesignator]>,
) -> NumeralConsistencyReport {
    use std::collections::BTreeMap;

    #[derive(Default)]
    struct Acc {
        names: Vec<String>,
        in_claims: bool,
        in_description: bool,
        in_legend: bool,
        in_drawing: bool,
    }

    let mut map: BTreeMap<String, Acc> = BTreeMap::new();
    let add_name = |acc: &mut Acc, name: &str| {
        if !name.is_empty() && !acc.names.iter().any(|n| n == name) {
            acc.names.push(name.to_string());
        }
    };

    // 1. 说明书图例定义（标号说明）
    let (legend_parts, legend_ranges) = extract_legend_designators_with_ranges(description_text);
    for p in &legend_parts {
        let acc = map.entry(p.number.clone()).or_default();
        acc.in_legend = true;
        add_name(acc, &p.name);
    }

    // 2. 剥离图例后的说明书正文行内引用
    let desc_body_only = strip_legend_ranges(description_text, &legend_ranges);
    for p in extract_inline_designators(&desc_body_only) {
        let acc = map.entry(p.number.clone()).or_default();
        acc.in_description = true;
        add_name(acc, &p.name);
    }

    // 3. 权利要求书中的标号（行内式 + 偶发图例式）
    for p in extract_part_designators(claims_text) {
        let acc = map.entry(p.number.clone()).or_default();
        acc.in_claims = true;
        add_name(acc, &p.name);
    }

    // 3b. 对已在图例中明确定义的部件标号（含 40/50/51/130/510/622 等），在正文与权利要求中直接核验 `名称+标号` 或 `名称(标号)`，避免被行内启发式 `n_val > 35` 误筛
    for p in &legend_parts {
        let p_plain = format!("{}{}", p.name, p.number);
        let p_cn = format!("{}（{}）", p.name, p.number);
        let p_en = format!("{}({})", p.name, p.number);
        if let Some(acc) = map.get_mut(&p.number) {
            if !acc.in_description
                && (desc_body_only.contains(&p_plain)
                    || desc_body_only.contains(&p_cn)
                    || desc_body_only.contains(&p_en))
            {
                acc.in_description = true;
            }
            if !acc.in_claims
                && (claims_text.contains(&p_plain)
                    || claims_text.contains(&p_cn)
                    || claims_text.contains(&p_en))
            {
                acc.in_claims = true;
            }
        }
    }

    // 4. 可选的附图视觉识别标号清单
    let has_drawing_source = drawing_parts.is_some_and(|list| !list.is_empty());
    if let Some(d_parts) = drawing_parts {
        for p in d_parts {
            let acc = map.entry(p.number.clone()).or_default();
            acc.in_drawing = true;
            add_name(acc, &p.name);
        }
    }

    // 检查是否存在“图例或附图”基准源（否则不强报 TEXT_ONLY_MISSING_IN_FIGURE 避免对无图例纯权要误报）
    let has_figure_or_legend_baseline = !legend_parts.is_empty() || has_drawing_source;

    let mut parts_out: Vec<NumeralPartSummary> = Vec::new();
    let mut issues: Vec<NumeralConsistencyIssue> = Vec::new();

    // 按数值大小升序排列标号（如 1, 2, 10 而非 1, 10, 2）
    let mut entries: Vec<(String, Acc)> = map.into_iter().collect();
    entries.sort_by_key(|(num, _)| (num.parse::<u32>().unwrap_or(u32::MAX), num.clone()));

    for (num, acc) in entries {
        let primary_name = acc
            .names
            .first()
            .cloned()
            .unwrap_or_else(|| format!("部件{num}"));
        let snippets = collect_numeral_snippets(claims_text, description_text, &num, &primary_name);
        let mention_count = snippets.len().max(
            usize::from(acc.in_claims)
                + usize::from(acc.in_description)
                + usize::from(acc.in_legend)
                + usize::from(acc.in_drawing),
        );

        // 错误类 1：同一标号前后部件名称不一致
        if acc.names.len() >= 2 {
            let mut conflicting_pair: Option<(String, String)> = None;
            'outer: for i in 0..acc.names.len() {
                for j in (i + 1)..acc.names.len() {
                    if names_conflict(&acc.names[i], &acc.names[j]) {
                        conflicting_pair = Some((acc.names[i].clone(), acc.names[j].clone()));
                        break 'outer;
                    }
                }
            }
            if let Some((n1, n2)) = conflicting_pair {
                issues.push(NumeralConsistencyIssue {
                    code: "NAME_INCONSISTENT_FOR_SAME_NUMERAL".to_string(),
                    severity: "error".to_string(),
                    numeral: num.clone(),
                    title: format!("标号 {num} 前后名称不一致"),
                    message: format!(
                        "同一附图标记「{num}」在文本中分别对应了「{n1}」与「{n2}」，涉嫌违反专利法实施细则关于同一附图标记仅代表同一组成部分的要求。"
                    ),
                    suggestion: format!(
                        "请核对附图并统一标号「{num}」的标准部件名称，或为不同部件分配独立标号。"
                    ),
                });
            }
        }

        // 错误类 2：图中有号正文未提
        let in_figure_side = acc.in_legend || acc.in_drawing;
        let in_text_side = acc.in_claims || acc.in_description;
        if in_figure_side && !in_text_side {
            issues.push(NumeralConsistencyIssue {
                code: "FIGURE_OR_LEGEND_ONLY".to_string(),
                severity: "warning".to_string(),
                numeral: num.clone(),
                title: format!("标号 {num}（{primary_name}）图中有号但正文未提及"),
                message: format!(
                    "附图/附图标记说明中列出了「{num}-{primary_name}」，但在权利要求书与具体实施方式正文中未检索到该标号的引用。"
                ),
                suggestion: format!(
                    "请在具体实施方式中补充「{primary_name}（{num}）」的结构与连接关系说明，或从附图及图例中删除多余标号。"
                ),
            });
        }

        // 错误类 3：正文有号附图/图例缺失
        if has_figure_or_legend_baseline && in_text_side && !in_figure_side {
            issues.push(NumeralConsistencyIssue {
                code: "TEXT_ONLY_MISSING_IN_FIGURE".to_string(),
                severity: "error".to_string(),
                numeral: num.clone(),
                title: format!("标号 {num}（{primary_name}）正文有号但附图/图例缺失"),
                message: format!(
                    "权利要求书或说明书正文中引用了「{primary_name}（{num}）」，但附图标记说明/附图清单中缺失标号「{num}」。"
                ),
                suggestion: format!(
                    "请在附图标记说明中补录「{num}-{primary_name}」并确认对应附图已清晰标出数字 {num}。"
                ),
            });
        }

        parts_out.push(NumeralPartSummary {
            number: num,
            name: primary_name,
            all_names: acc.names,
            in_claims: acc.in_claims,
            in_description: acc.in_description,
            in_legend: acc.in_legend,
            in_drawing: acc.in_drawing,
            mention_count,
            snippets,
        });
    }

    let total_numerals = parts_out.len();
    let issue_count = issues.len();
    let passed = issue_count == 0;
    let summary = if total_numerals == 0 {
        "未在文本中识别到机械/电气部件标号。".to_string()
    } else if passed {
        format!("共核查 {total_numerals} 个部件标号，图文标号与部件名称完全一致，未发现低级错误。")
    } else {
        format!(
            "共核查 {total_numerals} 个部件标号，检出 {issue_count} 处一致性异常，请在提交前核对修正。"
        )
    };

    NumeralConsistencyReport {
        passed,
        total_numerals,
        issue_count,
        summary,
        parts: parts_out,
        issues,
    }
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

    #[test]
    fn numeral_consistency_detects_all_three_error_classes() {
        let claims = "1. 一种液压隔膜阀，其特征在于包括：阀体（1）、与阀体1配合的膜片2，以及驱动膜片2运动的电磁线圈4。";
        let description = "附图标记说明：1-阀体；2-膜片；3-防尘盖。\n\
                           具体实施方式：工作时，阀体1内部安装有膜片2，且散热支架1固定在底座上方，电磁线圈4驱动膜片2开合。";

        let report = check_numeral_consistency(claims, description, None);
        assert!(!report.passed, "存在三类标号错误必须拦截");

        let codes: Vec<&str> = report.issues.iter().map(|i| i.code.as_str()).collect();
        assert!(
            codes.contains(&"FIGURE_OR_LEGEND_ONLY"),
            "必须检出图中有号正文未提（3-防尘盖），got: {:?}",
            report.issues
        );
        assert!(
            codes.contains(&"TEXT_ONLY_MISSING_IN_FIGURE"),
            "必须检出正文有号图例缺失（电磁线圈4），got: {:?}",
            report.issues
        );
        assert!(
            codes.contains(&"NAME_INCONSISTENT_FOR_SAME_NUMERAL"),
            "必须检出同一标号异名（1: 阀体 vs 散热支架），got: {:?}",
            report.issues
        );

        // 规范无误文本必须零误报通过
        let clean_claims = "1. 一种隔膜阀，包括阀体1与膜片2。";
        let clean_desc = "附图标记说明：1-阀体；2-膜片。\n具体实施方式：阀体1内腔设有膜片2。";
        let clean_report = check_numeral_consistency(clean_claims, clean_desc, None);
        assert!(
            clean_report.passed,
            "规范文本不应误报: {:?}",
            clean_report.issues
        );
        assert_eq!(clean_report.total_numerals, 2);
    }

    #[test]
    fn real_world_cnipa_patterns_reject_units_and_support_all_three_legend_formats() {
        // 1. 温度/百分比/日期/小数绝不能误判为机械部件标号
        let thermo_desc =
            "余热锅炉1产生近300℃蒸汽，经过两级换热后烟气温度降为75℃，余热回收效率达到90%，\
                           整个机组可以节能8%，持续运行12小时，申请日为2021年12月29日，起点为0。";
        let thermo_report = check_numeral_consistency("", thermo_desc, None);
        let nums: Vec<&str> = thermo_report
            .parts
            .iter()
            .map(|p| p.number.as_str())
            .collect();
        assert!(nums.contains(&"1"), "真实标号 1 应保留: {:?}", nums);
        for bad in ["0", "8", "12", "75", "90", "300", "29"] {
            assert!(
                !nums.contains(&bad),
                "物理数值/日期 {bad} 不得被误判为部件标号: {:?}",
                thermo_report.parts
            );
        }

        // 2. 「名称+数字、名称+数字」图例格式绝不能错位成 `1、热风炉`
        let c_claims = "1. 一种再生装置，包括再生滚筒(1)、热风炉(2)、再生燃烧器(3)。";
        let c_desc = "附图标记\n再生滚筒1、热风炉2、再生燃烧器3。\n\n具体实施方式\n所述再生滚筒1一侧设置热风炉2，热风炉2上并安装再生燃烧器3。";
        let c_report = check_numeral_consistency(c_claims, c_desc, None);
        assert!(
            c_report.passed,
            "名称+数字图例格式不应产生错位误报: {:?}",
            c_report.issues
        );
        assert!(
            c_report
                .parts
                .iter()
                .any(|p| p.number == "3" && p.name == "再生燃烧器"),
            "并安装前缀应剥离干净: {:?}",
            c_report.parts
        );

        // 3. 「数字直连中文名称」图例格式（无分隔符）应完整识别
        let b_claims = "1. 一种润滑装置，包括加注装置(1)、转动轴承(2)、量具(3)。";
        let b_desc = "附图标记的含义：1加注装置；2转动轴承；3量具。\n\n具体实施方式\n加注装置(1)位于转动轴承(2)上方，量具(3)位于下方。";
        let b_report = check_numeral_consistency(b_claims, b_desc, None);
        assert!(
            b_report.passed,
            "无分隔符图例格式不应误报缺失: {:?}",
            b_report.issues
        );
    }

    #[test]
    fn double_newline_two_column_cnipa_table_parses_cleanly_without_mis_pairing() {
        let claims =
            "1. 一种电机冷却系统，包括定子总成(10)、定子铁芯(100)、槽绝缘纸(400)与电机(20)。";
        let desc = "附图标号说明：\n\n标号\n\n名称\n\n标号\n\n名称\n\n10\n\n定子总成\n\n400\n\n槽绝缘纸\n\n20\n\n电机\n\n100\n\n定子铁芯\n\n具体实施方式\n本实施例公开定子总成10，其内部包括定子铁芯100与槽绝缘纸400，安装于电机20壳体内。";
        let report = check_numeral_consistency(claims, desc, None);
        assert!(
            report.passed,
            "双换行多列表格标号不得错配产生误报: {:?}",
            report.issues
        );
        let by_num: std::collections::HashMap<&str, &str> = report
            .parts
            .iter()
            .map(|p| (p.number.as_str(), p.name.as_str()))
            .collect();
        assert_eq!(by_num.get("10"), Some(&"定子总成"));
        assert_eq!(by_num.get("400"), Some(&"槽绝缘纸"));
        assert_eq!(by_num.get("20"), Some(&"电机"));
        assert_eq!(by_num.get("100"), Some(&"定子铁芯"));
        assert!(
            !report
                .parts
                .iter()
                .any(|p| p.name == "名称" || p.name == "标号"),
            "表头「名称」「标号」绝不得被当成部件名: {:?}",
            report.parts
        );
    }

    #[test]
    fn formula_prose_numbers_are_not_misclassified_as_drawing_numerals() {
        let claims = "1. 一种微压氧舱控制方法，其中气体质量指标为两者相加后除以2，含氧量测量值为10，气压最大值为100。";
        let desc = "具体实施方式：将第一参数与第二参数两者相加后除以2，记录当前测量值为10，阈值最大值为100。";
        let report = check_numeral_consistency(claims, desc, None);
        assert!(
            report.parts.is_empty(),
            "公式/测量值叙述中的数值不应误判为附图标号: {:?}",
            report.parts
        );
    }
}
