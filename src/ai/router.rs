//! 模块职责：表驱动四角色 AI 模型路由器（v0.9.69 收口）。
//! 上游调用者：`src/routes/mod.rs::AppConfig::ai_client_for_role` 与 `GET /api/settings/model-router`。
//! 模块专属禁区：全仓四类模型角色必须由此单一常量路由表驱动，严禁各业务模块硬编码模型名或另起炉灶。

use serde::{Deserialize, Serialize};

/// 创研台四角色模型路由枚举（对应 AGENTS.md §2.1 四类通道）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelRole {
    /// 快模型：检索改写、关键词扩展、候选重排、四元组初拆
    FastRewrite,
    /// 强推理专家模型：全文档研发深剖析、OA 答辩策略、权利要求攻防与专利报告
    DeepReasoning,
    /// 多模态看图模型：专利附图部件识别、结构拓扑视觉理解
    VisionMultimodal,
    /// 严格 JSON / 校验模型：低温度结构化抽取、事实核验与合规检查
    StrictJsonVerifier,
}

/// 单条静态角色路由规格（表驱动配置）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModelRouteSpec {
    pub role: ModelRole,
    pub role_key: &'static str,
    pub label_zh: &'static str,
    pub default_temperature: f32,
    pub recommended_timeout_secs: u64,
    pub channel: &'static str,
    pub purpose_zh: &'static str,
}

/// 唯一权威四角色路由表。
pub const MODEL_ROUTING_TABLE: [ModelRouteSpec; 4] = [
    ModelRouteSpec {
        role: ModelRole::FastRewrite,
        role_key: "fast_rewrite",
        label_zh: "快模型改写通道",
        default_temperature: 0.2,
        recommended_timeout_secs: 25,
        channel: "fast",
        purpose_zh: "查询改写、关键词扩展、检索重排与研发问题四元组初拆",
    },
    ModelRouteSpec {
        role: ModelRole::DeepReasoning,
        role_key: "deep_reasoning",
        label_zh: "强推理专家通道",
        default_temperature: 0.35,
        recommended_timeout_secs: 180,
        channel: "expert",
        purpose_zh: "全文档深度研发剖析、多维推演、权要攻防与专利代理师报告",
    },
    ModelRouteSpec {
        role: ModelRole::VisionMultimodal,
        role_key: "vision_multimodal",
        label_zh: "视觉多模态通道",
        default_temperature: 0.2,
        recommended_timeout_secs: 90,
        channel: "vision",
        purpose_zh: "专利附图部件识别、机械/电路拓扑看图与图文核对",
    },
    ModelRouteSpec {
        role: ModelRole::StrictJsonVerifier,
        role_key: "strict_json_verifier",
        label_zh: "严格 JSON 校验通道",
        default_temperature: 0.1,
        recommended_timeout_secs: 45,
        channel: "fast",
        purpose_zh: "低温度结构化 JSON 提取、事实核验与证据链一致性校验",
    },
];

/// 返回指定角色的静态路由规格。
pub fn route_spec(role: ModelRole) -> &'static ModelRouteSpec {
    match role {
        ModelRole::FastRewrite => &MODEL_ROUTING_TABLE[0],
        ModelRole::DeepReasoning => &MODEL_ROUTING_TABLE[1],
        ModelRole::VisionMultimodal => &MODEL_ROUTING_TABLE[2],
        ModelRole::StrictJsonVerifier => &MODEL_ROUTING_TABLE[3],
    }
}

/// 解析角色字符串（用于诊断与测试）。
pub fn parse_role(key: &str) -> Option<ModelRole> {
    match key.trim().to_ascii_lowercase().as_str() {
        "fast_rewrite" | "fast" => Some(ModelRole::FastRewrite),
        "deep_reasoning" | "expert" => Some(ModelRole::DeepReasoning),
        "vision_multimodal" | "vision" => Some(ModelRole::VisionMultimodal),
        "strict_json_verifier" | "verifier" | "json" => Some(ModelRole::StrictJsonVerifier),
        _ => None,
    }
}

/// 面向前端设置页与诊断 API 的实时路由表快照行。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelRouteInfo {
    pub role: ModelRole,
    pub role_key: String,
    pub label_zh: String,
    pub channel: String,
    pub resolved_model: String,
    pub ready: bool,
    pub default_temperature: f32,
    pub recommended_timeout_secs: u64,
    pub purpose_zh: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routing_table_covers_all_four_roles_with_distinct_temperatures() {
        assert_eq!(MODEL_ROUTING_TABLE.len(), 4);
        assert_eq!(route_spec(ModelRole::FastRewrite).channel, "fast");
        assert_eq!(route_spec(ModelRole::DeepReasoning).channel, "expert");
        assert_eq!(route_spec(ModelRole::VisionMultimodal).channel, "vision");
        assert_eq!(route_spec(ModelRole::StrictJsonVerifier).channel, "fast");
        assert!(
            route_spec(ModelRole::StrictJsonVerifier).default_temperature
                < route_spec(ModelRole::DeepReasoning).default_temperature
        );
        assert_eq!(parse_role("deep_reasoning"), Some(ModelRole::DeepReasoning));
        assert_eq!(parse_role("unknown_role"), None);
    }
}
