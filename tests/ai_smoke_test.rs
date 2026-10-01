//! §6.3 真实 AI 冒烟测试
//!
//! 这些测试需要真实的 AI API key 才能运行，因此标记为 `#[ignore]`。
//! 运行方式：`cargo test --test ai_smoke_test -- --ignored`
//!
//! 环境变量：
//! - DEEPSEEK_API_KEY: DeepSeek API 密钥（主路径）
//! - 可选 OPENAI_API_KEY: OpenAI 兼容接口密钥（备选路径）
//!
//! 测试覆盖：
//! 1. DeepSeek 基础对话 — 验证 AI client 能成功调用并返回内容
//! 2. 用量追踪 — 验证 take_last_usage 返回合理的 token 计数
//! 3. 专利摘要 — 验证 summarize_patent 端到端可用
//! 4. OA 分析 — 验证 office_action_response 端到端可用
//! 5. 语义搜索/向量化 — 验证 TF-IDF embedding 和余弦相似度
//! 6. RAG 检索 — 验证 RAG pipeline 端到端可用
//! 7. 成本统计 — 多次调用后的 token 汇总

use innoforge::ai::client::AiClient;
use innoforge::ai::patent::AiClient as PatentClient;
use innoforge::db::Database;
use innoforge::patent::Patent;
use innoforge::vector::{compute_tfidf_embedding, cosine_similarity, VectorIndex};
use innoforge::rag::rag_search;

// ── 辅助函数 ──

fn deepseek_key() -> Option<String> {
    std::env::var("DEEPSEEK_API_KEY").ok().filter(|k| !k.is_empty())
}

fn deepseek_client() -> AiClient {
    let key = deepseek_key().expect("DEEPSEEK_API_KEY must be set for smoke tests");
    AiClient::with_config(
        "https://api.deepseek.com/v1",
        &key,
        "deepseek-chat",
    )
}

fn deepseek_patent_client() -> PatentClient {
    let key = deepseek_key().expect("DEEPSEEK_API_KEY must be set for smoke tests");
    PatentClient::with_config(
        "https://api.deepseek.com/v1",
        &key,
        "deepseek-chat",
    )
}

fn make_test_patent(id: &str) -> Patent {
    Patent {
        id: id.to_string(),
        patent_number: "CN12345678A".to_string(),
        title: "基于深度学习的图像识别方法".to_string(),
        abstract_text: "本发明使用卷积神经网络对图像进行分类识别，提高了识别准确率。".to_string(),
        description: String::new(),
        claims: String::new(),
        applicant: "测试".to_string(),
        inventor: String::new(),
        filing_date: String::new(),
        publication_date: "2026-01-01".to_string(),
        grant_date: None,
        ipc_codes: String::new(),
        cpc_codes: String::new(),
        priority_date: String::new(),
        country: "CN".to_string(),
        kind_code: String::new(),
    }
}

// ── 1. DeepSeek 基础对话 ──

#[tokio::test]
#[ignore]
async fn deepseek_basic_chat_returns_content() {
    let client = deepseek_client().with_chat_timeout();
    let result = client
        .send_rag_chat("请用一句话回答：1+1等于几？", 0.0)
        .await;

    assert!(result.is_ok(), "DeepSeek chat should succeed: {:?}", result.err());
    let content = result.unwrap();
    assert!(!content.is_empty(), "response content should not be empty");
    assert!(
        content.contains('2') || content.contains("两") || content.contains("二"),
        "response should indicate the answer is 2, got: {content}"
    );
}

// ── 2. 用量追踪 ──

#[tokio::test]
#[ignore]
async fn deepseek_usage_tracking_returns_token_counts() {
    let client = deepseek_client().with_chat_timeout();
    let _ = client
        .send_rag_chat("请用一句话介绍人工智能。", 0.5)
        .await
        .expect("chat should succeed");

    let usage = client.take_last_usage();
    assert!(usage.is_some(), "usage should be tracked after a call");
    let usage = usage.unwrap();
    assert!(
        usage.input_tokens > 0,
        "input tokens should be positive, got: {}",
        usage.input_tokens
    );
    assert!(
        usage.output_tokens > 0,
        "output tokens should be positive, got: {}",
        usage.output_tokens
    );

    println!(
        "[SMOKE] DeepSeek usage: input={} tokens, output={} tokens",
        usage.input_tokens, usage.output_tokens
    );
}

// ── 3. 专利摘要 ──

#[tokio::test]
#[ignore]
async fn deepseek_summarize_patent_produces_summary() {
    let client = deepseek_patent_client().with_analysis_timeout();
    let result = client
        .summarize_patent(
            "一种基于深度学习的图像识别方法",
            "本发明涉及一种基于深度学习的图像识别方法，包括以下步骤：\
             (1)获取待识别图像；(2)对图像进行预处理；(3)将预处理后的图像\
             输入预训练的卷积神经网络模型；(4)输出识别结果。\
             该方法能够提高图像识别的准确率和效率。",
            "1. 一种图像识别方法，包括获取图像、预处理、卷积神经网络识别步骤。",
        )
        .await;

    assert!(result.is_ok(), "summarize_patent should succeed: {:?}", result.err());
    let summary = result.unwrap();
    assert!(!summary.is_empty(), "summary should not be empty");
    assert!(
        summary.contains("图像") || summary.contains("识别") || summary.contains("深度学习"),
        "summary should mention key technology, got: {summary}"
    );

    println!("[SMOKE] Patent summary length: {} chars", summary.len());
}

// ── 4. OA 分析（浅度模式，快速验证） ──

#[tokio::test]
#[ignore]
async fn deepseek_oa_analysis_shallow_returns_sections() {
    let client = deepseek_patent_client().with_analysis_timeout();
    let result = client
        .office_action_response(
            "权利要求1：一种图像识别方法，包括获取图像、预处理、卷积神经网络识别步骤。",
            "审查员认为权利要求1相对于对比文件D1不具备创造性。\
             D1（CN12345678A）公开了一种图像处理方法，包括图像获取和预处理步骤。",
            "D1：CN12345678A，一种图像处理方法。",
            "first_exam",
            "shallow",
            false,
        )
        .await;

    assert!(result.is_ok(), "OA analysis should succeed: {:?}", result.err());
    let analysis = result.unwrap();
    assert!(!analysis.is_empty(), "analysis should not be empty");
    assert!(
        analysis.len() > 50,
        "analysis should be substantial (>50 chars), got {} chars",
        analysis.len()
    );

    println!("[SMOKE] OA analysis length: {} chars", analysis.len());
}

// ── 5. 语义搜索/向量化 ──

#[test]
#[ignore]
fn tfidf_embedding_and_cosine_similarity_work() {
    let text1 = "图像识别 深度学习 卷积神经网络";
    let text2 = "图像处理 深度学习 神经网络";
    let text3 = "机械制造 金属加工 数控机床";

    let emb1 = compute_tfidf_embedding(text1);
    let emb2 = compute_tfidf_embedding(text2);
    let emb3 = compute_tfidf_embedding(text3);

    assert!(!emb1.is_empty(), "embedding should not be empty");
    assert!(!emb2.is_empty(), "embedding should not be empty");
    assert!(!emb3.is_empty(), "embedding should not be empty");

    let sim_12 = cosine_similarity(&emb1, &emb2);
    let sim_13 = cosine_similarity(&emb1, &emb3);

    // 相关文本相似度应高于不相关文本
    assert!(
        sim_12 > sim_13,
        "related texts should have higher similarity: sim(1,2)={sim_12} should > sim(1,3)={sim_13}"
    );

    println!(
        "[SMOKE] TF-IDF cosine similarity: related={sim_12:.4}, unrelated={sim_13:.4}"
    );
}

#[test]
#[ignore]
fn embedding_persistence_with_database() {
    let db = Database::init(":memory:").expect("in-memory db should work");
    let index = VectorIndex::default();

    // 插入一条专利
    let patent = make_test_patent("smoke-001");
    db.insert_patent(&patent).expect("insert should work");

    // 计算并保存 embedding
    let text = format!("{} {}", patent.title, patent.abstract_text);
    let result = innoforge::vector::compute_and_save_embedding(&index, &db, "smoke-001", &text);
    assert!(result.is_ok(), "embedding save should succeed: {:?}", result.err());

    // 验证 embedding 已保存
    let saved = db.get_patent_embedding("smoke-001").expect("embedding query should work");
    assert!(saved.is_some(), "embedding should be saved in db");
    let saved_vec = saved.unwrap();
    assert!(!saved_vec.is_empty(), "saved embedding should not be empty");

    println!("[SMOKE] Embedding persisted: {} dimensions", saved_vec.len());
}

// ── 6. RAG 检索 ──

#[tokio::test]
#[ignore]
async fn rag_search_end_to_end() {
    let db = Database::init(":memory:").expect("in-memory db should work");
    let index = VectorIndex::default();

    // 插入测试专利
    let patent = make_test_patent("rag-001");
    db.insert_patent(&patent).expect("insert should work");

    // 构建 embedding
    let text = format!("{} {}", patent.title, patent.abstract_text);
    let _ = innoforge::vector::compute_and_save_embedding(&index, &db, "rag-001", &text);

    // 执行 RAG 搜索
    let client = deepseek_client().with_chat_timeout();
    let result = rag_search(
        &client,
        &db,
        "rag-001",
        "这个专利的技术方案是什么？",
        "你是一位专利分析专家，请基于检索到的上下文回答问题。",
        2000,
    )
    .await;

    assert!(result.is_ok(), "RAG search should succeed: {:?}", result.err());
    let rag_result = result.unwrap();
    assert!(!rag_result.answer.is_empty(), "RAG answer should not be empty");

    println!(
        "[SMOKE] RAG search: answer={} chars, chunks={}, rag_enabled={}",
        rag_result.answer.len(),
        rag_result.chunk_count,
        rag_result.rag_enabled
    );
}

// ── 7. 成本统计汇总 ──

#[tokio::test]
#[ignore]
async fn cost_stats_summary_after_multiple_calls() {
    let client = deepseek_client().with_chat_timeout();

    let mut total_input = 0i64;
    let mut total_output = 0i64;

    for i in 1..=3 {
        let prompt = format!("请用一句话回答：{}乘以2等于多少？", i);
        let result = client.send_rag_chat(&prompt, 0.0).await;
        assert!(result.is_ok(), "call {} should succeed", i);

        if let Some(usage) = client.take_last_usage() {
            total_input += usage.input_tokens;
            total_output += usage.output_tokens;
        }
    }

    assert!(total_input > 0, "total input tokens should be positive");
    assert!(total_output > 0, "total output tokens should be positive");

    println!(
        "[SMOKE] Cost stats: 3 calls, total input={} tokens, total output={} tokens, total={}",
        total_input,
        total_output,
        total_input + total_output
    );
}
