//! 专利分析方法 / Patent analysis methods

#![allow(clippy::needless_borrow)]

use super::client::{safe_truncate, safe_truncate_chars, AiClient, Message};
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
        discuss: bool,
    ) -> Result<String> {
        let my_patent = safe_truncate(my_patent_info, 30000);
        let oa = safe_truncate(office_action, 20000);
        let refs = safe_truncate(references_info, 30000);
        let is_deep = depth == "deep";

        let (system_role, prompt) = match oa_type {
            "abnormal" => Self::build_abnormal_prompt(my_patent, oa, refs, depth, discuss),
            "reject_review" => Self::build_reject_review_prompt(my_patent, oa, refs, depth, discuss),
            _ => Self::build_first_exam_prompt(my_patent, oa, refs, depth, discuss),
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
        // Focus critique on the response draft (第五部分) specifically
        let response_part = Self::extract_oa_response_section(&step1);
        let critique = self.oa_critique(&response_part, oa).await?;
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
             4. **修改建议**：如果要让这份方案更站得住脚，应该加强哪几个方向\n\
             5. **特征对比准确性**：方案中对对比文献公开内容的认定是否有误？\
             如果有，请指出审查员认为正确的认定是什么\n\n\
             请用简洁、直接的语气，每个要点用一两句话说清楚，不要客套。\
             如果方案中引用了对比文献的具体段落号，请特别注意核实其准确性。\n\n\
             ## 审查意见通知书\n{}\n\n\
             ## 拟提交的答复方案\n{}",
            safe_truncate(office_action, 15000),
            safe_truncate(proposed_response, 20000),
        );

        let messages = vec![
            Message {
                role: "system".into(),
                content: "你是一位资深中国专利审查员（执业20年，曾任复审委员会成员）。\
                         你精通中国专利法及审查指南，对创造性审查（A22.3）尤为严格。\
                         你善于发现答复方案中的逻辑漏洞和论证不足。\
                         你对对比文献公开内容的认定极其敏感——如果申请人歪曲了对比文献的公开内容，\
                         你会毫不留情地指出。\
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

    /// Check whether amended claims properly overcome the rejection
    pub async fn check_claim_amendments(
        &self,
        original_claims: &str,
        amended_claims: &str,
        office_action: &str,
    ) -> Result<String> {
        let prompt = format!(
            "你是中国专利审查专家。请对比原始权利要求和修改后的权利要求，审查修改方案是否妥当。\n\n\
             ## 审查意见通知书（摘要）\n{}\n\n\
             ## 原始权利要求\n{}\n\n\
             ## 修改后的权利要求\n{}\n\n\
             请逐项分析：\n\n\
             1. **修改是否克服驳回理由**：每项修改是否直接回应了审查员指出的缺陷\n\
             2. **修改是否超范围**：修改是否超出原说明书和权利要求书记载的范围（A33）\n\
             3. **修改后的创造性**：修改后的区别特征是否具备创造性（A22.3）\n\
             4. **修改策略评分**：评分为 A（方案强）/ B（方案可接受）/ C（方案需加强）/ D（方案不可行）\n\
             5. **改进建议**：如方案有风险，给出具体的修改建议",
            safe_truncate(office_action, 6000),
            safe_truncate(original_claims, 8000),
            safe_truncate(amended_claims, 8000),
        );

        let messages = vec![
            Message {
                role: "system".into(),
                content: "你是一位资深中国专利代理师，精通专利法及审查指南，\
                         擅长审查权利要求修改方案是否能够克服审查意见通知书中的驳回理由。\
                         你的分析要严谨、具体，直接指出方案的风险和不足。"
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
        discuss: bool,
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

        // medium / deep: 先深度分析文档，再写回复
        let section5_prompt = if discuss {
            "\n\n---\n\
             注意：**本次仅输出第一部分至第四部分的分析内容。**\n\
             用户将在审阅分析结果并进行充分讨论后，再单独要求生成第五部分（意见陈述书草稿）。\n\
             因此请专注于提供详尽、准确的分析（第一部分至第四部分），确保每一个判断都有据可查。"
        } else {
            "\n\n=== 第五部分：意见陈述书草稿 ===\n\n\
             生成格式规范的「意见陈述书」，直接可用于提交，应包含：\n\n\
             一、关于审查意见的答复\n\
             （逐条回应审查员的驳回理由，引用上述分析）\n\n\
             二、关于权利要求的修改\n\
             （如有修改，逐条说明修改内容及A33合规性）\n\n\
             三、关于创造性的论述\n\
             （逐权利要求论述：区别特征、技术效果、非显而易见性）\n\n\
             四、结论\n\
             （请求审查员重新审查并授予专利权）\n\n\
             ---\n\
             注意：意见陈述书草稿必须**基于**第一部分至第四部分的分析结果撰写，\
             不得凭空给出与前面分析矛盾的论点。"
        };

        (
            "你是一位资深中国专利代理师（执业20年+），精通中国专利法及审查指南（2023修订版）。\
         你的工作流程是：**先深度分析，再写回复。**\
         分析必须精确到技术特征级别，不得笼统概括。\
         每一处论断都必须引用原文（通知书段落号、对比文献段落号）。\
         关键原则：\n\
         1. **精确到特征级**：每个权利要求必须拆解为~粒度适当的技术特征，逐特征对比\n\
         2. **引用原文**：每个判断必须引用原文出处（通知书第几页第几段、对比文献第几段）\n\
         3. **区分事实与观点**：审查员的结论是观点，对比文献公开的内容是事实，两者要分开论述\n\
         4. **每项权利要求单独处理**：不得笼统说「权利要求1-X」\n\
         5. **诚实分析**：如果对比文献确实公开了某特征，必须承认；区别在于未被公开的特征及其效果\n\
         请用严谨、专业的语言输出。分析结果使用固定格式（见用户提示），不得省略任何部分。"
                .into(),
            format!(
                "## 我的专利（权利要求书+说明书）\n{my_patent}\n\n\
             ## 审查意见通知书（全文）\n{oa}\n\n\
             ## 对比文献\n{refs}\n\n\
             请严格按以下结构输出。**每一部分都必须填写，不得跳过。**\n\n\
             === 第一部分：权利要求逐项解析 ===\n\n\
             对我的专利中的每项权利要求，逐项列出：\n\n\
             **权利要求1**（独立权利要求）\n\
             - 前序部分：……\n\
             - 特征部分：……\n\
             - 拆解为以下技术特征：\n\
               · 特征1.1：……\n\
               · 特征1.2：……\n\
               · 特征1.3：……\n\n\
             **权利要求2**（从属权利要求，引权利要求1）\n\
             - 附加特征：……\n\
             - 拆解为以下技术特征：\n\
               · 特征2.1：……\n\n\
             （以此类推，列出全部权利要求）\n\n\
             === 第二部分：审查员驳回逻辑逐条还原 ===\n\n\
             对审查员认为有问题的每项权利要求，还原审查员的完整推理链：\n\n\
             **权利要求X：**\n\
             - 审查员引用的法条：A22.2 新颖性 / A22.3 创造性 / ……\n\
             - 审查员引用的对比文献：D1（公开了……）、D2（公开了……）\n\
             - 审查员认定的事实：审查员认为 D1 公开了特征 A 和 B，D2 公开了特征 C……\n\
             - 审查员的法律推理：由于 D1+D2 公开了全部特征，且结合是显而易见的，因此……\n\
             - 审查员引用的段落（原文引用）：「……」（通知书具体段落）\n\n\
             （对每项被驳回的权利要求逐一还原）\n\n\
             === 第三部分：特征对比总表 ===\n\n\
             以表格形式输出【所有权利要求项】的特征对比，格式如下（严格按此格式）：\n\n\
             | 权利要求项 | 技术特征 | D1公开情况 | D2公开情况 | 其他对比文献 | 真正区别特征 |\n\
             |-----------|---------|-----------|-----------|------------|------------|\n\
             | 权1 | 特征1.1: …… | D1第[00XX]段公开了…… | 未公开 | 未公开 | — |\n\
             | 权1 | 特征1.2: …… | 未公开 | D2第[00XX]段公开了…… | 未公开 | ✅ 区别特征A |\n\
             | 权1 | 特征1.3: …… | 未公开 | 未公开 | 未公开 | ✅ 区别特征B |\n\
             | 权2 | 特征2.1: …… | …… | …… | …… | …… |\n\n\
             注意：\n\
             - 每项技术特征单独一行\n\
             - 「公开情况」必须写「公开了……（引用具体段落）」或「未公开」\n\
             - 「真正区别特征」列：如果该特征在所有对比文献中均未公开，标✅并命名；否则标—\n\
             - 最后加一行「审查员遗漏的区别特征总结」\n\n\
             === 第四部分：逐权利要求反驳论点 ===\n\n\
             基于上述对比总表，对审查员驳回的**每项权利要求**，按层级逐条写出反驳：\n\n\
             **权利要求X：**\n\n\
             **层级A—否定技术启示：**\n\
             - D1与本申请的技术领域是否相同？具体分析：……\n\
             - 本领域技术人员是否有动机将D1与D2结合？具体分析：……\n\
             - D1与D2之间是否存在反向教导？具体分析：……\n\
             - 综上，审查员的结合动机论证不成立，理由是：……\n\n\
             **层级B—强调协同技术效果：**\n\
             - 真正区别特征「区别特征A」产生的技术效果：……\n\
             - 区别特征A+B组合后的协同效果：……\n\
             - 对比文献未意识到该效果：……\n\
             - 说明书中是否有支持数据：……\n\n\
             **层级C—权利要求修改方案（如需要）：**\n\
             - 建议将区别特征A引入权利要求X：……\n\
             - 修改后的权利要求：……\n\
             - 修改后的区别特征及创造性论证：……\n\
             - A33合规性说明：修改未超出原说明书和权利要求书记载的范围，理由是……\n\n\
             {section5_prompt}",
            ),
        )
    }

    fn build_abnormal_prompt(
        my_patent: &str,
        oa: &str,
        refs: &str,
        depth: &str,
        discuss: bool,
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

        let section5_prompt = if discuss {
            "\n\n---\n\
             注意：**本次仅输出第一部分至第四部分的分析内容。**\n\
             用户将在审阅分析结果并进行充分讨论后，再单独要求生成第五部分（意见陈述书草稿）。\n\
             因此请专注于提供详尽、准确的分析（第一部分至第四部分），确保每一个判断都有据可查。"
        } else {
            "\n\n## 第五部分：意见陈述书草稿\n\
             生成可直接提交的意见陈述书，包括：\n\
             - 事实陈述\n\
             - 法律依据\n\
             - 请求撤销非正常认定"
        };

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
                 {section5_prompt}",
            ),
        )
    }

    fn build_reject_review_prompt(
        my_patent: &str,
        oa: &str,
        refs: &str,
        depth: &str,
        discuss: bool,
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

        let section4_prompt = if discuss {
            "\n\n---\n\
             注意：**本次仅输出第一部分至第三部分的分析内容。**\n\
             用户将在审阅分析结果并进行充分讨论后，再单独要求生成第四部分（复审请求书草稿）。\n\
             因此请专注于提供详尽、准确的分析（第一部分至第三部分），确保每一个判断都有据可查。"
        } else {
            "\n\n## 第四部分：复审请求书草稿\n\
             生成可直接提交的复审请求书，包括：\n\
             - 驳回决定的错误或不当之处\n\
             - 本申请具备创造性的理由\n\
             - 修改说明（如有）"
        };

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
                 {section4_prompt}",
            ),
        )
    }

    /// Helper: extract the response draft (第五部分) from the structured OA output.
    /// Falls back to the full text if the marker is not found.
    fn extract_oa_response_section(full_output: &str) -> String {
        // Look for the response section markers
        let response_markers = [
            "=== 第五部分：意见陈述书草稿 ===",
            "## 第五部分：意见陈述书草稿",
            "=== 第五部分",
            "## 第五部分",
            "意见陈述书草稿",
            "## 意见陈述书",
        ];
        for marker in &response_markers {
            if let Some(pos) = full_output.find(marker) {
                return full_output[pos..].to_string();
            }
        }
        // Fallback: return the last half of the output (heuristic for response part)
        let mid = full_output.len() / 2;
        full_output[mid..].to_string()
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

    /// 流式 OA 分析：返回 SSE chunk 接收端 / Streaming OA analysis
    /// 对于 deep 模式，先输出主分析，再输出审查员视角预判。
    pub fn office_action_response_stream(
        &self,
        my_patent_info: &str,
        office_action: &str,
        references_info: &str,
        oa_type: &str,
        depth: &str,
        discuss: bool,
    ) -> tokio::sync::mpsc::Receiver<String> {
        let (tx, rx) = tokio::sync::mpsc::channel::<String>(64);

        let my_patent_str = safe_truncate(my_patent_info, 30000);
        let oa_str = safe_truncate(office_action, 20000).to_string();
        let refs_str = safe_truncate(references_info, 30000);
        let is_deep = depth == "deep";

        let (system_role, prompt) = match oa_type {
            "abnormal" => Self::build_abnormal_prompt(&my_patent_str, &oa_str, &refs_str, depth, discuss),
            "reject_review" => {
                Self::build_reject_review_prompt(&my_patent_str, &oa_str, &refs_str, depth, discuss)
            }
            _ => Self::build_first_exam_prompt(&my_patent_str, &oa_str, &refs_str, depth, discuss),
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

        let self_clone = self.clone();
        Self::spawn_oa_stream_worker(tx, self_clone, messages, is_deep, oa_str);

        rx
    }

    /// Spawn the OA streaming worker with all owned data (no &str params to avoid lifetime issues).
    fn spawn_oa_stream_worker(
        tx: tokio::sync::mpsc::Sender<String>,
        self_clone: AiClient,
        messages: Vec<Message>,
        is_deep: bool,
        oa_str: String,
    ) {
        tokio::spawn(async move {
            // Phase 1: main analysis
            let mut stream1 = self_clone.send_chat_stream(messages, 0.3);
            let mut full_text = String::new();
            while let Some(chunk) = stream1.recv().await {
                if chunk.starts_with("[ERROR]") {
                    let _ = tx.send(chunk).await;
                    return;
                }
                full_text.push_str(&chunk);
                if tx.send(chunk).await.is_err() {
                    return;
                }
            }

            if !is_deep {
                return;
            }

            // Deep mode: phase 2 — self-critique from examiner perspective
            let response_part = AiClient::extract_oa_response_section(&full_text);
            let separator = "\n\n---\n\n## 审查员视角预判（AI 自检）\n";
            if tx.send(separator.to_string()).await.is_err() {
                return;
            }

            let critique_prompt = format!(
                "你是一位资深中国专利审查员，具有 20 年实质审查经验。\
                 请用审查员视角审阅以下答复方案，逐条指出：\n\n\
                 1. **逻辑漏洞**：方案中的哪些论断存在推理跳跃或证据不足\n\
                 2. **容易被反驳的点**：如果是你来审，你会从哪里切入反驳\n\
                 3. **遗漏的关键点**：申请人可能漏掉了哪些重要论据\n\
                 4. **修改建议**：如果要让这份方案更站得住脚，应该加强哪几个方向\n\
                 5. **特征对比准确性**：方案中对对比文献公开内容的认定是否有误？\
                 如果有，请指出审查员认为正确的认定是什么\n\n\
                 请用简洁、直接的语气，每个要点用一两句话说清楚，不要客套。\
                 如果方案中引用了对比文献的具体段落号，请特别注意核实其准确性。\n\n\
                 ## 审查意见通知书\n{}\n\n\
                 ## 拟提交的答复方案\n{}",
                safe_truncate(&oa_str, 8000),
                safe_truncate(&response_part, 12000),
            );

            let critique_messages = vec![
                Message {
                    role: "system".into(),
                    content: "你是一位资深中国专利审查员（执业20年，曾任复审委员会成员）。\
                             你精通中国专利法及审查指南，对创造性审查（A22.3）尤为严格。\
                             你善于发现答复方案中的逻辑漏洞和论证不足。\
                             你对对比文献公开内容的认定极其敏感——如果申请人歪曲了对比文献的公开内容，\
                             你会毫不留情地指出。\
                             请用挑剔、专业的眼光审阅，不需要客气。".into(),
                },
                Message {
                    role: "user".into(),
                    content: critique_prompt,
                },
            ];

            let mut stream2 = self_clone.send_chat_stream(critique_messages, 0.3);
            while let Some(chunk) = stream2.recv().await {
                if chunk.starts_with("[ERROR]") {
                    let _ = tx.send(chunk).await;
                    return;
                }
                if tx.send(chunk).await.is_err() {
                    return;
                }
            }
        });
    }

    /// Generate the formal response letter (第五部分) after discussion confirms the analysis.
    /// Takes the confirmed analysis text, discussion history, and original OA.
    pub fn generate_response_letter_stream(
        &self,
        analysis_text: &str,
        discussion_json: &str,
        office_action: &str,
        oa_type: &str,
    ) -> tokio::sync::mpsc::Receiver<String> {
        let (tx, rx) = tokio::sync::mpsc::channel::<String>(64);

        let analysis = safe_truncate_chars(analysis_text, 60000);   // 6万中文字
        let oa = safe_truncate_chars(office_action, 15000);          // 1.5万字 OA
        let discussion = safe_truncate_chars(discussion_json, 40000); // 4万讨论

        let doc_type = match oa_type {
            "abnormal" => "意见陈述书（非正常申请答辩）",
            "reject_review" => "复审请求书",
            _ => "意见陈述书",
        };

        let has_discussion = discussion.len() > 10 && discussion != "[]";
        let discussion_instruction = if has_discussion {
            "\n\n## 关键要求：融合讨论中的修改意见\n\
             讨论中发明人提出了修改意见且 AI 同意了的部分，\
             **必须**在意见陈述书中体现这些修改。具体来说：\n\
             - 如果讨论中确认某个技术特征被审查员误解，答复书中必须强调这一点\n\
             - 如果讨论中确认需要修改权利要求，答复书中必须包含修改后的权利要求\n\
             - 如果讨论中确认某个对比文献的技术领域不同，答复书中必须引用此论点\n\
             - 在答复书末尾附一段简短的「修改要点说明」，列出根据讨论做了哪些关键调整\n\
             这些修改意见优于原始分析——如果讨论和分析有矛盾，以讨论结论为准。"
        } else {
            ""
        };

        let system_prompt = format!(
            "你是一位资深中国专利代理师（执业20年+），精通中国专利法及审查指南。\
             你的任务是基于已确认的分析结果和讨论内容，撰写一份格式规范、逻辑严密、可直接提交的{doc_type}。\
             要求：\n\
             1. 严格基于分析结果，不得引入分析中未讨论的新论点\n\
             2. 讨论中发明人提出的修改意见是最高优先级——必须据此调整答复书内容\n\
             3. 语言专业、严谨、有据，每处论断均引用分析中的具体内容\n\
             4. 格式规范，符合国家知识产权局要求的文书格式\n\
             5. 语气尊重但坚定，不卑不亢\n\
             6. 必须逐条回应审查员的每一项驳回理由，不得遗漏\n\
             7. 创造性论述必须包含完整的三步法：区别特征→实际解决的技术问题→非显而易见性{discussion_instruction}"
        );

        let user_prompt = format!(
            "## 原始审查意见通知书\n{oa}\n\n\
             ## 已确认的分析结果（1-4部分）\n{analysis}\n\n\
             ## 讨论记录（含已确认的修改意见）\n{discussion}\n\n\
             请基于上述材料，生成完整的{doc_type}。注意：\n\
             1. 讨论中达成一致的修改意见必须体现在答复书中\n\
             2. 如果讨论中没有涉及某条驳回理由，按原始分析处理\n\
             3. 答复书末尾附「修改要点说明」摘要\n\n\
             输出格式：\n\n\
             === 意见陈述书 ===\n\n\
             **专利号**：【从分析中提取】\n\
             **发明名称**：【从分析中提取】\n\n\
             尊敬的审查员：\n\n\
             针对贵局于【日期】发出的第【通知书编号】号审查意见通知书，\
             申请人根据专利法及其实施细则的规定，特此陈述意见如下：\n\n\
             一、关于审查意见的答复\n\
             （逐条回应审查员的驳回理由，引用分析中的具体论证。如有讨论中确认的修改点，在此体现。）\n\n\
             二、关于权利要求的修改（如有）\n\
             （逐条说明修改内容及A33合规性。如讨论中确认了具体修改方案，务必采用。）\n\n\
             三、关于创造性的论述\n\
             （逐权利要求论述区别特征、技术效果、非显而易见性。使用三步法。）\n\n\
             四、结论\n\
             综上所述，申请人认为本申请符合专利法及其实施细则的相关规定，\
             请求审查员在考虑上述意见陈述的基础上，重新审查并授予专利权。\n\n\
             ## 修改要点说明\n\
             （列出本次答复中根据讨论结果做出的关键修改，2-3条即可）\n\n\
             此致\n\n\
             申请人：【待填写】\n\
             日期：【待填写】"
        );

        let messages = vec![
            Message {
                role: "system".into(),
                content: system_prompt,
            },
            Message {
                role: "user".into(),
                content: user_prompt,
            },
        ];

        let self_clone = self.clone();
        tokio::spawn(async move {
            let mut stream = self_clone.send_chat_stream(messages, 0.3);
            while let Some(chunk) = stream.recv().await {
                if tx.send(chunk).await.is_err() {
                    return;
                }
            }
        });

        rx
    }
}
