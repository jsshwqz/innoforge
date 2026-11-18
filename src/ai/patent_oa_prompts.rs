//! OA 审查意见提示词构建方法 / OA prompt builder methods for `AiClient`

use super::*;

impl AiClient {
    pub(super) fn build_first_exam_prompt(
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
        let mech = MECHANISM_ANALYSIS;
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
         6. **多维度对比**：除逐特征对比外，还需从技术领域、技术问题、技术效果、应用场景等维度综合评估\n\
         7. **组合动机分析**：审查员将多篇对比文件结合时，必须分析各文件的技术领域是否相同、解决的技术问题是否一致、是否存在反向教导\n\
         8. **AI输出风格限制**：你的输出将直接用于撰写正式意见陈述书，因此必须严格遵守以下规范：\n\
            - 不使用加粗、表情符号（✅❌⚠️）或过度格式化\n\
            - 表格可用，但不在表格内加粗\n\
            - 避免「综上所述」「经分析可知」等模板化过渡语\n\
            - 变化句式，不每段以「申请人」开头\n\
            - 全部使用中文术语，不夹带英文\n\
            - 语气尊重审查员但立场坚定，不攻击、不示弱\n\
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
             | 权1 | 特征1.2: …… | 未公开 | D2第[00XX]段公开了…… | 未公开 | ★ 区别特征A |\n\
             | 权1 | 特征1.3: …… | 未公开 | 未公开 | 未公开 | ★ 区别特征B |\n\
             | 权2 | 特征2.1: …… | …… | …… | …… | …… |\n\n\
             注意：\n\
             - 每项技术特征单独一行\n\
             - 「公开情况」必须写「公开了……（引用具体段落）」或「未公开」\n\
             - 「真正区别特征」列：如果该特征在所有对比文献中均未公开，标★并命名；否则标—\n\
             - 最后加一行「审查员遗漏的区别特征总结」\n\n\
             {mech}\n\n\
             === 第四部分：逐权利要求反驳论点 ===\n\n\
             基于上述对比总表，对审查员驳回的**每项权利要求**，按层级逐条写出反驳：\n\n\
             **权利要求X：**\n\n\
             **层级A—否定技术启示：**\n\
             - D1与本申请的技术领域是否相同？具体分析：……\n\
             - 本领域技术人员是否有动机将D1与D2结合？具体分析：……\n\
             - D1与D2之间是否存在反向教导？具体分析：……\n\
             - 各对比文件解决的技术问题是否相同？\n\
             - 各对比文件之间是否存在相互引用关系？\n\
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

    pub(super) fn build_abnormal_prompt(
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

        let mech = MECHANISM_ANALYSIS;
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
                 {mech}\n\n\
                 ## 第四部分：逐条反驳\n\
                 针对通知书中的每条认定理由：\n\
                 - 正面回应，不回避\n\
                 - 提供事实依据和技术说明\n\
                 - 必要时附证据组织建议\n\n\
                 {section5_prompt}",
            ),
        )
    }

    pub(super) fn build_reject_review_prompt(
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

        let mech = MECHANISM_ANALYSIS;
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
             4. 撰写符合国家知识产权局专利局复审和无效审理部要求的复审请求书\n\
             请用严谨、专业的语言，结论要有理有据，可直接用于提交。"
                .into(),
            format!(
                "## 我的专利（权利要求书+说明书）\n{my_patent}\n\n\
                 ## 驳回决定\n{oa}\n\n\
                 ## 对比文献\n{refs}\n\n\
                 请基于以上材料，按以下六角度结构化框架生成完整的驳回后复审请求方案。\n\
                 每个角度须独立成节，标题固定，禁止合并或更改标题。\n\n\
                 ## 一、事实对照\n\
                 - 驳回决定逐段拆解，明确每段认定的技术事实\n\
                 - 权利要求逐项特征矩阵：列出每项权利要求的全部技术特征\n\
                 - 「对比文件哪段公开/未公开哪个特征」映射表（表格形式：权项特征 × D1/D2/… × 公开/未公开/部分公开）\n\n\
                 {mech}\n\n\
                 ## 二、法条适用\n\
                 引用法条与三步法每一步逐一验真：\n\
                 - 步骤1：确定最接近现有技术（给出结论：成立/不成立/部分成立）\n\
                 - 步骤2：确定区别特征与实际解决的技术问题（给出结论）\n\
                 - 步骤3：判断显而易见性（给出结论）\n\
                 每步须有明确结论，不得含糊。\n\n\
                 ## 三、技术效果\n\
                 区别特征带来的技术效果逐条列举，每条须标注「原说明书是否记载该效果」（是/否+段落号）。\n\
                 效果论证须与驳回决定认定的效果做对比。\n\n\
                 ## 四、策略权衡树\n\
                 至少给出两条论证路径（主路径 + 备选路径），每条标注：\n\
                 - 胜率评估（高/中/低）\n\
                 - 风险点\n\
                 - 依赖前提\n\
                 路径间标注「若主路径不成立则转备选」的衔接关系。\n\n\
                 ## 五、程序适配\n\
                 复审程序与实审程序的关键差异说明（举证责任、审查范围、前置审查可能性）。\n\
                 本案在复审程序中的特殊注意事项。\n\n\
                 ## 六、说服结构\n\
                 论证编排建议（先攻哪条理由、后攻哪条、修改与论证如何配合）。\n\
                 ### 预判合议组质疑\n\
                 列出 2-3 个合议组可能提出的质疑，每个质疑给出预先回应：\n\
                 1. [质疑内容] → [预先回应]\n\
                 2. [质疑内容] → [预先回应]\n\
                 3. [质疑内容] → [预先回应]\n\n\
                 {section4_prompt}\n\n\
                 ---\n\n\
                 ## 出处锚点纪律\n\
                 所有事实断言必须带出处锚点，格式：【依据：<出处类型>：<定位信息>】\n\
                 合法出处类型（禁止自造）：\n\
                 - 驳回决定：第X段 / 第X页第X段\n\
                 - 权利要求：权项X / 权项X第X特征\n\
                 - 说明书：第X段 / 第X段第X行\n\
                 - 说明书附图：图X / 图X中标记Y\n\
                 - 对比文件：DX第X段 / DX第X页 / DX权利要求X\n\
                 - 法条：专利法第X条 / 审查指南第X章第X节\n\
                 - 审查指南：第X部分第X章第X节\n\
                 边界规则：\n\
                 - AI 自身的推理/分析不是合法出处——须追溯到具体材料出处\n\
                 - 无法定位到具体段落的笼统引用禁止使用\n\
                 - 无法溯源的断言须降级为【需申请人确认】或删除\n\
                 输出末尾附「出处对照表」（序号 | 断言摘要 | 出处类型 | 定位信息）",
            ),
        )
    }

    /// 构建一审历史上下文注入段落（T3: 一审-二审对比分析）
    pub(super) fn build_first_exam_context_section(first_exam_context: Option<&str>) -> String {
        match first_exam_context {
            Some(ctx) if !ctx.is_empty() => {
                format!("## 一审历史对比\n\n<user_input>\n{ctx}\n</user_input>\n\n")
            }
            _ => String::new(),
        }
    }

    /// 二审驳回决定答复 prompt
    pub(super) fn build_second_rejection_prompt(
        my_patent: &str,
        oa: &str,
        refs: &str,
        depth: &str,
        discuss: bool,
        first_exam_context: Option<&str>,
    ) -> (String, String) {
        if depth == "shallow" {
            return (
                "你是一位资深中国专利代理师，精通二审驳回答复。请提供简要答复思路。".into(),
                format!(
                    "## 我的专利\n{my_patent}\n\n## 二审驳回决定\n{oa}\n\n## 对比文献\n{refs}\n\n{}                     请生成简要的二审驳回答复方案：\n                     1. 二审驳回的核心问题（一句话概括）\n                     2. 与一审驳回的差异分析\n                     3. 答复的核心理由\n                     4. 答复意见书草稿（简要版）",
                    Self::build_first_exam_context_section(first_exam_context),
                ),
            );
        }
        let mech = MECHANISM_ANALYSIS;
        let section4 = if discuss {
            "\n\n---\n注意：**本次仅输出第一部分至第三部分的分析内容。**\n用户将在审阅后单独要求生成第四部分（答复意见书草稿）。"
        } else {
            "\n\n## 第四部分：答复意见书草稿\n生成可直接提交的答复意见书，包括：\n- 二审驳回决定的错误或不当之处\n- 本申请具备创造性的理由\n- 修改说明（如有）"
        };
        (
            "你是一位资深中国专利代理师，精通中国专利二审程序及驳回答复。             你擅长分析二审驳回决定中的审查逻辑，找出与一审的差异和新的推理漏洞，             在维持或修改权利要求的基础上提出有说服力的答复理由。"
                .into(),
            format!(
                "## 我的专利（权利要求书+说明书）\n{my_patent}\n\n                 ## 二审驳回决定\n{oa}\n\n                 ## 对比文献\n{refs}\n\n                 {}                 请基于以上材料，按以下结构化框架生成完整的二审驳回答复方案。\n\n                 ## 一、事实对照\n                 - 二审驳回决定逐段拆解，明确每段认定的技术事实\n                 - 与一审驳回决定的差异对比（新增理由/变更理由/维持理由）\n                 - 权利要求逐项特征矩阵\n\n                 {mech}\n\n                 ## 二、法条适用\n                 引用法条与三步法每一步逐一验真，标注与一审结论的差异。\n\n                 ## 三、技术效果\n                 区别特征带来的技术效果逐条列举，与驳回决定认定的效果做对比。\n\n                 ## 四、策略权衡树\n                 至少给出两条论证路径（主路径 + 备选路径），每条标注胜率评估和风险点。\n\n                 ## 五、程序适配\n                 二审程序特殊注意事项，前置审查驳回后的应对策略。\n\n                 ## 六、说服结构\n                 论证编排建议，预判合议组质疑并给出预先回应。\n\n                 {section4}",
                Self::build_first_exam_context_section(first_exam_context),
            ),
        )
    }

    /// 复审请求书 prompt
    pub(super) fn build_reexamination_request_prompt(
        my_patent: &str,
        oa: &str,
        refs: &str,
        depth: &str,
        discuss: bool,
    ) -> (String, String) {
        if depth == "shallow" {
            return (
                "你是一位资深中国专利代理师，精通专利复审请求程序（专利法第41条）。请提供简要复审请求思路。".into(),
                format!(
                    "## 我的专利\n{my_patent}\n\n## 驳回决定\n{oa}\n\n## 对比文献\n{refs}\n\n                     请生成简要的复审请求方案：\n                     1. 驳回决定的核心问题\n                     2. 复审请求的核心理由\n                     3. 复审请求书草稿（简要版）",
                ),
            );
        }
        let mech = MECHANISM_ANALYSIS;
        let section4 = if discuss {
            "\n\n---\n注意：**本次仅输出第一部分至第三部分的分析内容。**\n用户将在审阅后单独要求生成第四部分（复审请求书草稿）。"
        } else {
            "\n\n## 第四部分：复审请求书草稿\n生成可直接提交的复审请求书，包括：\n- 驳回决定的错误或不当之处\n- 本申请具备创造性的理由\n- 修改说明（如有）"
        };
        (
            "你是一位资深中国专利代理师，精通中国专利复审程序（专利法第41条）。             你擅长深入分析驳回决定中的审查逻辑，找出推理漏洞，             在维持或修改权利要求的基础上提出有说服力的复审理由，             撰写符合国家知识产权局专利局复审和无效审理部要求的复审请求书。"
                .into(),
            format!(
                "## 我的专利（权利要求书+说明书）\n{my_patent}\n\n                 ## 驳回决定\n{oa}\n\n                 ## 对比文献\n{refs}\n\n                 请基于以上材料，按以下结构化框架生成完整的复审请求方案。\n\n                 ## 一、事实对照\n                 - 驳回决定逐段拆解，明确每段认定的技术事实\n                 - 权利要求逐项特征矩阵\n                 - 对比文件公开/未公开映射表\n\n                 {mech}\n\n                 ## 二、法条适用\n                 引用法条与三步法每一步逐一验真。\n\n                 ## 三、技术效果\n                 区别特征带来的技术效果逐条列举。\n\n                 ## 四、策略权衡树\n                 至少给出两条论证路径，标注胜率评估和风险点。\n\n                 ## 五、程序适配\n                 复审程序与实审程序的关键差异，举证责任、审查范围、前置审查可能性。\n\n                 ## 六、说服结构\n                 论证编排建议，预判合议组质疑并给出预先回应。\n\n                 {section4}",
            ),
        )
    }

    /// 复审决定答复 prompt
    pub(super) fn build_reexamination_decision_prompt(
        my_patent: &str,
        oa: &str,
        refs: &str,
        depth: &str,
        discuss: bool,
    ) -> (String, String) {
        if depth == "shallow" {
            return (
                "你是一位资深中国专利代理师，精通复审决定答复程序。请提供简要答复思路。".into(),
                format!(
                    "## 我的专利\n{my_patent}\n\n## 复审决定\n{oa}\n\n## 对比文献\n{refs}\n\n                     请生成简要的复审决定答复方案：\n                     1. 复审决定的核心问题\n                     2. 答复的核心理由\n                     3. 答复意见书草稿（简要版）",
                ),
            );
        }
        let mech = MECHANISM_ANALYSIS;
        let section4 = if discuss {
            "\n\n---\n注意：**本次仅输出第一部分至第三部分的分析内容。**\n用户将在审阅后单独要求生成第四部分（答复意见书草稿）。"
        } else {
            "\n\n## 第四部分：答复意见书草稿\n生成可直接提交的答复意见书。"
        };
        (
            "你是一位资深中国专利代理师，精通中国专利复审决定答复程序。             你擅长分析复审决定中的认定逻辑，针对复审委员会的审查意见提出有说服力的答复理由。"
                .into(),
            format!(
                "## 我的专利（权利要求书+说明书）\n{my_patent}\n\n                 ## 复审决定\n{oa}\n\n                 ## 对比文献\n{refs}\n\n                 请基于以上材料，按以下结构化框架生成完整的复审决定答复方案。\n\n                 ## 一、事实对照\n                 - 复审决定逐段拆解，明确每段认定的技术事实\n                 - 权利要求逐项特征矩阵\n\n                 {mech}\n\n                 ## 二、法条适用\n                 引用法条与三步法逐一验真。\n\n                 ## 三、技术效果\n                 区别特征带来的技术效果逐条列举。\n\n                 ## 四、策略权衡树\n                 至少给出两条论证路径，标注胜率评估和风险点。\n\n                 ## 五、程序适配\n                 复审决定后的程序选择（答复/起诉/撤回），期限和形式要求。\n\n                 ## 六、说服结构\n                 论证编排建议，预判合议组质疑。\n\n                 {section4}",
            ),
        )
    }

    /// 行政诉讼 prompt
    pub(super) fn build_admin_lawsuit_prompt(
        my_patent: &str,
        oa: &str,
        refs: &str,
        depth: &str,
        discuss: bool,
    ) -> (String, String) {
        if depth == "shallow" {
            return (
                "你是一位资深专利行政诉讼代理人，精通北京知识产权法院专利行政诉讼程序。请提供简要诉讼思路。".into(),
                format!(
                    "## 我的专利\n{my_patent}\n\n## 复审决定\n{oa}\n\n## 对比文献\n{refs}\n\n                     请生成简要的行政诉讼方案：\n                     1. 复审决定的核心错误\n                     2. 诉讼请求\n                     3. 起诉状草稿（简要版）",
                ),
            );
        }
        let mech = MECHANISM_ANALYSIS;
        let section4 = if discuss {
            "\n\n---\n注意：**本次仅输出第一部分至第三部分的分析内容。**\n用户将在审阅后单独要求生成第四部分（起诉状草稿）。"
        } else {
            "\n\n## 第四部分：起诉状草稿\n生成可直接提交的行政起诉状，包括：\n- 诉讼请求\n- 事实和理由\n- 证据清单"
        };
        (
            "你是一位资深专利行政诉讼代理人，精通中国专利法及行政诉讼法，             擅长分析复审决定的合法性与合理性，撰写专利行政起诉状。"
                .into(),
            format!(
                "## 我的专利（权利要求书+说明书）\n{my_patent}\n\n                 ## 复审决定\n{oa}\n\n                 ## 对比文献\n{refs}\n\n                 请基于以上材料，按以下结构化框架生成完整的行政诉讼方案。\n\n                 ## 一、事实对照\n                 - 复审决定逐段拆解，明确认定的事实和法律适用\n                 - 复审决定中的程序性问题和实体性问题\n\n                 {mech}\n\n                 ## 二、法律适用\n                 - 复审决定适用的法律条款逐一验真\n                 - 行政诉讼审查范围（合法性+合理性审查）\n                 - 举证责任分配\n\n                 ## 三、技术效果\n                 区别特征带来的技术效果，与复审决定认定的效果对比。\n\n                 ## 四、策略权衡树\n                 至少给出两条诉讼路径（主路径 + 备选路径），标注胜率评估和风险点。\n\n                 ## 五、程序适配\n                 行政诉讼程序要点：管辖法院（北京知识产权法院）、起诉期限（收到决定书之日起三个月内）、                 证据要求、庭审流程。\n\n                 ## 六、说服结构\n                 论证编排建议，预判法院可能的质疑。\n\n                 {section4}",
            ),
        )
    }

    /// P3-T1: 答复策略智能推荐 prompt
    /// 根据 OA 类型、对比文献特征、专利技术领域，构建策略推荐 prompt
    pub fn build_strategy_recommend_prompt(
        oa_type: &str,
        my_patent: &str,
        refs: &str,
        oa_text: &str,
    ) -> (String, String) {
        let system = "你是一位专利答复策略专家，精通中国专利法及审查指南。
\n            请根据以下信息，推荐最优答复策略。

\n            分析维度：
\n            1. OA 类型 → 答复紧迫度（一审有修改机会 vs 二审受限 vs 复审最后机会）
\n            2. 对比文献数量和相关性 → 翻案难度评估
\n            3. 权利要求 vs 对比文献 → 哪些权项有救、哪些难救
\n            4. 技术领域 → 该领域审查标准宽严度

\n            输出格式（严格按此结构）：
\n            ## 推荐策略
\n            ### 策略1：[名称]
\n            - 成功概率：X%
\n            - 优先级：⭐⭐⭐
\n            - 理由：...
\n            - 具体操作：...

\n            ### 策略2：[名称]
\n            ...

\n            ## 不推荐的策略
\n            ### [名称]
\n            - 原因：...

\n            ## 综合建议
\n            ...

\n            请用中文回答。";

        let user = format!(
            "## OA 类型
{oa_type}

## 我的专利
{my_patent}

## 审查意见
{oa_text}

## 对比文献
{refs}"
        );

        (system.into(), user)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── §6.1 新增测试：build_first_exam_prompt 结构 ──

    #[test]
    fn first_exam_medium_prompt_contains_five_sections() {
        let (_system, user) =
            AiClient::build_first_exam_prompt("专利", "OA", "对比", "medium", false);
        assert!(user.contains("第一部分"), "missing section 1");
        assert!(user.contains("第二部分"), "missing section 2");
        assert!(user.contains("第三部分"), "missing section 3");
        assert!(user.contains("第四部分"), "missing section 4");
        assert!(user.contains("第五部分"), "missing section 5");
    }

    #[test]
    fn first_exam_deep_prompt_contains_section5_draft() {
        let (_system, user) =
            AiClient::build_first_exam_prompt("专利", "OA", "对比", "deep", false);
        // 非 discuss 模式应包含意见陈述书草稿指令
        assert!(user.contains("意见陈述书草稿"));
        assert!(user.contains("关于审查意见的答复"));
        assert!(user.contains("关于权利要求的修改"));
        assert!(user.contains("关于创造性的论述"));
    }

    #[test]
    fn first_exam_discuss_mode_omits_section5() {
        let (_system, user) = AiClient::build_first_exam_prompt("专利", "OA", "对比", "deep", true);
        // discuss 模式仅输出第一至第四部分
        assert!(user.contains("仅输出第一部分至第四部分"));
    }

    #[test]
    fn first_exam_shallow_prompt_is_brief() {
        let (system, user) =
            AiClient::build_first_exam_prompt("专利", "OA", "对比", "shallow", false);
        assert!(system.contains("简明扼要"));
        assert!(user.contains("一句话概括"));
        assert!(!user.contains("第一部分"));
    }

    // ── §6.1 新增测试：build_abnormal_prompt 结构 ──

    #[test]
    fn abnormal_medium_prompt_contains_four_sections() {
        let (_system, user) =
            AiClient::build_abnormal_prompt("专利", "OA", "参考", "medium", false);
        assert!(user.contains("认定原因分析"), "missing section 1");
        assert!(user.contains("技术方案真实性论证"), "missing section 2");
        assert!(user.contains("研发过程合理性说明"), "missing section 3");
        assert!(user.contains("逐条反驳"), "missing section 4");
    }

    #[test]
    fn abnormal_shallow_prompt_is_brief() {
        let (system, user) =
            AiClient::build_abnormal_prompt("专利", "OA", "参考", "shallow", false);
        assert!(system.contains("简要"));
        assert!(user.contains("一句话概括"));
        assert!(!user.contains("认定原因分析"));
    }

    #[test]
    fn abnormal_deep_prompt_contains_section5() {
        let (_system, user) = AiClient::build_abnormal_prompt("专利", "OA", "参考", "deep", false);
        assert!(user.contains("第五部分"));
        assert!(user.contains("意见陈述书"));
    }

    #[test]
    fn abnormal_discuss_mode_omits_section5() {
        let (_system, user) = AiClient::build_abnormal_prompt("专利", "OA", "参考", "deep", true);
        assert!(user.contains("仅输出第一部分至第四部分"));
    }

    #[test]
    fn strategy_recommend_prompt_returns_system_and_user() {
        let (system, user) = AiClient::build_strategy_recommend_prompt(
            "second_rejection",
            "专利内容",
            "对比文献",
            "OA文本",
        );
        assert!(!system.is_empty(), "system prompt should not be empty");
        assert!(!user.is_empty(), "user prompt should not be empty");
    }

    #[test]
    fn strategy_recommend_prompt_user_contains_all_inputs() {
        let (_system, user) = AiClient::build_strategy_recommend_prompt(
            "first_exam",
            "我的发明专利关于火焰燃烧",
            "D1: CN123456",
            "审查意见：缺乏创造性",
        );
        assert!(user.contains("first_exam"), "user should contain oa_type");
        assert!(user.contains("火焰燃烧"), "user should contain patent text");
        assert!(
            user.contains("CN123456"),
            "user should contain reference text"
        );
        assert!(user.contains("缺乏创造性"), "user should contain OA text");
    }

    #[test]
    fn strategy_recommend_prompt_system_mentions_analysis_dimensions() {
        let (system, _user) =
            AiClient::build_strategy_recommend_prompt("second_rejection", "p", "r", "o");
        assert!(
            system.contains("答复策略专家"),
            "system should mention strategy expert role"
        );
        assert!(
            system.contains("成功概率"),
            "system should mention success probability"
        );
        assert!(system.contains("优先级"), "system should mention priority");
        assert!(
            system.contains("不推荐的策略"),
            "system should mention not-recommended section"
        );
    }

    #[test]
    fn strategy_recommend_prompt_handles_empty_inputs() {
        let (system, user) = AiClient::build_strategy_recommend_prompt("", "", "", "");
        assert!(
            !system.is_empty(),
            "system should still be provided for empty inputs"
        );
        assert!(
            user.contains("## OA 类型\n\n"),
            "user should handle empty oa_type"
        );
    }
}
