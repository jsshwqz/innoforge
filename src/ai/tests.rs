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
