//! 工程落地交付包与期刊论文草稿纯生成模块 / Engineering deliverables & journal manuscript draft generator
//!
//! 消费 `PipelineContext`、`FeatureCard` 与 `ExperimentResult`（真实指标/stdout）以及
//! `patent_results` / `web_results` 中的真实 DOI，零网络调用、零编造数据。

use crate::ai::client::safe_truncate_chars;
use crate::patent::FeatureCard;
use crate::pipeline::context::{ExperimentResult, PipelineContext};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

const NOVELTY_LOCK_WARNING: &str = "> ⚠️ 【防破新颖性安全锁】：本手稿包含尚未公开的核心技术方案。在正式提交专利申请并取得申请号/申请日之前，严禁公开发表、上传预印本或外发审稿，否则将导致本申请丧失《专利法》第22条第2款新颖性！";

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
    let keywords_line = if ctx.keywords.is_empty() {
        "【待补充关键词】".to_string()
    } else {
        ctx.keywords.join("；")
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
                items.push(BomItem {
                    item_no: next_no,
                    part_name: format!("{} - 传动/连接执行组件", feature_name),
                    category: "外购标准件/执行器".to_string(),
                    spec_summary: safe_truncate_chars(card.key_relations.trim(), 120).to_string(),
                    source_feature: feature_name.to_string(),
                });
                next_no += 1;
                added_for_card = true;
            }

            if !card.process_steps.trim().is_empty() {
                items.push(BomItem {
                    item_no: next_no,
                    part_name: format!("{} - 传感与控制单元", feature_name),
                    category: "传感与控制件".to_string(),
                    spec_summary: safe_truncate_chars(card.process_steps.trim(), 120).to_string(),
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

    // 特征卡为空时，回退解析 ctx.keywords + ctx.description
    for kw in &ctx.keywords {
        let trimmed = kw.trim();
        if trimmed.is_empty() {
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

fn classify_keyword_category(keyword: &str) -> &'static str {
    let lower = keyword.to_lowercase();
    if lower.contains("传感")
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
}
