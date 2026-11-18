//! 多模态自适应解析、专利附图词典与多轮共创内核（`src/core/multimodal/`）
//!
//! 对上层卡片暴露统一服务门面 [`MultimodalService`]：
//! - [`doc_ingest`]：PDF / DOCX / CSV / 图像 / 文本三级自适应解析链（原生文字层 → 本机 Umi-OCR → Vision 看图模型，含涉密离线锁）；
//! - [`figure_marks`]：中英文专利附图部件标号提取、多实施例归属与正文悬停对照；
//! - [`session`]：多轮图文附件共创会话与《结构化技术方案基线卡》增量收敛。

pub mod doc_ingest;
pub mod figure_marks;
pub mod session;

use crate::core::vault::models::FigureNumeral;
use anyhow::Result;

pub use doc_ingest::{
    evaluate_page_text_quality, extract_embedded_images_from_pdf, ingest_attachment, IngestOptions,
    IngestTierUsed, IngestedAttachment,
};
pub use figure_marks::{annotate_text_with_numerals, extract_figure_numerals};
pub use session::{
    isolate_user_input, CocreationTurnInput, CocreationTurnOutput, MultimodalSession,
    MultimodalSessionManager, SchemeBaselineCard, SessionTurnRecord,
};

/// 多模态内核统一服务门面
#[derive(Debug, Clone, Default)]
pub struct MultimodalService {
    default_options: IngestOptions,
    sessions: MultimodalSessionManager,
}

impl MultimodalService {
    pub fn new(default_options: IngestOptions) -> Self {
        Self {
            default_options,
            sessions: MultimodalSessionManager::new(),
        }
    }

    pub fn session_manager(&self) -> &MultimodalSessionManager {
        &self.sessions
    }

    pub async fn ingest_attachment(
        &self,
        filename: &str,
        mime_type: Option<&str>,
        bytes: &[u8],
        override_options: Option<&IngestOptions>,
    ) -> Result<IngestedAttachment> {
        let opts = override_options.unwrap_or(&self.default_options);
        doc_ingest::ingest_attachment(filename, mime_type, bytes, opts).await
    }

    pub async fn advance_cocreation_turn(
        &self,
        session_id: &str,
        input: CocreationTurnInput,
        llm_endpoint: Option<&(String, String, String)>,
    ) -> Result<CocreationTurnOutput> {
        self.sessions
            .advance_cocreation_turn(session_id, input, llm_endpoint)
            .await
    }

    pub fn extract_figure_numerals(&self, doc_id: &str, text: &str) -> Vec<FigureNumeral> {
        figure_marks::extract_figure_numerals(doc_id, text)
    }

    pub fn annotate_text_with_numerals(
        &self,
        text: &str,
        glossary: &[FigureNumeral],
    ) -> Vec<(String, String)> {
        figure_marks::annotate_text_with_numerals(text, glossary)
    }
}
