//! 创意分析与图片描述 / Idea analysis and image description

use super::client::{extract_chat_content, AiClient, Message};
use anyhow::Result;
use std::time::Duration;

impl AiClient {
    pub async fn analyze_idea(
        &self,
        title: &str,
        description: &str,
        web_findings: &str,
        patent_findings: &str,
    ) -> Result<String> {
        let prompt = format!(
            "## 用户的想法\n\
             **标题：** {title}\n\
             **描述：** {description}\n\n\
             ## 网络上的相关发现\n\
             {web_findings}\n\n\
             ## 相关专利\n\
             {patent_findings}\n\n\
             请从以下几个方面进行深入分析，用 Markdown 格式返回：\n\n\
             ### 1. 新颖性评估\n\
             - 与已有方案的相似度（给出 0-100 的新颖性评分，100=完全原创）\n\
             - 哪些部分是已有的，哪些是创新的\n\n\
             ### 2. 已有方案分析\n\
             - 列出最相关的已有方案/产品/专利\n\
             - 分析它们的优缺点\n\n\
             ### 3. 差异化方向\n\
             - 与已有方案的关键差异\n\
             - 可以进一步拉开差距的方向\n\n\
             ### 4. 优化建议\n\
             - 技术实现路径建议\n\
             - 可以增强竞争力的功能点\n\
             - 潜在的商业化方向\n\n\
             ### 5. 风险提示\n\
             - 可能的技术壁垒\n\
             - 潜在的知识产权风险\n\
             - 市场竞争风险\n\n\
             最后请在分析最开头用一行给出新颖性评分，格式严格为：\n\
             **新颖性评分：XX/100**"
        );

        let messages = vec![
            Message {
                role: "system".into(),
                content: "你是一个专业的创新分析师和技术顾问。你会客观地评估用户想法的新颖性，\
                         对比已有方案，并提供建设性的改进建议。回答要全面、有深度、实用。"
                    .into(),
            },
            Message {
                role: "user".into(),
                content: prompt,
            },
        ];

        self.send_chat(messages, 0.7).await
    }

    /// Send a vision request to describe an image (using multimodal API like GLM-4V).
    pub async fn describe_image(&self, image_data_url: &str) -> Result<String> {
        let request_body = serde_json::json!({
            "model": self.primary.model,
            "messages": [{
                "role": "user",
                "content": [
                    {
                        "type": "text",
                        "text": "请详细描述这张图片中的技术内容。如果包含文字，请提取所有文字。\
                                 如果是技术图纸、流程图、结构图或专利附图，请详细描述其技术方案和结构特征。"
                    },
                    {
                        "type": "image_url",
                        "image_url": { "url": image_data_url }
                    }
                ]
            }],
            "temperature": 0.3
        });

        let max_retries = 2;
        let mut last_err = None;
        for attempt in 0..max_retries {
            if attempt > 0 {
                let delay = Duration::from_secs(3);
                tokio::time::sleep(delay).await;
            }

            match self
                .client
                .post(format!("{}/chat/completions", self.primary.base_url))
                .header("Authorization", format!("Bearer {}", self.primary.api_key))
                .json(&request_body)
                .send()
                .await
            {
                Ok(resp) => {
                    let raw_text = resp.text().await?;
                    let content = extract_chat_content(&raw_text);
                    if content.starts_with("AI 错误") && attempt < max_retries - 1 {
                        last_err = Some(anyhow::anyhow!("{}", content));
                        continue;
                    }
                    return Ok(content);
                }
                Err(e) => {
                    last_err = Some(e.into());
                    if attempt < max_retries - 1 {
                        continue;
                    }
                }
            }
        }

        Err(last_err.unwrap_or_else(|| anyhow::anyhow!("Image analysis failed")))
    }
}
