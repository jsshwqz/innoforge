#[cfg(test)]
mod extraction_tests {
    use crate::ai::client::extract_chat_content;

    #[test]
    fn extracts_openai_message_content() {
        let raw = r#"{"choices":[{"message":{"role":"assistant","content":"hello"}}]}"#;
        assert_eq!(extract_chat_content(raw), "hello");
    }

    #[test]
    fn extracts_provider_result_field() {
        let raw = r#"{"result":"provider text"}"#;
        assert_eq!(extract_chat_content(raw), "provider text");
    }

    #[test]
    fn formats_provider_error_message() {
        let raw = r#"{"error":{"message":"bad request"}}"#;
        assert_eq!(extract_chat_content(raw), "AI 错误：bad request");
    }
}

#[cfg(test)]
mod ai_data_truncation_tests {
    use crate::ai::client::truncate_for_ai;

    #[test]
    fn short_text_passes_through_untouched() {
        assert_eq!(truncate_for_ai("短文本", 100), "短文本");
    }

    #[test]
    fn text_at_limit_is_not_rewritten() {
        let text = "中".repeat(100);
        assert_eq!(truncate_for_ai(&text, 100), text);
    }

    #[test]
    fn overflow_keeps_head_and_marks_omission_explicitly() {
        let text = "中".repeat(120);
        let out = truncate_for_ai(&text, 100);
        assert!(out.starts_with(&"中".repeat(100)));
        assert!(out.contains("原文共 120 个字符"));
        assert!(out.contains("只送入前 100 个字符"));
        assert!(out.contains("其余 20 个字符未包含"));
        assert!(out.contains("禁止推测"));
    }

    #[test]
    fn limit_counts_characters_not_bytes() {
        // 每个 emoji 占 4 字节；按字节截断会切坏字符，按字符截断不会。
        let text = "🦀".repeat(60);
        assert_eq!(truncate_for_ai(&text, 60), text);
        let out = truncate_for_ai(&text, 59);
        assert!(out.contains("原文共 60 个字符"));
        assert!(out.starts_with(&"🦀".repeat(59)));
    }

    #[test]
    fn zero_budget_returns_only_the_notice() {
        let out = truncate_for_ai("abc", 0);
        assert!(out.starts_with("\n\n【数据完整性提示"));
        assert!(out.contains("原文共 3 个字符"));
    }
}

#[cfg(test)]
mod oa_capacity_tests {
    use crate::ai::client::{
        oa_capacity_error, AiClient, OA_DISCUSSION_ANALYSIS_MAX_CHARS,
        OA_RESPONSE_ANALYSIS_MAX_CHARS,
    };

    #[test]
    fn short_oa_input_passes_without_truncation() {
        assert!(
            oa_capacity_error("analysis", "短文本", OA_DISCUSSION_ANALYSIS_MAX_CHARS).is_none()
        );
    }

    #[test]
    fn unicode_capacity_uses_character_count() {
        let within = "中".repeat(OA_DISCUSSION_ANALYSIS_MAX_CHARS);
        assert!(oa_capacity_error("analysis", &within, OA_DISCUSSION_ANALYSIS_MAX_CHARS).is_none());

        let over = format!("{}中", within);
        let error = oa_capacity_error("analysis", &over, OA_DISCUSSION_ANALYSIS_MAX_CHARS)
            .expect("Unicode overflow should be visible");
        assert!(error.contains("actual_chars=60001"));
        assert!(error.contains("max_chars=60000"));
    }

    #[tokio::test]
    async fn response_letter_stream_emits_visible_overflow_error() {
        let client = AiClient::with_config("http://127.0.0.1:1", "", "test");
        let analysis = "中".repeat(OA_RESPONSE_ANALYSIS_MAX_CHARS + 1);
        let mut stream =
            client.generate_response_letter_stream(&analysis, "[]", "", "first_exam", false);
        let error = stream.recv().await.expect("overflow should emit an error");
        assert!(error.starts_with("[ERROR] OA_INPUT_TOO_LARGE"));
        assert!(error.contains("actual_chars=600001"));
    }
}
