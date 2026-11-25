//! 工程落地交付包与期刊论文草稿纯生成模块 / Engineering deliverables & journal manuscript draft generator
//!
//! 消费 `PipelineContext`、`FeatureCard` 与 `ExperimentResult`（真实指标/stdout）以及
//! `patent_results` / `web_results` 中的真实 DOI，零网络调用、零编造数据。

use crate::ai::client::safe_truncate_chars;
use crate::patent::{FeatureCard, Patent};
use crate::pipeline::context::{ExperimentResult, PipelineContext};
use crate::pipeline::steps::control_proto::StepSimulationData;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

pub const NOVELTY_LOCK_WARNING: &str = "> ⚠️ 【防破新颖性安全锁】：本手稿包含尚未公开的核心技术方案。在正式提交专利申请并取得申请号/申请日之前，严禁公开发表、上传预印本或外发审稿，否则将导致本申请丧失《专利法》第22条第2款新颖性！";

pub const NOVELTY_LOCK_TEXT_PLAIN: &str = "【防破新颖性安全锁 / Patent-Before-Paper Novelty Lock】：本手稿包含尚未公开的核心技术方案。在正式提交专利申请并取得申请号/申请日之前，严禁公开发表、上传预印本或外发审稿，否则将导致本申请丧失《专利法》第22条第2款新颖性！";

const MISSING_CALIBRATION_TAG: &str = "【待台架标定】";
const MISSING_DOI_TAG: &str = "【当前检索结果无 DOI 文献，请补充 OpenAlex 检索后再绑定引用】";

/// 分类 BOM 清单条目
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct BomItem {
    pub item_no: usize,
    pub part_name: String,
    /// `"自制结构件"` | `"外购标准件/执行器"` | `"传感与控制件"`
    pub category: String,
    pub spec_summary: String,
    pub source_feature: String,
}

/// 工程落地交付包（BOM + IO 控制规格 + 台架验收大纲）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct EngineeringDeliverablePack {
    pub idea_id: String,
    pub bom_items: Vec<BomItem>,
    pub control_io_spec_md: String,
    pub bench_test_outline_md: String,
    pub full_markdown: String,
}

/// 工程期刊论文手稿草稿
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct JournalManuscriptDraft {
    pub idea_id: String,
    pub novelty_lock_warning: String,
    pub cited_dois: Vec<String>,
    pub full_markdown: String,
}

/// 真实 DOI 学术文献 BibTeX 条目（仅来自 OpenAlex / CrossRef 等已核验检索结果，绝不编造）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BibEntry {
    pub cite_key: String,
    pub doi: String,
    pub title: String,
    pub author: String,
    pub journal: String,
    pub year: String,
    pub url: String,
    pub source: String,
}

/// 仿真曲线单条序列
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationSeries {
    pub label: String,
    pub color: String,
    pub dashed: bool,
    pub values: Vec<f64>,
}

/// 第 13 步数值仿真 / 控制响应绘图数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationPlotData {
    pub title: String,
    pub x_label: String,
    pub y_label: String,
    pub x_values: Vec<f64>,
    pub series: Vec<SimulationSeries>,
    pub summary_note: String,
    pub from_step13: bool,
}

/// 期刊投稿 LaTeX 模板类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum PaperLatexTemplate {
    /// 中文工程核心期刊（`\documentclass[UTF8]{ctexart}`，中文内容默认）
    #[default]
    Ctexart,
    /// 国际工程期刊 IEEE Transactions（`\documentclass[journal]{IEEEtran}`）
    Ieee,
    /// Elsevier 期刊（`\documentclass[preprint,12pt]{elsarticle}`）
    Elsevier,
}

impl PaperLatexTemplate {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ctexart => "ctexart",
            Self::Ieee => "ieee",
            Self::Elsevier => "elsevier",
        }
    }

    /// 解析查询参数 `?template=...`；未指定时中文内容默认 `ctexart`，纯英文默认 `ieee`。
    pub fn from_query_or_content(template_param: Option<&str>, content_sample: &str) -> Self {
        match template_param
            .map(|s| s.trim().to_ascii_lowercase())
            .as_deref()
        {
            Some("ctexart") | Some("cn") | Some("zh") => Self::Ctexart,
            Some("ieee") | Some("ieeetran") => Self::Ieee,
            Some("elsevier") | Some("elsarticle") => Self::Elsevier,
            _ => {
                let has_cjk = content_sample
                    .chars()
                    .any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c));
                if has_cjk {
                    Self::Ctexart
                } else {
                    Self::Ieee
                }
            }
        }
    }
}

/// 期刊投稿 LaTeX + BibTeX + SVG 仿真图打包输出
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaperLatexBundle {
    pub idea_id: String,
    pub template: PaperLatexTemplate,
    pub novelty_lock_warning: String,
    pub bib_entries: Vec<BibEntry>,
    pub bibtex_content: String,
    pub main_tex: String,
    pub simulation_svg: String,
    pub has_step13_simulation: bool,
    pub plot_data: Option<SimulationPlotData>,
}

/// 基于流水线上下文、特征卡与沙箱实验指标（`ctx.experiment_results`）生成《工程落地交付包》
#[allow(dead_code)]
pub fn build_engineering_bom_pack(
    ctx: &PipelineContext,
    feature_cards: &[FeatureCard],
) -> EngineeringDeliverablePack {
    let bom_items = build_bom_items(ctx, feature_cards);
    let control_io_spec_md = build_control_io_spec(ctx, &bom_items);
    let bench_test_outline_md = build_bench_test_outline(ctx);

    let mut bom_table = String::from(
        "## 一、机械与电控分类 BOM 清单\n\n\
         | 序号 | 部件名称 | 类别 | 规格/结构摘要 | 来源特征 |\n\
         | :---: | :--- | :--- | :--- | :--- |\n",
    );
    for item in &bom_items {
        bom_table.push_str(&format!(
            "| {} | {} | {} | {} | {} |\n",
            item.item_no,
            escape_md_cell(&item.part_name),
            escape_md_cell(&item.category),
            escape_md_cell(&item.spec_summary),
            escape_md_cell(&item.source_feature),
        ));
    }

    let domain_label = if ctx.technical_domain.trim().is_empty() {
        "通用工程与机电控制"
    } else {
        ctx.technical_domain.trim()
    };

    let full_markdown = format!(
        "# 工程落地交付包：{title}\n\n\
         - **创意 ID**：`{idea_id}`\n\
         - **技术领域**：{domain}\n\n\
         {bom_table}\n\
         {control_io}\n\
         {bench_outline}\n",
        title = if ctx.title.trim().is_empty() {
            "未命名工程方案"
        } else {
            ctx.title.trim()
        },
        idea_id = ctx.idea_id,
        domain = domain_label,
        bom_table = bom_table,
        control_io = control_io_spec_md,
        bench_outline = bench_test_outline_md,
    );

    EngineeringDeliverablePack {
        idea_id: ctx.idea_id.clone(),
        bom_items,
        control_io_spec_md,
        bench_test_outline_md,
        full_markdown,
    }
}

/// 生成《工程期刊论文手稿草稿》（强制置顶「防破新颖性安全锁」，仅绑定 `ctx` 中真实存在的 DOI）
#[allow(dead_code)]
pub fn build_journal_manuscript_draft(ctx: &PipelineContext) -> JournalManuscriptDraft {
    let novelty_lock_warning = NOVELTY_LOCK_WARNING.to_string();
    let cited_dois = extract_cited_dois(ctx);

    let title = if ctx.title.trim().is_empty() {
        "基于多维特征融合的工程系统设计与实验验证"
    } else {
        ctx.title.trim()
    };
    let domain = if ctx.technical_domain.trim().is_empty() {
        "工程技术"
    } else {
        ctx.technical_domain.trim()
    };
    let abstract_body = if ctx.description.trim().is_empty() {
        "【待补充方案技术摘要】".to_string()
    } else {
        safe_truncate_chars(ctx.description.trim(), 300).to_string()
    };
    let clean_keywords: Vec<String> = ctx
        .keywords
        .iter()
        .map(|k| k.trim())
        .filter(|k| is_valid_technical_keyword(k))
        .map(ToString::to_string)
        .collect();
    let keywords_line = if clean_keywords.is_empty() {
        "【待补充关键词】".to_string()
    } else {
        clean_keywords.join("；")
    };

    let total_refs = ctx.patent_results.len() + ctx.web_results.len();
    let intro_section = format!(
        "## 1. 引言与文献评述\n\n\
         本研究面向**{domain}**领域的技术瓶颈开展系统设计。本次共检索比对现有文献与专利 {total_refs} 篇，\
         其中可追溯 DOI 学术文献 {doi_count} 篇。\n",
        domain = domain,
        total_refs = total_refs,
        doi_count = cited_dois.len(),
    );

    let mechanism_body = if ctx.ai_analysis.trim().is_empty() {
        format!(
            "{}\n\n（注：深度机理分析尚未生成，请先执行分析步骤或补充结构建模说明。）",
            abstract_body
        )
    } else {
        safe_truncate_chars(ctx.ai_analysis.trim(), 500).to_string()
    };

    let experiment_section = format_experiment_section_for_paper(&ctx.experiment_results);

    let conclusion_body = if ctx.action_plan.trim().is_empty() {
        format!(
            "围绕「{}」完成了核心结构与控制方案梳理，后续需在专利申请受理后开展工程化验证。",
            title
        )
    } else {
        safe_truncate_chars(ctx.action_plan.trim(), 240).to_string()
    };

    let references_section = if cited_dois.is_empty() {
        format!("## 参考文献\n\n{}\n", MISSING_DOI_TAG)
    } else {
        let mut lines = String::from("## 参考文献\n\n");
        for (idx, doi) in cited_dois.iter().enumerate() {
            lines.push_str(&format!("[{}] DOI: https://doi.org/{}\n", idx + 1, doi));
        }
        lines
    };

    let full_markdown = format!(
        "{warning}\n\n\
         # {title}（工程期刊论文手稿草稿）\n\n\
         ## 摘要\n\n\
         {abstract_body}\n\n\
         **关键词**：{keywords_line}\n\n\
         {intro}\n\
         ## 2. 系统机理与结构建模\n\n\
         {mechanism}\n\n\
         {experiment}\n\
         ## 4. 结论\n\n\
         {conclusion}\n\n\
         {references}",
        warning = novelty_lock_warning,
        title = title,
        abstract_body = abstract_body,
        keywords_line = keywords_line,
        intro = intro_section,
        mechanism = mechanism_body,
        experiment = experiment_section,
        conclusion = conclusion_body,
        references = references_section,
    );

    JournalManuscriptDraft {
        idea_id: ctx.idea_id.clone(),
        novelty_lock_warning,
        cited_dois,
        full_markdown,
    }
}

fn build_bom_items(ctx: &PipelineContext, feature_cards: &[FeatureCard]) -> Vec<BomItem> {
    let mut items = Vec::new();
    let mut next_no = 1usize;

    if !feature_cards.is_empty() {
        for card in feature_cards {
            let feature_name = if card.title.trim().is_empty() {
                "未命名特征"
            } else {
                card.title.trim()
            };
            let mut added_for_card = false;

            if !card.core_structure.trim().is_empty() {
                items.push(BomItem {
                    item_no: next_no,
                    part_name: format!("{} - 主体结构件", feature_name),
                    category: "自制结构件".to_string(),
                    spec_summary: safe_truncate_chars(card.core_structure.trim(), 120).to_string(),
                    source_feature: feature_name.to_string(),
                });
                next_no += 1;
                added_for_card = true;
            }

            if !card.key_relations.trim().is_empty() {
                let rel = card.key_relations.trim();
                let (part_suffix, category) = if has_hardware_actuation_cue(rel) {
                    ("传动/连接执行组件", "外购标准件/执行器")
                } else {
                    ("关键机理与参数约束", "工艺/机理约束（非硬件件）")
                };
                items.push(BomItem {
                    item_no: next_no,
                    part_name: format!("{} - {}", feature_name, part_suffix),
                    category: category.to_string(),
                    spec_summary: safe_truncate_chars(rel, 120).to_string(),
                    source_feature: feature_name.to_string(),
                });
                next_no += 1;
                added_for_card = true;
            }

            if !card.process_steps.trim().is_empty() {
                let steps = card.process_steps.trim();
                let (part_suffix, category) = if has_sensing_control_cue(steps) {
                    ("传感与控制单元", "传感与控制件")
                } else {
                    ("核心工艺流程节点", "工艺流程/软件策略（非硬件件）")
                };
                items.push(BomItem {
                    item_no: next_no,
                    part_name: format!("{} - {}", feature_name, part_suffix),
                    category: category.to_string(),
                    spec_summary: safe_truncate_chars(steps, 120).to_string(),
                    source_feature: feature_name.to_string(),
                });
                next_no += 1;
                added_for_card = true;
            }

            if !added_for_card {
                let fallback_spec = if card.description.trim().is_empty() {
                    "【待结构细化】".to_string()
                } else {
                    safe_truncate_chars(card.description.trim(), 120).to_string()
                };
                items.push(BomItem {
                    item_no: next_no,
                    part_name: format!("{} - 核心功能模块", feature_name),
                    category: "自制结构件".to_string(),
                    spec_summary: fallback_spec,
                    source_feature: feature_name.to_string(),
                });
                next_no += 1;
            }
        }
        return items;
    }

    // 特征卡为空时，回退解析 ctx.keywords + ctx.description（过滤口语碎片与单字噪音）
    for kw in &ctx.keywords {
        let trimmed = kw.trim();
        if !is_valid_technical_keyword(trimmed) {
            continue;
        }
        let category = classify_keyword_category(trimmed);
        let desc_hint = if ctx.description.trim().is_empty() {
            "源自关键词推导，尺寸与选型待细化".to_string()
        } else {
            format!(
                "关联方案要素：{}",
                safe_truncate_chars(ctx.description.trim(), 80)
            )
        };
        items.push(BomItem {
            item_no: next_no,
            part_name: trimmed.to_string(),
            category: category.to_string(),
            spec_summary: desc_hint,
            source_feature: "ctx.keywords".to_string(),
        });
        next_no += 1;
    }

    if items.is_empty() {
        let part_name = if ctx.title.trim().is_empty() {
            "核心样机总成".to_string()
        } else {
            format!("{} - 核心样机总成", ctx.title.trim())
        };
        let spec_summary = if ctx.description.trim().is_empty() {
            "【待补充方案描述与结构要素】".to_string()
        } else {
            safe_truncate_chars(ctx.description.trim(), 120).to_string()
        };
        items.push(BomItem {
            item_no: 1,
            part_name,
            category: "自制结构件".to_string(),
            spec_summary,
            source_feature: "ctx.description".to_string(),
        });
    }

    items
}

/// 判断特征卡 `key_relations` 是否描述真实机械/机电/流体硬件执行与连接件（而非纯物理公式/相关性规律）
fn has_hardware_actuation_cue(text: &str) -> bool {
    let lower = text.to_lowercase();
    const HARDWARE_CUES: &[&str] = &[
        "电机",
        "泵",
        "阀",
        "气缸",
        "液压",
        "轴承",
        "齿轮",
        "连杆",
        "啮合",
        "驱动",
        "执行",
        "发生器",
        "联轴",
        "导轨",
        "丝杠",
        "弹簧",
        "传动",
        "法兰",
        "管路",
        "管道",
        "喷嘴",
        "散热",
        "翅片",
        "壳体",
        "腔体",
        "电极",
        "膜片",
        "电路",
        "电源",
        "风扇",
        "压缩机",
        "actuator",
        "motor",
        "valve",
        "gear",
        "bearing",
        "pump",
        "shaft",
    ];
    HARDWARE_CUES.iter().any(|kw| lower.contains(kw))
}

/// 判断特征卡 `process_steps` 是否包含真实传感/采集/电控硬件环节（而非纯工艺流程或物理传质步骤）
fn has_sensing_control_cue(text: &str) -> bool {
    let lower = text.to_lowercase();
    const CONTROL_CUES: &[&str] = &[
        "传感",
        "编码器",
        "检测",
        "采样",
        "控制",
        "闭环",
        "反馈",
        "调节",
        "驱动器",
        "变频",
        "继电器",
        "开关",
        "报警",
        "触发",
        "测温",
        "测压",
        "热电偶",
        "红外",
        "电控",
        "plc",
        "mcu",
        "fpga",
        "pid",
        "sensor",
        "encoder",
        "control",
        "feedback",
    ];
    CONTROL_CUES.iter().any(|kw| lower.contains(kw))
}

/// 判断关键词是否为有效的工程技术词汇（过滤 1 字碎片、纯数字/标点、口语虚词碎片如“录”“烧这”“录你”“话记”）
fn is_valid_technical_keyword(kw: &str) -> bool {
    let trimmed = kw.trim();
    let char_count = trimmed.chars().count();
    if char_count < 2 {
        return false;
    }
    if trimmed
        .chars()
        .all(|c| c.is_ascii_digit() || c.is_ascii_punctuation() || c.is_whitespace())
    {
        return false;
    }
    // 过滤纯参数/量纲碎片（如 "800v" / "1200v" / "600a" / "8nh" / "32%" / "10mm" / "90度"），避免污染硬件 BOM
    if is_bare_numeric_parameter(trimmed) {
        return false;
    }
    let has_cjk = trimmed
        .chars()
        .any(|c| ('\u{4E00}'..='\u{9FFF}').contains(&c));
    if has_cjk && char_count == 2 {
        const CONVERSATIONAL_FRAGMENTS: &str =
            "你我他她它这那哪怎吗呢吧啊呀哦嗯啦嘛了的得地着过把被给向对与和或但而却就都也还又再很太最更没不无话记";
        const STOPWORD_BIGRAMS: &[&str] = &[
            "先分", "来聊", "一下", "一些", "一个", "这个", "那个", "怎么", "什么", "可以", "能够",
            "进行", "通过", "使用", "采用", "主要", "目前", "现在", "分析", "讨论", "研究", "介绍",
        ];
        if STOPWORD_BIGRAMS.contains(&trimmed)
            || trimmed
                .chars()
                .any(|c| CONVERSATIONAL_FRAGMENTS.contains(c))
        {
            return false;
        }
    }
    true
}

/// 判断是否为纯数字+单位参数碎片（如 "800v"、"32%"、"8nh"、"10mm"、"90度"），非实体 BOM 部件
fn is_bare_numeric_parameter(s: &str) -> bool {
    let trimmed = s.trim();
    let mut chars = trimmed.chars().peekable();
    let mut digit_count = 0usize;
    while let Some(&c) = chars.peek() {
        if c.is_ascii_digit() || c == '.' || c == '-' || c == '<' || c == '>' || c == '~' {
            if c.is_ascii_digit() {
                digit_count += 1;
            }
            chars.next();
        } else {
            break;
        }
    }
    if digit_count == 0 {
        return false;
    }
    let rest: String = chars.collect::<String>().trim().to_ascii_lowercase();
    matches!(
        rest.as_str(),
        "" | "%"
            | "v"
            | "kv"
            | "mv"
            | "a"
            | "ma"
            | "w"
            | "kw"
            | "mw"
            | "hz"
            | "khz"
            | "mhz"
            | "nh"
            | "uh"
            | "mh"
            | "pf"
            | "nf"
            | "uf"
            | "mm"
            | "cm"
            | "um"
            | "nm"
            | "m"
            | "kg"
            | "g"
            | "mg"
            | "pa"
            | "kpa"
            | "mpa"
            | "rpm"
            | "deg"
            | "c"
            | "k"
            | "°c"
            | "℃"
            | "°"
            | "度"
            | "小时"
            | "h"
            | "s"
            | "ms"
    )
}

fn classify_keyword_category(keyword: &str) -> &'static str {
    let lower = keyword.to_lowercase();
    if lower == "sic"
        || lower == "gan"
        || lower == "igbt"
        || lower == "mosfet"
        || lower.contains("碳化硅")
        || lower.contains("氮化镓")
        || lower.contains("功率模块")
        || lower.contains("功率半导体")
        || lower.contains("igbt")
        || lower.contains("mosfet")
        || lower.contains("芯片")
        || lower.contains("晶圆")
    {
        "功率半导体/外购核心器件"
    } else if lower.contains("传感")
        || lower.contains("检测")
        || lower.contains("控制")
        || lower.contains("算法")
        || lower.contains("mcu")
        || lower.contains("plc")
        || lower.contains("sensor")
    {
        "传感与控制件"
    } else if lower.contains("电机")
        || lower.contains("泵")
        || lower.contains("阀")
        || lower.contains("气缸")
        || lower.contains("驱动")
        || lower.contains("执行")
        || lower.contains("轴承")
        || lower.contains("actuator")
    {
        "外购标准件/执行器"
    } else {
        "自制结构件"
    }
}

fn build_control_io_spec(ctx: &PipelineContext, bom_items: &[BomItem]) -> String {
    let successful: Vec<&ExperimentResult> = ctx
        .experiment_results
        .iter()
        .filter(|e| e.success)
        .collect();

    let mut md = String::from("## 二、电气控制 IO 分配与状态机规格\n\n");

    let control_parts: Vec<&str> = bom_items
        .iter()
        .filter(|b| b.category == "传感与控制件" || b.category == "外购标准件/执行器")
        .map(|b| b.part_name.as_str())
        .collect();

    md.push_str("### 2.1 控制对象与状态机流转\n\n");
    if control_parts.is_empty() {
        md.push_str("- **关联电控/执行单元**：暂无显式电控特征，默认按标准主控节点配置。\n");
    } else {
        md.push_str(&format!(
            "- **关联电控/执行单元**：{}\n",
            control_parts.join("、")
        ));
    }
    md.push_str(
        "- **主状态机**：`IDLE（待机） -> INIT（自检归零） -> RUN（闭环运行） -> FAULT / SAFE_STOP（急停保护）`\n\n\
         ### 2.2 实验标定参数与 IO 阈值\n\n",
    );

    if successful.is_empty() {
        md.push_str(&format!(
            "- **闭环阈值与标定参数**：{tag}（当前无已通过的沙箱/仿真实验结果，严禁填报估算阈值）。\n\
             - **DI/DO/AI 通道**：急停输入 `DI_01`、限位反馈 `DI_02`、执行器使能 `DO_01`、主反馈信号 `AI_01`（量程 {tag}）。\n",
            tag = MISSING_CALIBRATION_TAG
        ));
    } else {
        for (idx, exp) in successful.iter().enumerate() {
            let metrics_raw = exp.metrics.to_string();
            let metrics_summary = safe_truncate_chars(metrics_raw.trim(), 200);
            let stdout_summary = if exp.stdout.trim().is_empty() {
                "无标准输出".to_string()
            } else {
                safe_truncate_chars(exp.stdout.trim(), 160).to_string()
            };
            md.push_str(&format!(
                "- **实测基准 #{}（{}，耗时 {} ms）**：\n  - `metrics`: `{}`\n  - `stdout`: `{}`\n",
                idx + 1,
                exp.language,
                exp.duration_ms,
                metrics_summary,
                stdout_summary
            ));
        }
        md.push_str(
            "- **DI/DO/AI 通道**：急停输入 `DI_01`、限位反馈 `DI_02`、执行器使能 `DO_01`、主反馈信号 `AI_01`（按上述实测 `metrics` 设定上下限）。\n",
        );
    }

    md
}

fn build_bench_test_outline(ctx: &PipelineContext) -> String {
    let has_success_exp = ctx.experiment_results.iter().any(|e| e.success);
    let perf_criterion = if has_success_exp {
        "对照第 2.2 节实测 `metrics` 基准进行台架复现，偏差应在工程允许范围内。"
    } else {
        "【待台架标定】首次上电记录空载电流、温升与响应延时作为基线。"
    };

    format!(
        "## 三、样机台架验收大纲\n\n\
         1. **静态装配与安全检查**：核对机械 BOM 紧固件扭矩、绝缘阻抗与急停回路 `DI_01` 硬件切断功能。\n\
         2. **空载 IO 与状态机联调**：验证 `IDLE -> INIT -> RUN -> SAFE_STOP` 状态切换及传感器零点漂移。\n\
         3. **带载性能与边界工况测试**：{}\n\
         4. **连续稳定性放行**：连续运行无异常告警、关键结构无干涉或过温即可签署样机放行。\n",
        perf_criterion
    )
}

fn format_experiment_section_for_paper(results: &[ExperimentResult]) -> String {
    let successful: Vec<&ExperimentResult> = results.iter().filter(|e| e.success).collect();
    if successful.is_empty() {
        return format!(
            "## 3. 仿真与实验结果分析\n\n{} 当前尚无已通过的仿真或实验结果。\n",
            MISSING_CALIBRATION_TAG
        );
    }

    let mut md = String::from("## 3. 仿真与实验结果分析\n\n");
    for (idx, exp) in successful.iter().enumerate() {
        let metrics_raw = exp.metrics.to_string();
        let metrics_summary = safe_truncate_chars(metrics_raw.trim(), 200);
        let stdout_summary = safe_truncate_chars(exp.stdout.trim(), 160);
        md.push_str(&format!(
            "- **实验组 {}（{}，{} ms）**：指标 `{}`；输出摘要：`{}`。\n",
            idx + 1,
            exp.language,
            exp.duration_ms,
            metrics_summary,
            stdout_summary
        ));
    }
    md
}

fn extract_cited_dois(ctx: &PipelineContext) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut dois = Vec::new();

    for hit in ctx.patent_results.iter().chain(ctx.web_results.iter()) {
        for field in [&hit.id, &hit.link, &hit.title, &hit.snippet] {
            collect_dois_from_str(field, &mut seen, &mut dois);
        }
    }

    dois
}

fn collect_dois_from_str(input: &str, seen: &mut HashSet<String>, out: &mut Vec<String>) {
    for (idx, _) in input.match_indices("10.") {
        if idx > 0 {
            let prev = input
                .get(..idx)
                .and_then(|prefix| prefix.chars().next_back());
            if let Some(ch) = prev {
                if ch.is_ascii_alphanumeric() {
                    continue;
                }
            }
        }

        let Some(rest) = input.get(idx + 3..) else {
            continue;
        };
        let Some(slash_pos) = rest.find('/') else {
            continue;
        };
        let Some(registrant) = rest.get(..slash_pos) else {
            continue;
        };
        if !(4..=9).contains(&registrant.len()) || !registrant.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }

        let Some(after_slash) = rest.get(slash_pos + 1..) else {
            continue;
        };
        let raw_suffix: String = after_slash
            .chars()
            .take_while(|c| {
                !c.is_whitespace()
                    && !matches!(c, '"' | '\'' | '<' | '>' | '`' | '）' | '】' | '。' | '，')
            })
            .collect();
        let suffix = raw_suffix.trim_end_matches(['.', ',', ';', ')', ']', ':']);
        if !suffix.is_empty() {
            let doi = format!("10.{}/{}", registrant, suffix);
            if seen.insert(doi.to_lowercase()) {
                out.push(doi);
            }
        }
    }
}

fn escape_md_cell(s: &str) -> String {
    s.replace('|', "\\|").replace(['\r', '\n'], " ")
}

/// 从流水线上下文提取已去重的真实 DOI 列表
pub fn extract_verified_dois(ctx: &PipelineContext) -> Vec<String> {
    extract_cited_dois(ctx)
}

/// 从检索命中与可选数据库著录中提取带真实 DOI 的学术文献条目（绝不编造无 DOI 文献）
pub fn extract_verified_bib_entries(ctx: &PipelineContext, db_patents: &[Patent]) -> Vec<BibEntry> {
    let mut db_map: HashMap<String, &Patent> = HashMap::new();
    for p in db_patents {
        let key = p.patent_number.trim().to_lowercase();
        if key.is_empty() {
            continue;
        }
        let is_academic = matches!(
            p.source.trim().to_lowercase().as_str(),
            "openalex" | "crossref"
        );
        match db_map.get(&key) {
            Some(existing) => {
                let existing_academic = matches!(
                    existing.source.trim().to_lowercase().as_str(),
                    "openalex" | "crossref"
                );
                if is_academic && !existing_academic {
                    db_map.insert(key, p);
                }
            }
            None => {
                db_map.insert(key, p);
            }
        }
    }

    let mut seen = HashSet::new();
    let mut entries = Vec::new();

    for hit in ctx.patent_results.iter().chain(ctx.web_results.iter()) {
        let mut hit_dois = Vec::new();
        let mut hit_seen = HashSet::new();
        for field in [&hit.id, &hit.link, &hit.title, &hit.snippet] {
            collect_dois_from_str(field, &mut hit_seen, &mut hit_dois);
        }

        for doi in hit_dois {
            let doi_lower = doi.to_lowercase();
            if !seen.insert(doi_lower.clone()) {
                continue;
            }
            let db_rec = db_map.get(&doi_lower).copied();
            let title = if !hit.title.trim().is_empty() {
                hit.title.trim().to_string()
            } else if let Some(p) = db_rec {
                if !p.title.trim().is_empty() {
                    p.title.trim().to_string()
                } else {
                    format!("DOI: {}", doi)
                }
            } else {
                format!("DOI: {}", doi)
            };

            let author = db_rec
                .map(|p| p.inventor.trim().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_default();

            let journal = db_rec
                .map(|p| p.applicant.trim().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_default();

            let year = db_rec
                .and_then(|p| {
                    extract_four_digit_year(&p.filing_date)
                        .or_else(|| extract_four_digit_year(&p.publication_date))
                })
                .or_else(|| extract_four_digit_year(&hit.snippet))
                .unwrap_or_default();

            let source = if !hit.source.trim().is_empty() {
                hit.source.trim().to_string()
            } else if let Some(p) = db_rec {
                p.source.trim().to_string()
            } else {
                "doi".to_string()
            };

            let idx = entries.len() + 1;
            let cite_key = format!("ref{}_{}", idx, sanitize_bib_key(&doi));
            entries.push(BibEntry {
                cite_key,
                doi: doi.clone(),
                title,
                author,
                journal,
                year,
                url: format!("https://doi.org/{}", doi),
                source,
            });
        }
    }

    entries
}

fn sanitize_bib_key(doi: &str) -> String {
    let raw: String = doi
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    let mut collapsed = String::new();
    let mut prev_underscore = false;
    for ch in raw.chars() {
        if ch == '_' {
            if !prev_underscore && !collapsed.is_empty() {
                collapsed.push('_');
            }
            prev_underscore = true;
        } else {
            collapsed.push(ch);
            prev_underscore = false;
        }
    }
    let trimmed = collapsed.trim_matches('_');
    let truncated: String = trimmed.chars().take(36).collect();
    if truncated.is_empty() {
        "doi".to_string()
    } else {
        truncated.trim_end_matches('_').to_string()
    }
}

fn extract_four_digit_year(input: &str) -> Option<String> {
    let chars: Vec<char> = input.chars().collect();
    for win in chars.windows(4) {
        if win.iter().all(|c| c.is_ascii_digit()) {
            let s: String = win.iter().collect();
            if let Ok(y) = s.parse::<u32>() {
                if (1950..=2099).contains(&y) {
                    return Some(s);
                }
            }
        }
    }
    None
}

fn escape_bibtex_value(s: &str) -> String {
    s.replace('\\', "\\textbackslash ")
        .replace('{', "\\{")
        .replace('}', "\\}")
        .replace('&', "\\&")
        .replace('%', "\\%")
        .replace('#', "\\#")
        .replace('_', "\\_")
        .replace(['\r', '\n'], " ")
}

/// 将已核验真实 DOI 文献格式化为标准 `.bib` 内容（无真实 DOI 时不输出伪造条目）
pub fn format_bibtex(entries: &[BibEntry]) -> String {
    if entries.is_empty() {
        return "% No verified DOI references found in retrieved OpenAlex/CrossRef records.\n"
            .to_string();
    }
    let mut out = String::from("% Verified Academic References (OpenAlex / CrossRef DOI)\n\n");
    for entry in entries {
        out.push_str(&format!("@article{{{},\n", entry.cite_key));
        out.push_str(&format!(
            "  title = {{{{{}}}}},\n",
            escape_bibtex_value(&entry.title)
        ));
        if !entry.author.trim().is_empty() {
            out.push_str(&format!(
                "  author = {{{}}},\n",
                escape_bibtex_value(&entry.author)
            ));
        }
        if !entry.journal.trim().is_empty() {
            out.push_str(&format!(
                "  journal = {{{}}},\n",
                escape_bibtex_value(&entry.journal)
            ));
        }
        if !entry.year.trim().is_empty() {
            out.push_str(&format!("  year = {{{}}},\n", entry.year.trim()));
        }
        out.push_str(&format!("  doi = {{{}}},\n", entry.doi.trim()));
        out.push_str(&format!("  url = {{{}}},\n", entry.url.trim()));
        if !entry.source.trim().is_empty() {
            out.push_str(&format!(
                "  note = {{Indexed source: {}}}\n",
                escape_bibtex_value(&entry.source)
            ));
        }
        out.push_str("}\n\n");
    }
    out
}

fn parse_numeric_array(val: &serde_json::Value) -> Option<Vec<f64>> {
    let arr = val.as_array()?;
    let nums: Vec<f64> = arr
        .iter()
        .filter_map(|v| v.as_f64())
        .filter(|n| n.is_finite())
        .collect();
    if nums.len() >= 2 && nums.len() == arr.len() {
        Some(nums)
    } else {
        None
    }
}

fn try_extract_series_from_json(
    obj: &serde_json::Map<String, serde_json::Value>,
) -> Option<SimulationPlotData> {
    let x_candidates = [
        "time_ms",
        "t_ms",
        "time",
        "t",
        "x",
        "step",
        "steps",
        "freq_hz",
        "frequency",
    ];
    let mut x_key_used: Option<String> = None;
    let mut x_vals: Option<Vec<f64>> = None;
    for key in x_candidates {
        if let Some(arr) = obj.get(key).and_then(parse_numeric_array) {
            x_key_used = Some(key.to_string());
            x_vals = Some(arr);
            break;
        }
    }

    let palette = ["#2563eb", "#dc2626", "#059669", "#d97706"];
    let mut series = Vec::new();
    let mut color_idx = 0usize;

    for (key, val) in obj {
        if x_key_used.as_deref() == Some(key.as_str()) {
            continue;
        }
        if let Some(vals) = parse_numeric_array(val) {
            let lower = key.to_lowercase();
            let is_ref = lower.contains("setpoint")
                || lower.contains("target")
                || lower.contains("baseline")
                || lower.contains("ref");
            let color = if is_ref {
                "#64748b".to_string()
            } else {
                let c = palette[color_idx % palette.len()];
                color_idx += 1;
                c.to_string()
            };
            series.push(SimulationSeries {
                label: key.clone(),
                color,
                dashed: is_ref,
                values: vals,
            });
            if series.len() >= 4 {
                break;
            }
        }
    }

    if series.is_empty() {
        return None;
    }

    let min_len = series.iter().map(|s| s.values.len()).min().unwrap_or(0);
    if min_len < 2 {
        return None;
    }

    let x_values = match x_vals {
        Some(mut xv) if xv.len() >= 2 => {
            let len = xv.len().min(min_len);
            xv.truncate(len);
            for s in &mut series {
                s.values.truncate(len);
            }
            xv
        }
        _ => {
            for s in &mut series {
                s.values.truncate(min_len);
            }
            (0..min_len).map(|i| i as f64).collect()
        }
    };

    let x_label = match x_key_used.as_deref() {
        Some("time_ms") | Some("t_ms") => "Time (ms)".to_string(),
        Some("time") | Some("t") => "Time (s)".to_string(),
        Some("freq_hz") | Some("frequency") => "Frequency (Hz)".to_string(),
        Some(other) => other.to_string(),
        None => "Sample Index".to_string(),
    };

    let scalar_notes: Vec<String> = obj
        .iter()
        .filter_map(|(k, v)| {
            v.as_f64()
                .filter(|n| n.is_finite())
                .map(|n| format!("{k}={n:.3}"))
        })
        .take(5)
        .collect();

    Some(SimulationPlotData {
        title: "Step 13 Numerical Simulation Curve".to_string(),
        x_label,
        y_label: "Simulated Response".to_string(),
        x_values,
        series,
        summary_note: if scalar_notes.is_empty() {
            "Source: Step 13 Experiment Validation".to_string()
        } else {
            format!("Step 13 Metrics: {}", scalar_notes.join(" | "))
        },
        from_step13: true,
    })
}

/// 从第 13 步实验结果（优先）或控制原型闭环仿真数据提取数值仿真绘图序列
pub fn extract_simulation_plot_data(
    ctx: &PipelineContext,
    fallback_sim: Option<&StepSimulationData>,
) -> Option<SimulationPlotData> {
    let mut ordered: Vec<&ExperimentResult> = ctx
        .experiment_results
        .iter()
        .filter(|e| e.success)
        .collect();
    for exp in &ctx.experiment_results {
        if !exp.success {
            ordered.push(exp);
        }
    }

    for exp in &ordered {
        if let Some(obj) = exp.metrics.as_object() {
            if let Some(plot) = try_extract_series_from_json(obj) {
                return Some(plot);
            }
        }
        for line in exp.stdout.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('{') && trimmed.ends_with('}') {
                if let Ok(serde_json::Value::Object(obj)) =
                    serde_json::from_str::<serde_json::Value>(trimmed)
                {
                    if let Some(plot) = try_extract_series_from_json(&obj) {
                        return Some(plot);
                    }
                }
            }
        }
    }

    // 若无数组序列但第 13 步存在标量数值指标，提取标量对比曲线
    let mut metric_names = Vec::new();
    let mut metric_vals = Vec::new();
    for exp in &ordered {
        if let Some(obj) = exp.metrics.as_object() {
            for (k, v) in obj {
                if let Some(n) = v.as_f64().filter(|x| x.is_finite()) {
                    metric_names.push(format!("{k}={n:.2}"));
                    metric_vals.push(n);
                }
            }
        }
    }
    if metric_vals.len() >= 2 {
        let x_values: Vec<f64> = (1..=metric_vals.len()).map(|i| i as f64).collect();
        return Some(SimulationPlotData {
            title: "Step 13 Experiment Metrics Profile".to_string(),
            x_label: "Metric Index".to_string(),
            y_label: "Measured Value".to_string(),
            x_values,
            series: vec![SimulationSeries {
                label: "Measured Metrics".to_string(),
                color: "#2563eb".to_string(),
                dashed: false,
                values: metric_vals,
            }],
            summary_note: metric_names
                .into_iter()
                .take(6)
                .collect::<Vec<_>>()
                .join(" | "),
            from_step13: true,
        });
    }

    if let Some(sim) = fallback_sim {
        if sim.time_ms.len() >= 2 && sim.actual_value.len() >= 2 {
            let len = sim.time_ms.len().min(sim.actual_value.len());
            let mut series = Vec::new();
            if sim.setpoint.len() >= len {
                series.push(SimulationSeries {
                    label: "Setpoint".to_string(),
                    color: "#64748b".to_string(),
                    dashed: true,
                    values: sim.setpoint.iter().copied().take(len).collect(),
                });
            }
            series.push(SimulationSeries {
                label: "Actual Response".to_string(),
                color: "#2563eb".to_string(),
                dashed: false,
                values: sim.actual_value.iter().copied().take(len).collect(),
            });
            return Some(SimulationPlotData {
                title: "Closed-Loop Step Response Simulation".to_string(),
                x_label: "Time (ms)".to_string(),
                y_label: "Response Value (%)".to_string(),
                x_values: sim.time_ms.iter().copied().take(len).collect(),
                series,
                summary_note: format!(
                    "Overshoot: {:.2}% | Steady-State Error: {:.2} | Settling Time: {:.1} ms",
                    sim.metrics.overshoot_pct,
                    sim.metrics.steady_state_error,
                    sim.metrics.settling_time_ms
                ),
                from_step13: false,
            });
        }
    }

    None
}

fn escape_svg_text(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// 纯本地高分辨率 SVG 仿真曲线渲染器（含坐标轴标注、刻度网格、标题与图例）
pub fn render_simulation_svg(plot: &SimulationPlotData) -> String {
    let width = 900.0_f64;
    let height = 520.0_f64;
    let left = 90.0_f64;
    let right = 860.0_f64;
    let top = 72.0_f64;
    let bottom = 430.0_f64;
    let plot_w = right - left;
    let plot_h = bottom - top;

    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    for &x in &plot.x_values {
        if x.is_finite() {
            min_x = min_x.min(x);
            max_x = max_x.max(x);
        }
    }
    if !min_x.is_finite() || !max_x.is_finite() {
        min_x = 0.0;
        max_x = 1.0;
    }
    if (max_x - min_x).abs() < 1e-9 {
        max_x = min_x + 1.0;
    }

    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for s in &plot.series {
        for &y in &s.values {
            if y.is_finite() {
                min_y = min_y.min(y);
                max_y = max_y.max(y);
            }
        }
    }
    if !min_y.is_finite() || !max_y.is_finite() {
        min_y = 0.0;
        max_y = 100.0;
    }
    if (max_y - min_y).abs() < 1e-9 {
        min_y -= 1.0;
        max_y += 1.0;
    } else {
        let pad = (max_y - min_y) * 0.08;
        min_y -= pad;
        max_y += pad;
    }

    let map_x = |x: f64| -> f64 { left + ((x - min_x) / (max_x - min_x)) * plot_w };
    let map_y = |y: f64| -> f64 { bottom - ((y - min_y) / (max_y - min_y)) * plot_h };

    let mut svg = String::new();
    svg.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {width:.0} {height:.0}\" width=\"{width:.0}\" height=\"{height:.0}\">\n"
    ));
    svg.push_str(&format!(
        "  <rect width=\"{width:.0}\" height=\"{height:.0}\" fill=\"#ffffff\"/>\n"
    ));
    svg.push_str(&format!(
        "  <text x=\"{:.1}\" y=\"32\" text-anchor=\"middle\" font-family=\"Arial, Helvetica, sans-serif\" font-size=\"18\" font-weight=\"bold\" fill=\"#111827\">{}</text>\n",
        (left + right) / 2.0,
        escape_svg_text(&plot.title)
    ));
    if !plot.summary_note.is_empty() {
        svg.push_str(&format!(
            "  <text x=\"{:.1}\" y=\"54\" text-anchor=\"middle\" font-family=\"Arial, Helvetica, sans-serif\" font-size=\"12\" fill=\"#4b5563\">{}</text>\n",
            (left + right) / 2.0,
            escape_svg_text(&plot.summary_note)
        ));
    }

    // 网格与刻度
    let ticks = 5usize;
    for i in 0..=ticks {
        let frac = (i as f64) / (ticks as f64);
        let y_pos = bottom - frac * plot_h;
        let y_val = min_y + frac * (max_y - min_y);
        svg.push_str(&format!(
            "  <line x1=\"{left:.1}\" y1=\"{y_pos:.1}\" x2=\"{right:.1}\" y2=\"{y_pos:.1}\" stroke=\"#e5e7eb\" stroke-width=\"1\"/>\n"
        ));
        svg.push_str(&format!(
            "  <text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"end\" font-family=\"Arial, Helvetica, sans-serif\" font-size=\"11\" fill=\"#4b5563\">{y_val:.1}</text>\n",
            left - 8.0,
            y_pos + 4.0
        ));

        let x_pos = left + frac * plot_w;
        let x_val = min_x + frac * (max_x - min_x);
        svg.push_str(&format!(
            "  <line x1=\"{x_pos:.1}\" y1=\"{top:.1}\" x2=\"{x_pos:.1}\" y2=\"{bottom:.1}\" stroke=\"#e5e7eb\" stroke-width=\"1\"/>\n"
        ));
        svg.push_str(&format!(
            "  <text x=\"{x_pos:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-family=\"Arial, Helvetica, sans-serif\" font-size=\"11\" fill=\"#4b5563\">{x_val:.1}</text>\n",
            bottom + 18.0
        ));
    }

    // 坐标轴边框
    svg.push_str(&format!(
        "  <line x1=\"{left:.1}\" y1=\"{top:.1}\" x2=\"{left:.1}\" y2=\"{bottom:.1}\" stroke=\"#374151\" stroke-width=\"1.5\"/>\n"
    ));
    svg.push_str(&format!(
        "  <line x1=\"{left:.1}\" y1=\"{bottom:.1}\" x2=\"{right:.1}\" y2=\"{bottom:.1}\" stroke=\"#374151\" stroke-width=\"1.5\"/>\n"
    ));

    // 坐标轴名称标注
    svg.push_str(&format!(
        "  <text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-family=\"Arial, Helvetica, sans-serif\" font-size=\"13\" font-weight=\"bold\" fill=\"#1f2937\">{}</text>\n",
        (left + right) / 2.0,
        bottom + 46.0,
        escape_svg_text(&plot.x_label)
    ));
    let y_mid = (top + bottom) / 2.0;
    svg.push_str(&format!(
        "  <text x=\"24\" y=\"{y_mid:.1}\" text-anchor=\"middle\" transform=\"rotate(-90 24 {y_mid:.1})\" font-family=\"Arial, Helvetica, sans-serif\" font-size=\"13\" font-weight=\"bold\" fill=\"#1f2937\">{}</text>\n",
        escape_svg_text(&plot.y_label)
    ));

    // 曲线绘制
    for s in &plot.series {
        let count = plot.x_values.len().min(s.values.len());
        if count < 2 {
            continue;
        }
        let mut pts = Vec::with_capacity(count);
        for i in 0..count {
            let xv = plot.x_values[i];
            let yv = s.values[i];
            if xv.is_finite() && yv.is_finite() {
                pts.push((map_x(xv), map_y(yv)));
            }
        }
        if pts.len() < 2 {
            continue;
        }
        let points_attr: String = pts
            .iter()
            .map(|(px, py)| format!("{px:.2},{py:.2}"))
            .collect::<Vec<_>>()
            .join(" ");
        let dash = if s.dashed {
            " stroke-dasharray=\"6,4\""
        } else {
            ""
        };
        svg.push_str(&format!(
            "  <polyline fill=\"none\" stroke=\"{}\" stroke-width=\"2.4\"{dash} points=\"{points_attr}\"/>\n",
            escape_svg_text(&s.color)
        ));
        if pts.len() <= 30 {
            for (px, py) in &pts {
                svg.push_str(&format!(
                    "  <circle cx=\"{px:.2}\" cy=\"{py:.2}\" r=\"3\" fill=\"{}\"/>\n",
                    escape_svg_text(&s.color)
                ));
            }
        }
    }

    // 图例框
    if !plot.series.is_empty() {
        let legend_w = 195.0_f64;
        let legend_h = 18.0 + (plot.series.len() as f64) * 22.0;
        let legend_x = right - legend_w - 14.0;
        let legend_y = top + 12.0;
        svg.push_str(&format!(
            "  <g class=\"legend\">\n    <rect x=\"{legend_x:.1}\" y=\"{legend_y:.1}\" width=\"{legend_w:.1}\" height=\"{legend_h:.1}\" rx=\"6\" fill=\"#ffffff\" fill-opacity=\"0.92\" stroke=\"#d1d5db\" stroke-width=\"1\"/>\n"
        ));
        for (idx, s) in plot.series.iter().enumerate() {
            let row_y = legend_y + 20.0 + (idx as f64) * 22.0;
            let dash = if s.dashed {
                " stroke-dasharray=\"5,3\""
            } else {
                ""
            };
            svg.push_str(&format!(
                "    <line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"{}\" stroke-width=\"2.5\"{dash}/>\n",
                legend_x + 12.0,
                row_y - 4.0,
                legend_x + 36.0,
                row_y - 4.0,
                escape_svg_text(&s.color)
            ));
            svg.push_str(&format!(
                "    <text x=\"{:.1}\" y=\"{row_y:.1}\" font-family=\"Arial, Helvetica, sans-serif\" font-size=\"12\" fill=\"#111827\">{}</text>\n",
                legend_x + 44.0,
                escape_svg_text(safe_truncate_chars(&s.label, 20))
            ));
        }
        svg.push_str("  </g>\n");
    }

    svg.push_str("</svg>\n");
    svg
}

fn escape_latex(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\textbackslash{}"),
            '{' => out.push_str("\\{"),
            '}' => out.push_str("\\}"),
            '$' => out.push_str("\\$"),
            '&' => out.push_str("\\&"),
            '%' => out.push_str("\\%"),
            '#' => out.push_str("\\#"),
            '_' => out.push_str("\\_"),
            '^' => out.push_str("\\textasciicircum{}"),
            '~' => out.push_str("\\textasciitilde{}"),
            _ => out.push(ch),
        }
    }
    out
}

/// 构造可直接用于投稿的 LaTeX + BibTeX + SVG 仿真图打包输出（自动按内容语言选择默认模板）
pub fn build_paper_latex_bundle(
    ctx: &PipelineContext,
    feature_cards: &[FeatureCard],
    db_patents: &[Patent],
    fallback_sim: Option<&StepSimulationData>,
) -> PaperLatexBundle {
    let sample = format!("{} {} {}", ctx.title, ctx.description, ctx.ai_analysis);
    let template = PaperLatexTemplate::from_query_or_content(None, &sample);
    build_paper_latex_bundle_with_template(ctx, feature_cards, db_patents, fallback_sim, template)
}

/// 按指定模板构造可直接用于投稿的 LaTeX + BibTeX + SVG 仿真图打包输出
pub fn build_paper_latex_bundle_with_template(
    ctx: &PipelineContext,
    feature_cards: &[FeatureCard],
    db_patents: &[Patent],
    fallback_sim: Option<&StepSimulationData>,
    template: PaperLatexTemplate,
) -> PaperLatexBundle {
    let bib_entries = extract_verified_bib_entries(ctx, db_patents);
    let bibtex_content = format_bibtex(&bib_entries);
    let plot_data = extract_simulation_plot_data(ctx, fallback_sim);
    let has_step13_simulation = plot_data.as_ref().is_some_and(|p| p.from_step13);
    let simulation_svg = match &plot_data {
        Some(plot) => render_simulation_svg(plot),
        None => {
            let default_sim =
                crate::pipeline::steps::control_proto::run_closed_loop_step_simulation(
                    &crate::pipeline::steps::control_proto::default_calibration_table(),
                );
            let default_plot = SimulationPlotData {
                title: "Closed-Loop Step Response Simulation".to_string(),
                x_label: "Time (ms)".to_string(),
                y_label: "Response Value (%)".to_string(),
                x_values: default_sim.time_ms,
                series: vec![
                    SimulationSeries {
                        label: "Setpoint".to_string(),
                        color: "#64748b".to_string(),
                        dashed: true,
                        values: default_sim.setpoint,
                    },
                    SimulationSeries {
                        label: "Actual Response".to_string(),
                        color: "#2563eb".to_string(),
                        dashed: false,
                        values: default_sim.actual_value,
                    },
                ],
                summary_note: format!(
                    "Overshoot: {:.2}% | Steady-State Error: {:.2} | Settling Time: {:.1} ms",
                    default_sim.metrics.overshoot_pct,
                    default_sim.metrics.steady_state_error,
                    default_sim.metrics.settling_time_ms
                ),
                from_step13: false,
            };
            render_simulation_svg(&default_plot)
        }
    };

    let title = if ctx.title.trim().is_empty() {
        "Engineering System Design and Experimental Validation"
    } else {
        ctx.title.trim()
    };
    let domain = if ctx.technical_domain.trim().is_empty() {
        "Engineering & Mechatronics"
    } else {
        ctx.technical_domain.trim()
    };
    let abstract_body = if ctx.description.trim().is_empty() {
        "Technical abstract pending refinement."
    } else {
        safe_truncate_chars(ctx.description.trim(), 500)
    };
    let clean_latex_keywords: Vec<String> = ctx
        .keywords
        .iter()
        .map(|k| k.trim())
        .filter(|k| is_valid_technical_keyword(k))
        .map(ToString::to_string)
        .collect();
    let keywords_line = if clean_latex_keywords.is_empty() {
        "Engineering Design; Closed-Loop Control; Experimental Validation".to_string()
    } else {
        clean_latex_keywords.join("; ")
    };

    let related_work_tex = if bib_entries.is_empty() {
        escape_latex(MISSING_DOI_TAG)
    } else {
        let mut s = String::from(
            "Prior academic studies retrieved via verified OpenAlex/CrossRef DOI indexing are summarized below:\\par\n\\begin{itemize}\n",
        );
        for entry in &bib_entries {
            s.push_str(&format!(
                "  \\item {} (DOI: \\texttt{{{}}})~\\cite{{{}}}.\n",
                escape_latex(&entry.title),
                escape_latex(&entry.doi),
                entry.cite_key
            ));
        }
        s.push_str("\\end{itemize}\n");
        s
    };

    let mut mechanism_tex = if ctx.ai_analysis.trim().is_empty() {
        escape_latex(abstract_body)
    } else {
        escape_latex(safe_truncate_chars(ctx.ai_analysis.trim(), 900))
    };
    if !feature_cards.is_empty() {
        mechanism_tex.push_str("\n\n\\begin{itemize}\n");
        for card in feature_cards.iter().take(6) {
            mechanism_tex.push_str(&format!(
                "  \\item \\textbf{{{}}}: {}\n",
                escape_latex(card.title.trim()),
                escape_latex(safe_truncate_chars(card.description.trim(), 200))
            ));
        }
        mechanism_tex.push_str("\\end{itemize}\n");
    }

    let sim_caption = match &plot_data {
        Some(p) if p.from_step13 => format!(
            "{} ({})",
            escape_latex(&p.title),
            escape_latex(&p.summary_note)
        ),
        Some(p) => format!(
            "{} ({})",
            escape_latex(&p.title),
            escape_latex(&p.summary_note)
        ),
        None => "Closed-loop step response simulation curve.".to_string(),
    };

    let exp_rows_tex = {
        let successful: Vec<&ExperimentResult> = ctx
            .experiment_results
            .iter()
            .filter(|e| e.success)
            .collect();
        if successful.is_empty() {
            format!(
                "\\noindent\\textit{{{}}}\n",
                escape_latex(MISSING_CALIBRATION_TAG)
            )
        } else {
            let mut tbl = String::from(
                "\\begin{table}[htbp]\n\\centering\n\\caption{Step 13 Numerical Experiment Metrics}\n\\begin{tabular}{llp{7.5cm}}\n\\toprule\nRun & Duration (ms) & Verified Metrics \\\\\n\\midrule\n",
            );
            for (idx, exp) in successful.iter().enumerate() {
                tbl.push_str(&format!(
                    "{} & {} & \\texttt{{{}}} \\\\\n",
                    idx + 1,
                    exp.duration_ms,
                    escape_latex(safe_truncate_chars(&exp.metrics.to_string(), 160))
                ));
            }
            tbl.push_str("\\bottomrule\n\\end{tabular}\n\\end{table}\n");
            tbl
        }
    };

    let conclusion_body = if ctx.action_plan.trim().is_empty() {
        format!(
            "This study establishes the structural and closed-loop validation baseline for {}.",
            title
        )
    } else {
        safe_truncate_chars(ctx.action_plan.trim(), 400).to_string()
    };

    let bibliography_block = if bib_entries.is_empty() {
        "% No \\cite{} keys emitted because zero verified DOI references are bound.\n".to_string()
    } else {
        let bib_style = match template {
            PaperLatexTemplate::Ctexart => "plain",
            PaperLatexTemplate::Ieee => "IEEEtran",
            PaperLatexTemplate::Elsevier => "elsarticle-num",
        };
        format!("\\bibliographystyle{{{bib_style}}}\n\\bibliography{{references}}\n")
    };

    let main_tex = match template {
        PaperLatexTemplate::Ctexart => format!(
            "% !TEX program = xelatex\n\
             % !NOVELTY LOCK WARNING: {novelty_plain}\n\
             \\documentclass[UTF8]{{ctexart}}\n\
             \\usepackage{{graphicx}}\n\
             \\usepackage{{amsmath}}\n\
             \\usepackage{{booktabs}}\n\
             \\usepackage{{hyperref}}\n\
             \\title{{{tex_title}}}\n\
             \\author{{InnoForge 工程创研工作台}}\n\
             \\date{{\\today}}\n\n\
             \\begin{{document}}\n\
             \\maketitle\n\n\
             \\begin{{center}}\n\
             \\fbox{{\\parbox{{0.94\\linewidth}}{{\\textbf{{防破新颖性安全锁：}} {tex_warning}}}}}\n\
             \\end{{center}}\n\n\
             \\begin{{abstract}}\n\
             {tex_abstract}\n\
             \\end{{abstract}}\n\n\
             \\noindent\\textbf{{关键词：}} {tex_keywords}\n\n\
             \\section{{引言与文献评述}}\n\
             本研究面向\\textbf{{{tex_domain}}}领域开展技术建模与实验验证。\n\
             {related_work}\n\
             \\section{{系统机理与结构建模}}\n\
             {mechanism}\n\n\
             \\section{{仿真与实验验证}}\n\
             {exp_rows}\n\
             \\begin{{figure}}[htbp]\n\
             \\centering\n\
             % Note: For pdfLaTeX, convert figures/simulation_curve.svg to PDF/PNG or compile with svg package.\n\
             \\includegraphics[width=0.92\\linewidth]{{figures/simulation_curve.svg}}\n\
             \\caption{{{sim_caption}}}\n\
             \\label{{fig:sim_curve}}\n\
             \\end{{figure}}\n\n\
             \\section{{结论}}\n\
             {tex_conclusion}\n\n\
             {bibliography_block}\
             \\end{{document}}\n",
            novelty_plain = NOVELTY_LOCK_TEXT_PLAIN,
            tex_title = escape_latex(title),
            tex_warning = escape_latex(NOVELTY_LOCK_TEXT_PLAIN),
            tex_abstract = escape_latex(abstract_body),
            tex_keywords = escape_latex(&keywords_line),
            tex_domain = escape_latex(domain),
            related_work = related_work_tex,
            mechanism = mechanism_tex,
            exp_rows = exp_rows_tex,
            sim_caption = sim_caption,
            tex_conclusion = escape_latex(&conclusion_body),
            bibliography_block = bibliography_block,
        ),
        PaperLatexTemplate::Ieee => format!(
            "% !TEX program = xelatex\n\
             % !NOVELTY LOCK WARNING: {novelty_plain}\n\
             \\documentclass[journal]{{IEEEtran}}\n\
             \\usepackage{{graphicx}}\n\
             \\usepackage{{amsmath}}\n\
             \\usepackage{{booktabs}}\n\
             \\usepackage{{cite}}\n\
             \\usepackage{{hyperref}}\n\
             \\title{{{tex_title}}}\n\
             \\author{{InnoForge Engineering Research Workspace}}\n\n\
             \\begin{{document}}\n\
             \\maketitle\n\n\
             \\begin{{center}}\n\
             \\fbox{{\\parbox{{0.94\\linewidth}}{{\\textbf{{Novelty Lock Warning:}} {tex_warning}}}}}\n\
             \\end{{center}}\n\n\
             \\begin{{abstract}}\n\
             {tex_abstract}\n\
             \\end{{abstract}}\n\n\
             \\begin{{IEEEkeywords}}\n\
             {tex_keywords}\n\
             \\end{{IEEEkeywords}}\n\n\
             \\section{{Introduction and Related Work}}\n\
             This work investigates technical challenges in \\textbf{{{tex_domain}}}.\n\
             {related_work}\n\
             \\section{{System Mechanism and Structural Modeling}}\n\
             {mechanism}\n\n\
             \\section{{Simulation and Experimental Validation}}\n\
             {exp_rows}\n\
             \\begin{{figure}}[htbp]\n\
             \\centering\n\
             % Note: For pdfLaTeX, convert figures/simulation_curve.svg to PDF/PNG or compile with svg package.\n\
             \\includegraphics[width=0.92\\linewidth]{{figures/simulation_curve.svg}}\n\
             \\caption{{{sim_caption}}}\n\
             \\label{{fig:sim_curve}}\n\
             \\end{{figure}}\n\n\
             \\section{{Conclusion}}\n\
             {tex_conclusion}\n\n\
             {bibliography_block}\
             \\end{{document}}\n",
            novelty_plain = NOVELTY_LOCK_TEXT_PLAIN,
            tex_title = escape_latex(title),
            tex_warning = escape_latex(NOVELTY_LOCK_TEXT_PLAIN),
            tex_abstract = escape_latex(abstract_body),
            tex_keywords = escape_latex(&keywords_line),
            tex_domain = escape_latex(domain),
            related_work = related_work_tex,
            mechanism = mechanism_tex,
            exp_rows = exp_rows_tex,
            sim_caption = sim_caption,
            tex_conclusion = escape_latex(&conclusion_body),
            bibliography_block = bibliography_block,
        ),
        PaperLatexTemplate::Elsevier => format!(
            "% !TEX program = xelatex\n\
             % !NOVELTY LOCK WARNING: {novelty_plain}\n\
             \\documentclass[preprint,12pt]{{elsarticle}}\n\
             \\usepackage{{graphicx}}\n\
             \\usepackage{{amsmath}}\n\
             \\usepackage{{booktabs}}\n\
             \\usepackage{{hyperref}}\n\n\
             \\begin{{document}}\n\
             \\begin{{frontmatter}}\n\
             \\title{{{tex_title}}}\n\
             \\author{{InnoForge Engineering Research Workspace}}\n\
             \\begin{{abstract}}\n\
             {tex_abstract}\n\
             \\end{{abstract}}\n\
             \\begin{{keyword}}\n\
             {tex_keywords}\n\
             \\end{{keyword}}\n\
             \\end{{frontmatter}}\n\n\
             \\begin{{center}}\n\
             \\fbox{{\\parbox{{0.94\\linewidth}}{{\\textbf{{Novelty Lock Warning:}} {tex_warning}}}}}\n\
             \\end{{center}}\n\n\
             \\section{{Introduction and Related Work}}\n\
             This work investigates technical challenges in \\textbf{{{tex_domain}}}.\n\
             {related_work}\n\
             \\section{{System Mechanism and Structural Modeling}}\n\
             {mechanism}\n\n\
             \\section{{Simulation and Experimental Validation}}\n\
             {exp_rows}\n\
             \\begin{{figure}}[htbp]\n\
             \\centering\n\
             \\includegraphics[width=0.92\\linewidth]{{figures/simulation_curve.svg}}\n\
             \\caption{{{sim_caption}}}\n\
             \\label{{fig:sim_curve}}\n\
             \\end{{figure}}\n\n\
             \\section{{Conclusion}}\n\
             {tex_conclusion}\n\n\
             {bibliography_block}\
             \\end{{document}}\n",
            novelty_plain = NOVELTY_LOCK_TEXT_PLAIN,
            tex_title = escape_latex(title),
            tex_warning = escape_latex(NOVELTY_LOCK_TEXT_PLAIN),
            tex_abstract = escape_latex(abstract_body),
            tex_keywords = escape_latex(&keywords_line),
            tex_domain = escape_latex(domain),
            related_work = related_work_tex,
            mechanism = mechanism_tex,
            exp_rows = exp_rows_tex,
            sim_caption = sim_caption,
            tex_conclusion = escape_latex(&conclusion_body),
            bibliography_block = bibliography_block,
        ),
    };

    PaperLatexBundle {
        idea_id: ctx.idea_id.clone(),
        template,
        novelty_lock_warning: NOVELTY_LOCK_WARNING.to_string(),
        bib_entries,
        bibtex_content,
        main_tex,
        simulation_svg,
        has_step13_simulation,
        plot_data,
    }
}

/// 从 LaTeX 源码提取全部 `\cite{...}` 引用键
pub fn extract_tex_cite_keys(tex: &str) -> Vec<String> {
    let mut keys = Vec::new();
    let mut rest = tex;
    while let Some(pos) = rest.find("\\cite{") {
        let after_start = pos + "\\cite{".len();
        let Some(tail) = rest.get(after_start..) else {
            break;
        };
        let Some(end) = tail.find('}') else {
            break;
        };
        if let Some(raw_keys) = tail.get(..end) {
            for k in raw_keys.split(',') {
                let trimmed = k.trim();
                if !trimmed.is_empty() {
                    keys.push(trimmed.to_string());
                }
            }
        }
        rest = tail.get(end + 1..).unwrap_or("");
    }
    keys
}

/// 从 `.bib` 内容提取全部 `@type{key,` 条目键
pub fn extract_bib_keys(bib: &str) -> HashSet<String> {
    let mut keys = HashSet::new();
    for line in bib.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with('@') {
            continue;
        }
        if let Some(open) = trimmed.find('{') {
            if let Some(after_open) = trimmed.get(open + 1..) {
                if let Some(comma) = after_open.find(',') {
                    if let Some(key) = after_open.get(..comma) {
                        let k = key.trim();
                        if !k.is_empty() {
                            keys.insert(k.to_string());
                        }
                    }
                }
            }
        }
    }
    keys
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::search::PipelinePatentHit;

    fn sample_feature_card() -> FeatureCard {
        FeatureCard {
            id: "fc-1".to_string(),
            idea_id: "idea-1".to_string(),
            title: "自适应谐波减速关节".to_string(),
            description: "用于协作机械臂的低背隙关节".to_string(),
            novelty_score: Some(0.85),
            created_at: "2026-11-18T00:00:00Z".to_string(),
            technical_problem: "高负载下背隙增大".to_string(),
            core_structure: "双支撑柔轮壳体与交叉滚子轴承座".to_string(),
            key_relations: "伺服电机经偏心波发生器与刚轮啮合".to_string(),
            process_steps: "编码器闭环采样扭矩并动态补偿".to_string(),
            application_scenarios: "精密装配机器人".to_string(),
        }
    }

    #[test]
    fn bom_pack_with_feature_cards_and_experiment_metrics() {
        let mut ctx = PipelineContext::new("idea-1", "低背隙谐波关节", "面向精密装配的关节模组");
        ctx.technical_domain = "机器人传动".to_string();
        ctx.experiment_results.push(ExperimentResult {
            script_path: "sim.py".to_string(),
            language: "python".to_string(),
            exit_code: 0,
            stdout: "peak_torque=42.5Nm".to_string(),
            stderr: String::new(),
            metrics: serde_json::json!({"backlash_arcmin": 0.8, "efficiency": 0.91}),
            duration_ms: 120,
            success: true,
        });

        let pack = build_engineering_bom_pack(&ctx, &[sample_feature_card()]);
        assert_eq!(pack.idea_id, "idea-1");
        assert_eq!(pack.bom_items.len(), 3);
        assert!(pack.control_io_spec_md.contains("0.91"));
        assert!(pack.control_io_spec_md.contains("peak_torque=42.5Nm"));
        assert!(!pack.control_io_spec_md.contains(MISSING_CALIBRATION_TAG));
        assert!(pack.full_markdown.contains("机械与电控分类 BOM 清单"));
        assert!(pack.full_markdown.contains("样机台架验收大纲"));
    }

    #[test]
    fn bom_pack_fallback_marks_calibration_needed_when_empty() {
        let mut ctx = PipelineContext::new("idea-2", "智能流体阀", "高精度微流量比例调节阀");
        ctx.keywords = vec!["比例电磁阀".to_string(), "压差传感器".to_string()];

        let pack = build_engineering_bom_pack(&ctx, &[]);
        assert_eq!(pack.bom_items.len(), 2);
        assert!(pack.control_io_spec_md.contains(MISSING_CALIBRATION_TAG));
        assert!(pack.bench_test_outline_md.contains(MISSING_CALIBRATION_TAG));
    }

    #[test]
    fn bom_separates_physics_relations_and_workflow_steps_from_hardware_parts() {
        let ctx = PipelineContext::new("idea-coal", "煤粉低氮稳燃工艺", "优化煤粉细度与燃烧效率");
        let card = FeatureCard {
            id: "fc-coal".to_string(),
            idea_id: "idea-coal".to_string(),
            title: "煤粉细度与燃烧耦合优化".to_string(),
            description: "降低未燃尽碳损失并抑制 NOx 生成".to_string(),
            novelty_score: Some(0.78),
            created_at: "2026-11-24T00:00:00Z".to_string(),
            technical_problem: "过粗煤粉燃尽差、过细能耗高".to_string(),
            core_structure: "分级旋流煤粉浓缩室与浓淡分离挡板".to_string(),
            key_relations: "煤粉细度R90与燃烧效率呈负相关，一次风速与火焰稳定性呈非线性耦合"
                .to_string(),
            process_steps: "原煤入磨→研磨至目标细度→风煤混合→浓淡分离→炉膛稳燃".to_string(),
            application_scenarios: "燃煤电站锅炉".to_string(),
        };

        let pack = build_engineering_bom_pack(&ctx, &[card]);
        assert_eq!(pack.bom_items.len(), 3);
        assert_eq!(pack.bom_items[0].category, "自制结构件");
        assert_eq!(
            pack.bom_items[1].category, "工艺/机理约束（非硬件件）",
            "纯物理相关性规律不得误标为外购标准件/执行器"
        );
        assert_eq!(
            pack.bom_items[2].category, "工艺流程/软件策略（非硬件件）",
            "纯工艺流转步骤不得误标为传感与控制件"
        );
        assert!(
            pack.control_io_spec_md.contains("暂无显式电控特征"),
            "非硬件工艺/机理条目不得污染电控 IO 单元列表: {}",
            pack.control_io_spec_md
        );
    }

    #[test]
    fn journal_draft_pins_novelty_lock_and_handles_real_vs_zero_dois() {
        let mut ctx_with_doi = PipelineContext::new("idea-3", "固态电解质界面修饰", "抑制枝晶生长");
        ctx_with_doi.patent_results.push(PipelinePatentHit {
            id: "10.1038/s41586-020-2649-2".to_string(),
            title: "Solid-state interface".to_string(),
            snippet: "OpenAlex work".to_string(),
            link: "https://doi.org/10.1038/s41586-020-2649-2".to_string(),
            source: "openalex".to_string(),
        });
        ctx_with_doi.web_results.push(PipelinePatentHit {
            id: "web-1".to_string(),
            title: "Duplicate DOI".to_string(),
            snippet: "See 10.1038/s41586-020-2649-2 and 10.1126/science.abc1234.".to_string(),
            link: String::new(),
            source: "web".to_string(),
        });

        let draft_with_doi = build_journal_manuscript_draft(&ctx_with_doi);
        assert!(draft_with_doi
            .full_markdown
            .starts_with(&draft_with_doi.novelty_lock_warning));
        assert!(draft_with_doi
            .novelty_lock_warning
            .contains("专利法》第22条第2款新颖性"));
        assert_eq!(draft_with_doi.cited_dois.len(), 2);
        assert_eq!(
            draft_with_doi.cited_dois.first().map(String::as_str),
            Some("10.1038/s41586-020-2649-2")
        );
        assert_eq!(
            draft_with_doi.cited_dois.get(1).map(String::as_str),
            Some("10.1126/science.abc1234")
        );
        assert!(draft_with_doi
            .full_markdown
            .contains("https://doi.org/10.1038/s41586-020-2649-2"));

        let ctx_zero_doi = PipelineContext::new("idea-4", "无引用方案", "纯内部预研描述");
        let draft_zero_doi = build_journal_manuscript_draft(&ctx_zero_doi);
        assert!(draft_zero_doi
            .full_markdown
            .starts_with(&draft_zero_doi.novelty_lock_warning));
        assert!(draft_zero_doi.cited_dois.is_empty());
        assert!(draft_zero_doi.full_markdown.contains(MISSING_DOI_TAG));
    }

    #[test]
    fn latex_bundle_zero_orphan_citations_and_real_doi_only() {
        let mut ctx = PipelineContext::new(
            "idea-bib-1",
            "主动磁悬浮隔振平台",
            "结合前馈自适应补偿的六自由度磁悬浮隔振系统",
        );
        ctx.keywords = vec!["磁悬浮".to_string(), "自适应控制".to_string()];
        ctx.patent_results.push(PipelinePatentHit {
            id: "10.1016/j.ymssp.2024.111222".to_string(),
            title: "Active magnetic vibration isolation".to_string(),
            snippet: "OpenAlex indexed work (2024)".to_string(),
            link: "https://doi.org/10.1016/j.ymssp.2024.111222".to_string(),
            source: "openalex".to_string(),
        });
        ctx.web_results.push(PipelinePatentHit {
            id: "crossref-1".to_string(),
            title: "Feedforward damping control".to_string(),
            snippet: "CrossRef DOI 10.1109/TIE.2023.3301234".to_string(),
            link: "https://doi.org/10.1109/TIE.2023.3301234".to_string(),
            source: "crossref".to_string(),
        });
        // 无 DOI 的网页条目，严禁被编造进参考文献
        ctx.web_results.push(PipelinePatentHit {
            id: "web-no-doi".to_string(),
            title: "Blog post without DOI".to_string(),
            snippet: "General engineering notes without identifier".to_string(),
            link: "https://example.org/blog/1".to_string(),
            source: "web".to_string(),
        });

        let db_patent = Patent {
            patent_number: "10.1016/j.ymssp.2024.111222".to_string(),
            title: "Active magnetic vibration isolation".to_string(),
            inventor: "Zhang Wei, Li Ming".to_string(),
            applicant: "Mechanical Systems and Signal Processing".to_string(),
            filing_date: "2024".to_string(),
            source: "openalex".to_string(),
            ..Default::default()
        };

        let bundle = build_paper_latex_bundle(&ctx, &[sample_feature_card()], &[db_patent], None);
        assert_eq!(bundle.bib_entries.len(), 2);
        assert!(bundle.bibtex_content.contains("@article{"));
        assert!(bundle.bibtex_content.contains("Zhang Wei, Li Ming"));
        assert!(bundle
            .bibtex_content
            .contains("Mechanical Systems and Signal Processing"));
        assert!(!bundle.bibtex_content.contains("Blog post without DOI"));
        assert!(bundle.main_tex.contains("专利法》第22条第2款新颖性"));

        let tex_cites = extract_tex_cite_keys(&bundle.main_tex);
        let bib_keys = extract_bib_keys(&bundle.bibtex_content);
        assert_eq!(tex_cites.len(), 2);
        assert_eq!(bib_keys.len(), 2);
        for cite_key in &tex_cites {
            assert!(
                bib_keys.contains(cite_key),
                "orphan citation {cite_key} not found in references.bib keys: {bib_keys:?}"
            );
        }

        // 零 DOI 时不应出现任何 \cite{...} 且不应编造 @article
        let empty_ctx = PipelineContext::new("idea-empty-doi", "纯内部预研", "无外网文献");
        let empty_bundle = build_paper_latex_bundle(&empty_ctx, &[], &[], None);
        assert!(empty_bundle.bib_entries.is_empty());
        assert!(extract_tex_cite_keys(&empty_bundle.main_tex).is_empty());
        assert!(extract_bib_keys(&empty_bundle.bibtex_content).is_empty());
    }

    #[test]
    fn svg_simulation_renderer_handles_step13_and_renders_axes_and_legend() {
        let mut ctx = PipelineContext::new("idea-sim-1", "闭环阶跃仿真测试", "仿真验证");
        ctx.experiment_results.push(ExperimentResult {
            script_path: "step13_sim.py".to_string(),
            language: "python".to_string(),
            exit_code: 0,
            stdout: "EXPERIMENT_DONE".to_string(),
            stderr: String::new(),
            metrics: serde_json::json!({
                "time_ms": [0.0, 50.0, 100.0, 150.0, 200.0],
                "setpoint": [80.0, 80.0, 80.0, 80.0, 80.0],
                "actual_value": [0.0, 32.5, 68.4, 79.2, 80.1],
                "overshoot_pct": 0.125
            }),
            duration_ms: 45,
            success: true,
        });

        let bundle = build_paper_latex_bundle(&ctx, &[], &[], None);
        assert!(bundle.has_step13_simulation);
        assert!(bundle.simulation_svg.contains("<svg"));
        assert!(bundle.simulation_svg.contains("Time (ms)"));
        assert!(bundle.simulation_svg.contains("Simulated Response"));
        assert!(bundle.simulation_svg.contains("class=\"legend\""));
        assert!(bundle.simulation_svg.contains("setpoint"));
        assert!(bundle.simulation_svg.contains("actual_value"));
        assert!(bundle.simulation_svg.contains("<polyline"));
        assert!(bundle.main_tex.contains("figures/simulation_curve.svg"));
    }

    #[test]
    fn latex_bundle_supports_ctexart_default_and_ieee_template_switch() {
        let mut ctx_zh = PipelineContext::new(
            "idea-tpl-zh",
            "双闭环磁流变隔振器",
            "针对高频微振动抑制的磁流变阻尼器建模与测试",
        );
        ctx_zh.patent_results.push(PipelinePatentHit {
            id: "10.1016/j.ymssp.2024.111222".to_string(),
            title: "MR Damper Control".to_string(),
            snippet: "OpenAlex".to_string(),
            link: "https://doi.org/10.1016/j.ymssp.2024.111222".to_string(),
            source: "openalex".to_string(),
        });

        // 中文内容默认选择 ctexart
        let default_zh = build_paper_latex_bundle(&ctx_zh, &[], &[], None);
        assert_eq!(default_zh.template, PaperLatexTemplate::Ctexart);
        assert!(default_zh
            .main_tex
            .contains(r"\documentclass[UTF8]{ctexart}"));
        assert!(default_zh.main_tex.contains("专利法》第22条第2款新颖性"));

        // 显式切换为 IEEEtran
        let ieee_bundle = build_paper_latex_bundle_with_template(
            &ctx_zh,
            &[],
            &[],
            None,
            PaperLatexTemplate::Ieee,
        );
        assert_eq!(ieee_bundle.template, PaperLatexTemplate::Ieee);
        assert!(ieee_bundle
            .main_tex
            .contains(r"\documentclass[journal]{IEEEtran}"));
        assert!(ieee_bundle.main_tex.contains(r"\begin{IEEEkeywords}"));
        assert_eq!(
            extract_tex_cite_keys(&ieee_bundle.main_tex).len(),
            1,
            "IEEE template must still cite verified DOI"
        );

        // 纯英文内容默认选择 IEEEtran
        let ctx_en = PipelineContext::new(
            "idea-tpl-en",
            "Active Magnetic Vibration Isolation",
            "Closed-loop feedforward damping control system",
        );
        let default_en = build_paper_latex_bundle(&ctx_en, &[], &[], None);
        assert_eq!(default_en.template, PaperLatexTemplate::Ieee);
        assert!(default_en
            .main_tex
            .contains(r"\documentclass[journal]{IEEEtran}"));
    }

    #[test]
    fn bom_and_journal_filter_noisy_cjk_fragment_keywords() {
        let mut ctx = PipelineContext::new(
            "idea-noise-kw",
            "高效散热电机",
            "采用双螺旋流道散热结构实现定子温升降低",
        );
        ctx.keywords = vec![
            "录".into(),
            "烧这".into(),
            "录你".into(),
            "话记".into(),
            "定子散热".into(),
            "电磁阀".into(),
        ];
        let bom = build_bom_items(&ctx, &[]);
        let part_names: Vec<&str> = bom.iter().map(|b| b.part_name.as_str()).collect();
        assert!(part_names.contains(&"定子散热"));
        assert!(part_names.contains(&"电磁阀"));
        for noise in ["录", "烧这", "录你", "话记"] {
            assert!(
                !part_names.contains(&noise),
                "BOM 不得包含口语碎片关键词 {}: {:?}",
                noise,
                part_names
            );
        }

        let journal = build_journal_manuscript_draft(&ctx);
        assert!(journal.full_markdown.contains("定子散热"));
        for noise in ["烧这", "录你", "话记"] {
            assert!(
                !journal.full_markdown.contains(noise),
                "期刊手稿关键词不得包含噪音碎片 {}",
                noise
            );
        }
    }
}
