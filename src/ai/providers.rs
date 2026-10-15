//! AI Provider 预设配置 / Provider Presets
//!
//! 御三家（OpenAI + Anthropic Claude + Google Gemini）预设配置。
//! 用户在设置页面选择预设后，自动填入 base_url / model / expert_model。
//! 所有预设均走 OpenAI 兼容 API 格式，无需格式适配器。

/// 单个 Provider 预设
#[derive(Debug, Clone, serde::Serialize)]
#[allow(dead_code)]
pub struct ProviderPreset {
    /// 预设 ID（英文标识）
    pub id: &'static str,
    /// 显示名称
    pub display_name: &'static str,
    /// 描述
    pub description: &'static str,
    /// API Base URL（OpenAI 兼容端点）
    pub base_url: &'static str,
    /// API Key 环境变量名
    pub api_key_env: &'static str,
    /// 默认模型（日常使用）
    pub default_model: &'static str,
    /// 专家模型（高推理任务）
    pub expert_model: &'static str,
    /// 可用模型列表
    pub available_models: &'static [&'static str],
    /// 是否需要特殊认证头（如 Anthropic 的 anthropic-version）
    pub extra_headers: &'static [(&'static str, &'static str)],
    /// 官网注册地址
    pub signup_url: &'static str,
}

/// 御三家预设列表
#[allow(dead_code)]
pub static TOP3_PRESETS: &[ProviderPreset] = &[
    // ── OpenAI ──
    ProviderPreset {
        id: "openai",
        display_name: "OpenAI",
        description: "GPT-4o / o3 系列，原生 OpenAI API",
        base_url: "https://api.openai.com/v1",
        api_key_env: "OPENAI_API_KEY",
        default_model: "gpt-4o",
        expert_model: "o3-mini",
        available_models: &["gpt-4o", "gpt-4o-mini", "gpt-4.1", "o3-mini", "o3"],
        extra_headers: &[],
        signup_url: "https://platform.openai.com/api-keys",
    },
    // ── Anthropic Claude ──
    // 关键：Anthropic 已推出 OpenAI 兼容端点
    // https://api.anthropic.com/v1/chat/completions
    // 认证从 x-api-key 改为 Authorization: Bearer
    ProviderPreset {
        id: "anthropic",
        display_name: "Anthropic Claude",
        description: "Claude Sonnet/Opus 系列，通过 OpenAI 兼容端点接入",
        base_url: "https://api.anthropic.com/v1",
        api_key_env: "ANTHROPIC_API_KEY",
        default_model: "claude-sonnet-4-6",
        expert_model: "claude-opus-4",
        available_models: &["claude-sonnet-4-6", "claude-haiku-4-5", "claude-opus-4"],
        extra_headers: &[],
        signup_url: "https://console.anthropic.com/settings/keys",
    },
    // ── Google Gemini ──
    // OpenAI 兼容端点：在 v1beta 路径下加 /openai/
    ProviderPreset {
        id: "gemini",
        display_name: "Google Gemini",
        description: "Gemini 2.0/2.5 系列，OpenAI 兼容端点",
        base_url: "https://generativelanguage.googleapis.com/v1beta/openai/",
        api_key_env: "GEMINI_API_KEY",
        default_model: "gemini-2.0-flash",
        expert_model: "gemini-2.5-pro",
        available_models: &["gemini-2.0-flash", "gemini-2.5-flash", "gemini-2.5-pro"],
        extra_headers: &[],
        signup_url: "https://aistudio.google.com/apikey",
    },
];

/// 按 ID 查找预设
#[allow(dead_code)]
pub fn find_preset(id: &str) -> Option<&'static ProviderPreset> {
    TOP3_PRESETS.iter().find(|p| p.id == id)
}

/// 获取所有预设 ID
#[allow(dead_code)]
pub fn preset_ids() -> Vec<&'static str> {
    TOP3_PRESETS.iter().map(|p| p.id).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_presets_count() {
        assert_eq!(TOP3_PRESETS.len(), 3, "应该有 3 个预设");
    }

    #[test]
    fn test_find_openai() {
        let p = find_preset("openai").expect("OpenAI 预设应存在");
        assert!(p.base_url.contains("api.openai.com"));
        assert!(p.available_models.contains(&"gpt-4o"));
    }

    #[test]
    fn test_find_anthropic() {
        let p = find_preset("anthropic").expect("Anthropic 预设应存在");
        assert!(p.base_url.contains("api.anthropic.com"));
        assert!(p.available_models.contains(&"claude-sonnet-4-6"));
    }

    #[test]
    fn test_find_gemini() {
        let p = find_preset("gemini").expect("Gemini 预设应存在");
        assert!(p.base_url.contains("googleapis.com"));
        assert!(p.available_models.contains(&"gemini-2.5-pro"));
    }

    #[test]
    fn test_all_presets_have_models() {
        for p in TOP3_PRESETS {
            assert!(!p.available_models.is_empty(), "{} 应有可用模型", p.id);
            assert!(!p.default_model.is_empty(), "{} 应有默认模型", p.id);
            assert!(!p.expert_model.is_empty(), "{} 应有专家模型", p.id);
        }
    }
}
