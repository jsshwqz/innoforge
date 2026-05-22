//! 专利分析方法 / Patent analysis methods

use super::client::{safe_truncate, AiClient, Message};
use anyhow::Result;

impl AiClient {
    pub async fn summarize_patent(
        &self,
        patent_title: &str,
        abstract_text: &str,
        claims: &str,
    ) -> Result<String> {
        let prompt = format!(
            "请对以下专利进行全面分析摘要：\n\n\
             标题：{patent_title}\n\n\
             摘要：{abstract_text}\n\n\
             权利要求（前部分）：{claims_preview}\n\n\
             请从以下几个方面分析：\n\
             1. 技术领域\n\
             2. 核心技术方案\n\
             3. 创新点\n\
             4. 应用场景\n\
             5. 关键权利要求解读",
            claims_preview = safe_truncate(claims, 2000)
        );
        self.chat(&prompt, None).await
    }

    /// Analyze patent claims: identify independent vs dependent, extract scope elements.
    pub async fn analyze_claims(&self, patent_title: &str, claims: &str) -> Result<String> {
        let prompt = format!(
            "请对以下专利的权利要求进行深度分析：\n\n\
             专利标题：{patent_title}\n\n\
             权利要求全文：\n{claims_text}\n\n\
             请按以下格式分析（使用 Markdown 表格）：\n\n\
             ### 1. 权利要求结构总览\n\
             列出每条权利要求的编号、类型（独立/从属）、所从属的权利要求号\n\n\
             ### 2. 独立权利要求分析\n\
             对每条独立权利要求：\n\
             - 保护范围要素（技术特征列表）\n\
             - 保护范围宽度评估（宽/中/窄）\n\
             - 可能的规避设计方向\n\n\
             ### 3. 从属权利要求层级\n\
             用缩进或树形结构展示权利要求之间的从属关系\n\n\
             ### 4. 关键技术特征\n\
             提取最核心的限定性技术特征（决定保护范围的关键要素）\n\n\
             ### 5. 保护强度评估\n\
             综合评估该专利权利要求的保护强度（强/中/弱），并说明原因",
            claims_text = safe_truncate(claims, 4000)
        );

        let messages = vec![
            Message {
                role: "system".into(),
                content: "你是一位资深专利代理人和知识产权律师。你擅长解读专利权利要求书，\
                         分析保护范围，识别关键技术特征。请用专业、严谨的语言分析。"
                    .into(),
            },
            Message {
                role: "user".into(),
                content: prompt,
            },
        ];

        self.send_chat(messages, 0.3).await
    }

    /// Assess infringement risk: compare a product/tech description against multiple patents.
    pub async fn assess_infringement(
        &self,
        product_description: &str,
        patents_info: &str,
    ) -> Result<String> {
        let prompt = format!(
            "## 待评估的产品/技术方案\n{product}\n\n\
             ## 对比专利列表\n{patents}\n\n\
             请对每个专利逐一进行侵权风险评估，按以下格式输出（使用 Markdown 表格）：\n\n\
             ### 侵权风险评估矩阵\n\
             | 专利号 | 风险等级 | 关键风险点 | 规避建议 |\n\
             |--------|----------|------------|----------|\n\n\
             风险等级说明：\n\
             - **高风险**: 产品技术方案与专利权利要求高度重合\n\
             - **中风险**: 部分技术特征重合，需进一步分析\n\
             - **低风险**: 技术方案存在明显差异\n\
             - **无风险**: 不在专利保护范围内\n\n\
             ### 详细分析\n\
             对每个高/中风险专利，详细说明：\n\
             1. 哪些技术特征与专利权利要求对应\n\
             2. 字面侵权还是等同侵权的可能性\n\
             3. 具体的规避设计建议\n\n\
             ### 综合建议\n\
             整体风险评估和应对策略建议",
            product = safe_truncate(product_description, 2000),
            patents = safe_truncate(patents_info, 4000),
        );

        let messages = vec![
            Message {
                role: "system".into(),
                content: "你是一位资深知识产权律师和专利侵权分析专家。你擅长评估产品的专利侵权风险，\
                         对比技术方案与专利权利要求的对应关系。请客观、专业地分析，并提供可操作的建议。".into(),
            },
            Message {
                role: "user".into(),
                content: prompt,
            },
        ];

        self.send_chat(messages, 0.3).await
    }

    /// Compare multiple patents across multiple dimensions.
    pub async fn compare_multiple(&self, patents_info: &str) -> Result<String> {
        let prompt = format!(
            "请对以下多个专利进行多维度对比分析：\n\n{patents}\n\n\
             请按以下格式输出（使用 Markdown 表格）：\n\n\
             ### 1. 基本信息对比\n\
             | 维度 | 专利1 | 专利2 | ... |\n\
             |------|-------|-------|-----|\n\
             | 技术领域 | | | |\n\
             | 核心问题 | | | |\n\
             | 申请人 | | | |\n\n\
             ### 2. 技术方案对比\n\
             | 维度 | 专利1 | 专利2 | ... |\n\
             |------|-------|-------|-----|\n\
             | 核心方案 | | | |\n\
             | 创新点 | | | |\n\
             | 技术路线 | | | |\n\n\
             ### 3. 优缺点分析\n\
             | 专利 | 优点 | 缺点 | 应用场景 |\n\n\
             ### 4. 综合评价\n\
             - 技术演进趋势\n\
             - 最具创新性的方案\n\
             - 互补性分析",
            patents = safe_truncate(patents_info, 6000),
        );

        let messages = vec![
            Message {
                role: "system".into(),
                content: "你是一位专利技术分析专家，擅长对比分析多个专利的技术方案，\
                         识别技术演进趋势和创新差异。请用结构化的表格形式呈现分析结果。"
                    .into(),
            },
            Message {
                role: "user".into(),
                content: prompt,
            },
        ];

        self.send_chat(messages, 0.5).await
    }

    /// Inventiveness (创造性) analysis: compare my patent against reference documents
    /// using the three-step method (三步法) per Chinese patent examination guidelines.
    pub async fn inventiveness_analysis(
        &self,
        my_patent_info: &str,
        references_info: &str,
    ) -> Result<String> {
        let prompt = format!(
            "## 我的专利\n{my_patent}\n\n## 对比文件\n{references}\n\n\
             请按照中国专利审查指南的「三步法」，对我的专利的每条独立权利要求逐一进行创造性分析：\n\n\
             ### 分析要求\n\
             对每条独立权利要求，依次完成：\n\
             1. **确定最接近的现有技术**：从对比文件中选出最接近的一篇，说明理由\n\
             2. **确定区别技术特征**：列出我的权利要求与最接近现有技术之间的全部区别技术特征\n\
             3. **判断是否显而易见**：\n\
                - 该区别技术特征解决了什么技术问题（重新确定的技术问题）\n\
                - 对比文件中是否给出了将该区别特征应用于最接近现有技术的技术启示\n\
                - 综合判断：显而易见 / 非显而易见\n\
             4. **技术效果分析**：区别技术特征带来的技术效果（预料不到的效果加分）\n\
             5. **答辩建议**：\n\
                - 如果创造性成立：给出答辩要点\n\
                - 如果创造性不足：给出修改建议（如合并从属权利要求）\n\n\
             ### 输出格式\n\
             请使用 Markdown，对每条独立权利要求单独成节，使用表格汇总区别特征。\n\
             每个步骤的结论请按以下结构化格式输出：\n\
             - **结论**: ...\n\
             - **置信度**: 高 / 中 / 低\n\
             - **证据**: 具体引用对比文件内容\n\
             - **反论**: 可能存在的相反观点或证据\n\
             - **下一步**: 建议的应对策略\n\
             最后给出「综合答辩策略建议」。",
            my_patent = safe_truncate(my_patent_info, 5000),
            references = safe_truncate(references_info, 5000),
        );

        let messages = vec![
            Message {
                role: "system".into(),
                content: "你是一位资深中国专利代理师（执业15年+），精通中国专利法及审查指南中的创造性判断标准（三步法）。\
                         你擅长答复审查意见通知书，尤其是创造性驳回（A22.3）的答辩。\
                         请用严谨、专业的语言分析，结论要有理有据。".into(),
            },
            Message {
                role: "user".into(),
                content: prompt,
            },
        ];

        self.send_chat(messages, 0.3).await
    }

    /// Office action response: AI-assisted response strategy for patent office actions
    ///
    /// Supports three OA types:
    /// - "first_exam": 一审/二审答复 (first/second office action)
    /// - "abnormal": 非正常申请答复 (abnormal application notice)
    /// - "reject_review": 驳回后复审请求 (re-examination after rejection)
    ///
    /// Supports three depth levels:
    /// - "shallow": 简要分析 / Quick overview
    /// - "medium": 特征对比表+分层策略 / Feature table + layered strategy (default)
    /// - "deep": medium + AI自检反驳 / medium + self-critique from examiner perspective
    pub async fn office_action_response(
        &self,
        my_patent_info: &str,
        office_action: &str,
        references_info: &str,
        oa_type: &str,
        depth: &str,
    ) -> Result<String> {
        let my_patent = safe_truncate(my_patent_info, 15000);
        let oa = safe_truncate(office_action, 10000);
        let refs = safe_truncate(references_info, 15000);
        let is_deep = depth == "deep";

        let (system_role, prompt) = match oa_type {
            "abnormal" => Self::build_abnormal_prompt(my_patent, oa, refs, depth),
            "reject_review" => Self::build_reject_review_prompt(my_patent, oa, refs, depth),
            _ => Self::build_first_exam_prompt(my_patent, oa, refs, depth),
        };

        let messages = vec![
            Message {
                role: "system".into(),
                content: system_role,
            },
            Message {
                role: "user".into(),
                content: prompt,
            },
        ];

        let step1 = self.send_chat(messages, 0.3).await?;

        if !is_deep {
            return Ok(step1);
        }

        // Deep mode: second pass — self-critique from examiner perspective
        let critique = self.oa_critique(&step1, oa).await?;
        Ok(format!(
            "{}\n\n---\n\n## 审查员视角预判（AI 自检）\n{}",
            step1, critique
        ))
    }

    /// Self-critique step: review the proposed response from an examiner's perspective
    async fn oa_critique(&self, proposed_response: &str, office_action: &str) -> Result<String> {
        let prompt = format!(
            "你是一位资深中国专利审查员，具有 20 年实质审查经验。\
             请用审查员视角审阅以下答复方案，逐条指出：\n\n\
             1. **逻辑漏洞**：方案中的哪些论断存在推理跳跃或证据不足\n\
             2. **容易被反驳的点**：如果是你来审，你会从哪里切入反驳\n\
             3. **遗漏的关键点**：申请人可能漏掉了哪些重要论据\n\
             4. **修改建议**：如果要让这份方案更站得住脚，应该加强哪几个方向\n\n\
             请用简洁、直接的语气，每个要点用一两句话说清楚，不要客套。\n\n\
             ## 审查意见通知书\n{}\n\n\
             ## 拟提交的答复方案\n{}",
            safe_truncate(office_action, 8000),
            safe_truncate(proposed_response, 12000),
        );

        let messages = vec![
            Message {
                role: "system".into(),
                content: "你是一位资深中国专利审查员（执业20年，曾任复审委员会成员）。\
                         你精通中国专利法及审查指南，对创造性审查（A22.3）尤为严格。\
                         你善于发现答复方案中的逻辑漏洞和论证不足。\
                         请用挑剔、专业的眼光审阅，不需要客气。"
                    .into(),
            },
            Message {
                role: "user".into(),
                content: prompt,
            },
        ];

        self.send_chat(messages, 0.3).await
    }

    // ── OA prompt builders ──

    fn build_first_exam_prompt(
        my_patent: &str,
        oa: &str,
        refs: &str,
        depth: &str,
    ) -> (String, String) {
        if depth == "shallow" {
            return (
                "你是一位资深中国专利代理师，擅长应对审查意见通知书。请提供简明扼要的答复思路。"
                    .into(),
                format!(
                "## 我的专利\n{my_patent}\n\n## 审查意见通知书\n{oa}\n\n## 对比文献\n{refs}\n\n\
                 请生成简要的审查意见答复方案，包含：\n\
                 1. 审查意见核心问题（一句话概括）\n\
                 2. 最有力的 2-3 个反驳论点\n\
                 3. 建议的修改方向（如需要）\n\
                 4. 意见陈述书草稿（简要版）",
            ),
            );
        }

        // medium / deep: feature table + layered strategy
        (
            "你是一位资深中国专利代理师（执业20年+），精通中国专利法及审查指南。\
         你擅长应对审查意见通知书，尤其是创造性驳回（A22.3）的答辩。\
         你的答复策略注重：\n\
         1. **特征分解表**：按权利要求逐项拆解技术特征，与各对比文献逐一比对，\
            确保不遗漏任何区别特征\n\
         2. **分层论证**：按「否定技术启示 → 强调协同效果 → 退路修改」三层展开\n\
         3. 精确分解技术特征，找出审查员遗漏的区别点\n\
         4. 质疑对比文献组合的合理性（技术启示、技术领域差异、反向教导）\n\
         5. 强调组合后的协同技术效果\n\
         6. 提供可操作的权利要求修改方案\n\
         请用严谨、专业的语言，结论要有理有据，可直接用于提交。"
                .into(),
            format!(
                "## 我的专利（权利要求书+说明书）\n{my_patent}\n\n\
             ## 审查意见通知书\n{oa}\n\n\
             ## 对比文献\n{refs}\n\n\
             请基于以上材料，生成完整的审查意见答复方案：\n\n\
             ## 第一部分：审查意见解析\n\
             - 逐条列出审查员对每项权利要求的驳回理由\n\
             - 识别审查员引用的对比文献和具体段落\n\n\
             ## 第二部分：逐项权利要求特征对比表\n\
             以表格形式列出：| 权利要求项 | 技术特征 | D1 公开内容 | D2 公开内容 | 真正区别特征 |\n\
             对每项权利要求逐行填写，确保不遗漏任何技术特征。\
             如果某特征在某对比文献中未公开，标注「未公开」。\
             最后一行总结「审查员遗漏的区别特征」（如有）。\n\n\
             ## 第三部分：分层反驳策略\n\
             ### 层级A（最有力）：否定技术启示\n\
             - 对比文献与本申请的技术领域是否相同？\n\
             - 本领域技术人员是否有动机将多篇对比文献组合？\n\
             - 对比文献之间是否存在技术矛盾（反向教导）？\n\
             - 最接近现有技术选择是否合理？\n\n\
             ### 层级B（次有力）：强调协同技术效果\n\
             - 各特征组合后产生了什么意外技术效果？\n\
             - 对比文献是否意识到这个效果？\n\
             - 说明书中是否有支持性数据或实验？\n\n\
             ### 层级C（保底）：权利要求修改方案\n\
             - 修改后的权利要求书\n\
             - 修改后的区别特征相对于对比文献的创造性\n\n\
             ## 第四部分：答复策略建议\n\
             1. 建议优先采用的层级（A/B/C）及理由\n\
             2. 意见陈述的核心论点\n\
             3. 建议的修改后权利要求书\n\n\
             ## 第五部分：意见陈述书草稿\n\
             生成可直接提交的意见陈述书，包括：\n\
             - 对审查意见的回应\n\
             - 修改说明\n\
             - 创造性论述",
            ),
        )
    }

    fn build_abnormal_prompt(
        my_patent: &str,
        oa: &str,
        refs: &str,
        depth: &str,
    ) -> (String, String) {
        if depth == "shallow" {
            return (
                "你是一位资深中国专利代理师，专攻非正常专利申请的答辩。请提供简要答辩思路。".into(),
                format!(
                    "## 我的专利\n{my_patent}\n\n## 非正常申请认定通知\n{oa}\n\n## 参考材料\n{refs}\n\n\
                     请生成简要的非正常申请答辩方案：\n\
                     1. 被认定的主要原因（一句话概括）\n\
                     2. 最有力的 2-3 个答辩理由\n\
                     3. 意见陈述书草稿（简要版）",
                ),
            );
        }

        (
            "你是一位资深中国专利代理师，专攻非正常专利申请的答辩（中国国家知识产权局第77号令相关）。\
             你擅长：\n\
             1. 分析申请被认定为非正常的具体原因（批量申请、编造、抄袭、刻意回避等）\n\
             2. 从技术方案的真实性、研发过程的合理性、申请行为的正当性三个维度构建答辩理由\n\
             3. 提供充分的证据组织建议（研发记录、实验数据、产品照片、合作协议等）\n\
             4. 撰写有理有据的答复意见，语气诚恳、实事求是，不激怒审查员\n\
             请用严谨、专业的语言，结论要有理有据，可直接用于提交。"
                .into(),
            format!(
                "## 我的专利（权利要求书+说明书）\n{my_patent}\n\n\
                 ## 非正常申请认定通知\n{oa}\n\n\
                 ## 参考材料\n{refs}\n\n\
                 请基于以上材料，生成完整的非正常申请答辩方案：\n\n\
                 ## 第一部分：认定原因分析\n\
                 - 分析通知书中引用的具体认定依据（属于哪种非正常情形）\n\
                 - 评估认定是否有充分依据\n\n\
                 ## 第二部分：技术方案真实性论证\n\
                 - 说明本申请技术方案的技术来源和研发背景\n\
                 - 强调技术方案的具体性、可行性和可实施性\n\
                 - 如有实验数据或样品，指出如何佐证\n\n\
                 ## 第三部分：研发过程合理性说明\n\
                 - 本申请与申请人实际研发方向的关系\n\
                 - 申请时机和布局策略的合理性\n\
                 - 区别于「编造申请」或「抄袭申请」的关键证据\n\n\
                 ## 第四部分：逐条反驳\n\
                 针对通知书中的每条认定理由：\n\
                 - 正面回应，不回避\n\
                 - 提供事实依据和技术说明\n\
                 - 必要时附证据组织建议\n\n\
                 ## 第五部分：意见陈述书草稿\n\
                 生成可直接提交的意见陈述书，包括：\n\
                 - 事实陈述\n\
                 - 法律依据\n\
                 - 请求撤销非正常认定",
            ),
        )
    }

    fn build_reject_review_prompt(
        my_patent: &str,
        oa: &str,
        refs: &str,
        depth: &str,
    ) -> (String, String) {
        if depth == "shallow" {
            return (
                "你是一位资深中国专利代理师，精通中国专利复审程序。请提供简要复审思路。".into(),
                format!(
                    "## 我的专利\n{my_patent}\n\n## 驳回决定\n{oa}\n\n## 对比文献\n{refs}\n\n\
                     请生成简要的驳回后复审请求方案：\n\
                     1. 驳回决定的核心问题（一句话概括）\n\
                     2. 审查逻辑中的主要漏洞\n\
                     3. 复审请求的核心理由\n\
                     4. 复审请求书草稿（简要版）",
                ),
            );
        }

        (
            "你是一位资深中国专利代理师，精通中国专利复审程序（专利法第41条）。\
             你擅长：\n\
             1. 深入分析驳回决定中的审查逻辑，找出推理漏洞\n\
             2. 对驳回决定中认定的事实、法律适用和审查逻辑进行逐条辨析\n\
             3. 在维持原权利要求或修改权利要求的基础上提出有说服力的复审理由\n\
             4. 撰写符合专利复审委员会要求的复审请求书\n\
             请用严谨、专业的语言，结论要有理有据，可直接用于提交。"
                .into(),
            format!(
                "## 我的专利（权利要求书+说明书）\n{my_patent}\n\n\
                 ## 驳回决定\n{oa}\n\n\
                 ## 对比文献\n{refs}\n\n\
                 请基于以上材料，生成完整的驳回后复审请求方案：\n\n\
                 ## 第一部分：驳回决定分析\n\
                 - 逐条列出驳回决定中针对每项权利要求的驳回理由\n\
                 - 明确审查员引用的法条（A22.2新颖性 / A22.3创造性 / A25 等）\n\
                 - 识别审查员所依据的对比文献和具体对比内容\n\n\
                 ## 第二部分：审查逻辑漏洞分析\n\
                 1. 事实认定是否准确？（对比文献的内容是否被正确解读）\n\
                 2. 法条适用是否正确？（创造性判断方法是否符合审查指南）\n\
                 3. 区别特征是否有遗漏？（审查员是否忽略了本申请独有的技术特征）\n\
                 4. 技术启示是否成立？（现有技术是否有明确的教导或启示）\n\
                 5. 技术效果是否被低估？（组合后的协同效果或意外技术效果）\n\n\
                 ## 第三部分：答复策略与修改方案\n\
                 1. 是否修改权利要求？（维持 / 缩小 / 重写）\n\
                 2. 修改后的权利要求相对于对比文献的区别\n\
                 3. 建议的修改后权利要求书\n\n\
                 ## 第四部分：复审请求书草稿\n\
                 生成可直接提交的复审请求书，包括：\n\
                 - 驳回决定的错误或不当之处\n\
                 - 本申请具备创造性的理由\n\
                 - 修改说明（如有）",
            ),
        )
    }

    /// Batch summarize multiple patents concurrently.
    pub async fn batch_summarize(
        &self,
        patents: &[(String, String, String)],
    ) -> Vec<(String, Result<String>)> {
        let mut results = Vec::new();
        for (id, title, abstract_text) in patents {
            let result = self
                .chat(
                    &format!(
                        "请用2-3句话简要总结这个专利的核心技术方案：\n标题：{}\n摘要：{}",
                        title,
                        safe_truncate(abstract_text, 500)
                    ),
                    None,
                )
                .await;
            results.push((id.clone(), result));
        }
        results
    }
}
