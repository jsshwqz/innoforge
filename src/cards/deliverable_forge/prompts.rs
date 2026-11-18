//! 卡片 ⑥ 四大体裁分节编排与 Know-how 颗粒度裁剪提示词（`src/cards/deliverable_forge/prompts.rs`）

use crate::core::vault::models::DeliverableGenre;

/// 单个体裁的标准章节蓝图条目
#[derive(Debug, Clone, Copy)]
pub struct GenreSectionSpec {
    pub key: &'static str,
    pub heading_zh: &'static str,
    pub heading_en: &'static str,
    pub guidance: &'static str,
}

/// 返回指定体裁的标准分节蓝图
pub fn section_blueprint(genre: DeliverableGenre) -> &'static [GenreSectionSpec] {
    match genre {
        DeliverableGenre::ProductPrdTrd => PRD_TRD_SECTIONS,
        DeliverableGenre::SurveyFtoReport => SURVEY_FTO_SECTIONS,
        DeliverableGenre::AcademicPaper => ACADEMIC_PAPER_SECTIONS,
        DeliverableGenre::PatentDisclosure => PATENT_DISCLOSURE_SECTIONS,
        DeliverableGenre::OaPackage => OA_PACKAGE_SECTIONS,
        DeliverableGenre::PaperRebuttalPackage => PAPER_REBUTTAL_SECTIONS,
    }
}

/// 构造一源四出 AI 编译提示词（含体裁规范与 Know-how 颗粒度裁剪要求）
pub fn build_compile_system_prompt(genre: DeliverableGenre, knowhow_markers: &[String]) -> String {
    let genre_directive = match genre {
        DeliverableGenre::ProductPrdTrd => {
            "目标体裁为【产品工程方案 PRD/TRD】。聚焦系统架构、模块接口契约、非功能指标（延迟/吞吐/功耗）、验收测试用例与风险清单；保留工程可落地性。"
        }
        DeliverableGenre::SurveyFtoReport => {
            "目标体裁为【全域技术调研与 FTO 自由实施报告】。覆盖四维文献矩阵（专利+论文+开源+标准）、权利要求对比与规避设计路径，每项结论须绑定真实锚点。"
        }
        DeliverableGenre::AcademicPaper => {
            "目标体裁为【学术期刊/会议论文手稿】。严格采用 IMRaD 结构（Abstract / Introduction / Method / Experiments / Conclusion），所有引用必须仅使用已核验真实 DOI 的题录与真实实测实验锚点 Exp:{id}，严禁编造任何 DOI 或实验数据。"
        }
        DeliverableGenre::PatentDisclosure => {
            "目标体裁为【专利技术交底书】。包含技术领域、背景技术、发明内容（要解决的技术问题、技术方案、有益效果）、附图说明与具体实施方式（含多组可替换实施例与参数区间）。"
        }
        DeliverableGenre::OaPackage => {
            "目标体裁为【专利审查意见答复套包】。聚焦驳回条款逐条回应、权利要求修改对照与第 33 条不超范围论证。"
        }
        DeliverableGenre::PaperRebuttalPackage => {
            "目标体裁为【学术论文同行评审返修回复套包】。逐条回应审稿人意见，并同步对齐补充实验锚点 Exp:{id}。"
        }
    };

    let knowhow_rule = if knowhow_markers.is_empty() {
        "商业秘密裁剪：当前未指定额外脱敏词条，仍需避免泄露未授权凭据或涉密代号。".to_string()
    } else {
        format!(
            "商业秘密颗粒度裁剪（强制）：对以下敏感工艺参数/商业秘密词条执行脱敏或区间泛化，不得原文暴露：{}。",
            knowhow_markers.join("、")
        )
    };

    format!(
        "你是 InnoForge 创研台的全谱系成果编译专家。\n{}\n{}\n\
         输出要求：\n\
         1. 严格按指定章节结构组织内容，保持技术术语前后一致；\n\
         2. 引用实验数据时必须使用 `Exp:{{exp_id}}` 锚点，引用文献时使用真实锚点或核验 DOI；\n\
         3. 不得捏造未在上下文中出现的实验指标、专利号或文献题录。",
        genre_directive, knowhow_rule
    )
}

const PRD_TRD_SECTIONS: &[GenreSectionSpec] = &[
    GenreSectionSpec {
        key: "overview",
        heading_zh: "1. 产品背景与核心目标 (PRD Overview)",
        heading_en: "1. Product Overview & Objectives",
        guidance: "阐述用户痛点、业务目标与核心价值主张。",
    },
    GenreSectionSpec {
        key: "architecture",
        heading_zh: "2. 系统架构与技术方案 (TRD Architecture)",
        heading_en: "2. System Architecture & Technical Design",
        guidance: "描述核心模块划分、关键算法/硬件链路及数据流。",
    },
    GenreSectionSpec {
        key: "validation",
        heading_zh: "3. 对照实验与量化验收指标 (Validation & Metrics)",
        heading_en: "3. Validation & Quantitative Acceptance Criteria",
        guidance: "绑定已核验实验记录 Exp:{id} 与实测性能基线。",
    },
    GenreSectionSpec {
        key: "ip_boundary",
        heading_zh: "4. 知识产权边界与实施风险 (IP & Risk Guard)",
        heading_en: "4. IP Boundaries & Implementation Risks",
        guidance: "标注商业秘密保留项与专利布局前置要求。",
    },
];

const SURVEY_FTO_SECTIONS: &[GenreSectionSpec] = &[
    GenreSectionSpec {
        key: "scope",
        heading_zh: "1. 调研范围与四维情报源 (Scope & Omni-Sources)",
        heading_en: "1. Survey Scope & 4D Intelligence Sources",
        guidance: "汇总全球专利、学术论文、开源工程与行业标准检索边界。",
    },
    GenreSectionSpec {
        key: "landscape",
        heading_zh: "2. 技术演进路线与竞品墙分析 (Landscape & Competitor Wall)",
        heading_en: "2. Technology Landscape & Competitor Analysis",
        guidance: "对照核心文献与竞品方案识别技术空白与高风险权利要求。",
    },
    GenreSectionSpec {
        key: "fto_matrix",
        heading_zh: "3. FTO 自由实施核查与规避建议 (FTO Matrix & Design-Around)",
        heading_en: "3. FTO Assessment & Design-Around Pathways",
        guidance: "逐项比对必要技术特征并给出工程规避方案。",
    },
    GenreSectionSpec {
        key: "references",
        heading_zh: "4. 核验参考文献与实验佐证 (Verified References & Experiments)",
        heading_en: "4. Verified References & Experimental Evidence",
        guidance: "列出所有经核验的文献与实测实验记录。",
    },
];

const ACADEMIC_PAPER_SECTIONS: &[GenreSectionSpec] = &[
    GenreSectionSpec {
        key: "abstract",
        heading_zh: "Abstract / 摘要",
        heading_en: "Abstract",
        guidance: "概括研究动机、核心方法、实测提升幅度与主要结论。",
    },
    GenreSectionSpec {
        key: "introduction",
        heading_zh: "1. Introduction / 引言与相关工作",
        heading_en: "1. Introduction & Related Work",
        guidance: "基于已核验真实 DOI 文献梳理研究缺口与本文贡献。",
    },
    GenreSectionSpec {
        key: "methodology",
        heading_zh: "2. Methodology / 核心方法与系统设计",
        heading_en: "2. Methodology",
        guidance: "形式化描述技术方案，并对商业秘密保留参数做科学脱敏。",
    },
    GenreSectionSpec {
        key: "experiments",
        heading_zh: "3. Experiments & Results / 实验验证与结果分析",
        heading_en: "3. Experiments & Results",
        guidance: "严格绑定已核验对照实验 Exp:{id}，禁止未经实测的虚构指标。",
    },
    GenreSectionSpec {
        key: "conclusion",
        heading_zh: "4. Conclusion / 结论与展望",
        heading_en: "4. Conclusion",
        guidance: "总结贡献、局限性与后续工作。",
    },
];

const PATENT_DISCLOSURE_SECTIONS: &[GenreSectionSpec] = &[
    GenreSectionSpec {
        key: "tech_field",
        heading_zh: "一、技术领域与背景技术",
        heading_en: "1. Technical Field & Background Art",
        guidance: "界定所属技术领域并客观指出最接近现有技术的技术缺陷。",
    },
    GenreSectionSpec {
        key: "invention_content",
        heading_zh: "二、发明内容（技术问题、方案与有益效果）",
        heading_en: "2. Summary of the Invention",
        guidance: "清晰给出独立权利要求核心构思及对应的技术效果。",
    },
    GenreSectionSpec {
        key: "embodiments",
        heading_zh: "三、附图说明与具体实施方式（含多实施例扩展）",
        heading_en: "3. Brief Description of Drawings & Detailed Embodiments",
        guidance: "展开结构/步骤细节、可替换参数范围与对照实验验证。",
    },
    GenreSectionSpec {
        key: "claims_draft",
        heading_zh: "四、权利要求保护要点建议",
        heading_en: "4. Recommended Claim Scope",
        guidance: "分层列出独权、从权梯次保护要点。",
    },
];

const OA_PACKAGE_SECTIONS: &[GenreSectionSpec] = &[
    GenreSectionSpec {
        key: "oa_summary",
        heading_zh: "一、审查意见焦点摘要",
        heading_en: "1. Office Action Summary",
        guidance: "归纳驳回条款与对比文件核心结合点。",
    },
    GenreSectionSpec {
        key: "amendment_and_argument",
        heading_zh: "二、权利要求修改说明与第 33 条合规论证",
        heading_en: "2. Claim Amendments & Art. 33 Compliance",
        guidance: "逐条给出修改依据与创造性/新颖性抗辩。",
    },
];

const PAPER_REBUTTAL_SECTIONS: &[GenreSectionSpec] = &[
    GenreSectionSpec {
        key: "rebuttal_overview",
        heading_zh: "1. Summary of Revisions / 返修总览",
        heading_en: "1. Summary of Revisions",
        guidance: "概括对审稿人核心关切的回应与新增实验验证。",
    },
    GenreSectionSpec {
        key: "point_by_point",
        heading_zh: "2. Point-by-Point Response / 逐条回复",
        heading_en: "2. Point-by-Point Response",
        guidance: "逐条引用核验文献与 Exp:{id} 实测数据回应。",
    },
];
