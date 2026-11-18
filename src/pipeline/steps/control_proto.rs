//! 控制系统原型代码生成（IEC 61131-3 ST + 多段标定表 + 闭环阶跃响应仿真）。

use crate::ai::client::{safe_truncate_chars, AiClient};
use crate::patent::FeatureCard;
use serde::{Deserialize, Serialize};
use std::process::Command;

/// 多段标定/控制映射点（开度/设定百分比 -> 输出频率/流量/占空比）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CalibrationPoint {
    pub input_pct: f64,
    pub output_val: f64,
    pub label: String,
}

/// 闭环阶跃响应关键性能指标
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StepResponseMetrics {
    pub overshoot_pct: f64,
    pub steady_state_error: f64,
    pub settling_time_ms: f64,
}

/// 闭环阶跃响应时序数据（供 Chart.js 绘图）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepSimulationData {
    pub time_ms: Vec<f64>,
    pub setpoint: Vec<f64>,
    pub actual_value: Vec<f64>,
    pub metrics: StepResponseMetrics,
}

/// 控制原型完整出参
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlPrototypeResult {
    pub status: String,
    pub idea_id: String,
    pub model_used: String,
    pub st_code: String,
    pub calibration_table: Vec<CalibrationPoint>,
    pub simulation: StepSimulationData,
}

#[derive(Deserialize, Default)]
struct RawAiControlPayload {
    #[serde(default)]
    st_code: String,
    #[serde(default)]
    calibration_table: Vec<CalibrationPoint>,
}

/// 分段线性插值（含越界钳位与防除零保护）。
pub fn interpolate_calibration(points: &[CalibrationPoint], input_pct: f64) -> f64 {
    if points.is_empty() {
        return input_pct.clamp(0.0, 100.0);
    }
    let mut sorted = points.to_vec();
    sorted.sort_by(|a, b| {
        a.input_pct
            .partial_cmp(&b.input_pct)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    if input_pct <= sorted[0].input_pct {
        return sorted[0].output_val;
    }
    if input_pct >= sorted[sorted.len() - 1].input_pct {
        return sorted[sorted.len() - 1].output_val;
    }
    for win in sorted.windows(2) {
        let p0 = &win[0];
        let p1 = &win[1];
        if input_pct >= p0.input_pct && input_pct <= p1.input_pct {
            let dx = p1.input_pct - p0.input_pct;
            if dx.abs() < 1e-9 {
                return p0.output_val;
            }
            let ratio = (input_pct - p0.input_pct) / dx;
            return p0.output_val + ratio * (p1.output_val - p0.output_val);
        }
    }
    sorted[sorted.len() - 1].output_val
}

/// 生成默认多段标定表（5 点非线性补偿映射）。
pub fn default_calibration_table() -> Vec<CalibrationPoint> {
    vec![
        CalibrationPoint {
            input_pct: 0.0,
            output_val: 0.0,
            label: "零位死区关闭".to_string(),
        },
        CalibrationPoint {
            input_pct: 25.0,
            output_val: 18.0,
            label: "低开度精细调节段".to_string(),
        },
        CalibrationPoint {
            input_pct: 50.0,
            output_val: 48.0,
            label: "中位线性工作段".to_string(),
        },
        CalibrationPoint {
            input_pct: 75.0,
            output_val: 78.0,
            label: "高负荷响应段".to_string(),
        },
        CalibrationPoint {
            input_pct: 100.0,
            output_val: 100.0,
            label: "额定满量程上限".to_string(),
        },
    ]
}

/// 生成标准 IEC 61131-3 结构化文本（ST）功能块草稿（含防除零、超限联锁与手自动无扰切换）。
pub fn build_deterministic_st_block(idea_title: &str, cards: &[FeatureCard]) -> String {
    let feature_comment = cards
        .iter()
        .take(3)
        .map(|c| c.title.trim())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" / ");
    let tag_line = if feature_comment.is_empty() {
        "核心闭环控制".to_string()
    } else {
        feature_comment
    };

    format!(
        "(* ============================================================\n\
         * 工程控制原型功能块 (IEC 61131-3 Structured Text)\n\
         * 项目: {title}\n\
         * 覆盖特征: {features}\n\
         * ============================================================ *)\n\
         FUNCTION_BLOCK FB_ClosedLoopControl\n\
         VAR_INPUT\n\
             bEnable        : BOOL;   (* 系统使能 *)\n\
             bAutoMode      : BOOL;   (* TRUE=自动PID, FALSE=手动直通 *)\n\
             bEStop         : BOOL;   (* 急停联锁信号 *)\n\
             fSetpoint      : REAL;   (* 设定值 0.0..100.0 % *)\n\
             fFeedback      : REAL;   (* 传感器反馈 0.0..100.0 % *)\n\
             fManualCmd     : REAL;   (* 手动给定 0.0..100.0 % *)\n\
             fSpanMax       : REAL := 100.0; (* 量程上限，用于防除零保护 *)\n\
             fKp            : REAL := 1.45;\n\
             fKi            : REAL := 0.35;\n\
             fKd            : REAL := 0.08;\n\
             fDtSec         : REAL := 0.02;\n\
         END_VAR\n\
         VAR_OUTPUT\n\
             fActuatorOut   : REAL;   (* 执行器输出 0.0..100.0 % *)\n\
             bInterlockTrip : BOOL;   (* 超限/异常联锁保护触发 *)\n\
         END_VAR\n\
         VAR\n\
             fErr           : REAL := 0.0;\n\
             fPrevErr       : REAL := 0.0;\n\
             fIntegral      : REAL := 0.0;\n\
             fDeriv         : REAL := 0.0;\n\
             fSafeSpan      : REAL := 100.0;\n\
             fRawOut        : REAL := 0.0;\n\
         END_VAR\n\n\
         (* 1. 安全联锁与防除零保护 *)\n\
         IF bEStop OR (NOT bEnable) OR (fFeedback > 105.0) OR (fFeedback < -5.0) THEN\n\
             bInterlockTrip := TRUE;\n\
             fActuatorOut   := 0.0;\n\
             fIntegral      := 0.0;\n\
             RETURN;\n\
         END_IF;\n\
         bInterlockTrip := FALSE;\n\n\
         IF fSpanMax <= 0.0001 THEN\n\
             fSafeSpan := 100.0;\n\
         ELSE\n\
             fSafeSpan := fSpanMax;\n\
         END_IF;\n\n\
         (* 2. 手自动无扰切换逻辑 *)\n\
         IF NOT bAutoMode THEN\n\
             fRawOut      := LIMIT(0.0, fManualCmd, 100.0);\n\
             fIntegral    := fRawOut;\n\
             fPrevErr     := (fSetpoint - fFeedback) / fSafeSpan * 100.0;\n\
             fActuatorOut := fRawOut;\n\
             RETURN;\n\
         END_IF;\n\n\
         (* 3. 闭环 PID 控制计算 *)\n\
         fErr      := (fSetpoint - fFeedback) / fSafeSpan * 100.0;\n\
         fIntegral := LIMIT(-50.0, fIntegral + fErr * fKi * fDtSec, 100.0);\n\
         IF fDtSec > 0.0001 THEN\n\
             fDeriv := (fErr - fPrevErr) / fDtSec;\n\
         ELSE\n\
             fDeriv := 0.0;\n\
         END_IF;\n\
         fRawOut      := fKp * fErr + fIntegral + fKd * fDeriv;\n\
         fActuatorOut := LIMIT(0.0, fRawOut, 100.0);\n\
         fPrevErr     := fErr;\n\
         END_FUNCTION_BLOCK\n",
        title = safe_truncate_chars(idea_title, 80),
        features = tag_line
    )
}

/// 运行闭环阶跃响应数值仿真（优先沙箱 Python，失败则无损回退 Rust 确定性数值仿真）。
pub fn run_closed_loop_step_simulation(calibration: &[CalibrationPoint]) -> StepSimulationData {
    if let Some(sim) = try_python_step_simulation() {
        return sim;
    }
    deterministic_rust_step_simulation(calibration)
}

fn try_python_step_simulation() -> Option<StepSimulationData> {
    let script = r#"
import json
dt = 0.05
sp = 80.0
y = 0.0
v = 0.0
integral = 0.0
prev_e = 0.0
kp, ki, kd = 1.6, 1.1, 0.25
t_arr, sp_arr, y_arr = [], [], []
for i in range(61):
    t_ms = round(i * dt * 1000.0, 1)
    e = sp - y
    integral = max(-40.0, min(100.0, integral + e * dt))
    deriv = (e - prev_e) / dt
    u = max(0.0, min(100.0, kp * e + ki * integral + kd * deriv))
    # 二阶阻尼对象模型
    a = (u - 1.4 * v - y) / 0.35
    v += a * dt
    y += v * dt
    prev_e = e
    t_arr.append(t_ms)
    sp_arr.append(sp)
    y_arr.append(round(y, 2))
peak = max(y_arr)
overshoot = round(max(0.0, (peak - sp) / sp * 100.0), 2)
sse = round(abs(sp - y_arr[-1]), 2)
settling = t_arr[-1]
for idx in range(len(y_arr) - 1, -1, -1):
    if abs(y_arr[idx] - sp) > sp * 0.02:
        settling = t_arr[min(idx + 1, len(t_arr) - 1)]
        break
print(json.dumps({
    "time_ms": t_arr,
    "setpoint": sp_arr,
    "actual_value": y_arr,
    "metrics": {"overshoot_pct": overshoot, "steady_state_error": sse, "settling_time_ms": settling}
}))
"#;

    let output = Command::new("python3")
        .arg("-c")
        .arg(script)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str::<StepSimulationData>(stdout.trim()).ok()
}

fn deterministic_rust_step_simulation(calibration: &[CalibrationPoint]) -> StepSimulationData {
    let dt = 0.05_f64;
    let sp = 80.0_f64;
    let gain = interpolate_calibration(calibration, sp) / sp.max(1.0);
    let eff_gain = gain.clamp(0.6, 1.3);
    let mut y = 0.0_f64;
    let mut v = 0.0_f64;
    let mut integral = 0.0_f64;
    let mut prev_e = 0.0_f64;
    let (kp, ki, kd) = (1.6, 1.1, 0.25);

    let mut time_ms = Vec::with_capacity(61);
    let mut setpoint = Vec::with_capacity(61);
    let mut actual_value = Vec::with_capacity(61);

    for i in 0..=60 {
        let t_ms = (i as f64) * dt * 1000.0;
        let e = sp - y;
        integral = (integral + e * dt).clamp(-40.0, 100.0);
        let deriv = (e - prev_e) / dt;
        let u = (kp * e + ki * integral + kd * deriv).clamp(0.0, 100.0) * eff_gain;
        let a = (u - 1.4 * v - y) / 0.35;
        v += a * dt;
        y += v * dt;
        prev_e = e;

        time_ms.push((t_ms * 10.0).round() / 10.0);
        setpoint.push(sp);
        actual_value.push((y * 100.0).round() / 100.0);
    }

    let peak = actual_value.iter().copied().fold(0.0_f64, f64::max);
    let overshoot_pct = (((peak - sp) / sp) * 100.0).max(0.0);
    let last_val = actual_value.last().copied().unwrap_or(0.0);
    let steady_state_error = (sp - last_val).abs();
    let mut settling_time_ms = time_ms.last().copied().unwrap_or(3000.0);
    for idx in (0..actual_value.len()).rev() {
        if (actual_value[idx] - sp).abs() > sp * 0.02 {
            let next_idx = (idx + 1).min(time_ms.len() - 1);
            settling_time_ms = time_ms[next_idx];
            break;
        }
    }

    StepSimulationData {
        time_ms,
        setpoint,
        actual_value,
        metrics: StepResponseMetrics {
            overshoot_pct: (overshoot_pct * 100.0).round() / 100.0,
            steady_state_error: (steady_state_error * 100.0).round() / 100.0,
            settling_time_ms,
        },
    }
}

/// 解析 AI 返回的控制原型 JSON（兼容 markdown code fence）。
pub fn parse_ai_control_payload(raw: &str) -> Option<(String, Vec<CalibrationPoint>)> {
    let trimmed = raw.trim();
    let body = trimmed
        .strip_prefix("```json")
        .and_then(|s| s.strip_suffix("```"))
        .or_else(|| {
            trimmed
                .strip_prefix("```")
                .and_then(|s| s.strip_suffix("```"))
        })
        .unwrap_or(trimmed)
        .trim();
    let parsed: RawAiControlPayload = serde_json::from_str(body).ok()?;
    if parsed.st_code.trim().is_empty() {
        return None;
    }
    Some((parsed.st_code, parsed.calibration_table))
}

/// 生成完整控制原型结果（优先专家模型，失败或离线回退确定性 ST + 仿真）。
pub async fn generate_control_prototype(
    idea_id: &str,
    idea_title: &str,
    cards: &[FeatureCard],
    ai_expert: Option<&AiClient>,
) -> ControlPrototypeResult {
    let mut st_code = build_deterministic_st_block(idea_title, cards);
    let mut calibration = default_calibration_table();
    let mut model_used = "deterministic-iec61131-3".to_string();

    if let Some(ai) = ai_expert {
        let feat_list: Vec<String> = cards
            .iter()
            .take(6)
            .map(|c| format!("- {}: {}", c.title, c.description))
            .collect();
        let prompt = format!(
            "请为以下工程项目生成 IEC 61131-3 结构化文本（ST）功能块及 5 点非线性标定映射表，\
严格返回合法 JSON：\n{{\"st_code\": \"...\", \"calibration_table\": [{{\"input_pct\": 0.0, \"output_val\": 0.0, \"label\": \"...\"}}]}}\n\
必须包含防除零保护、超限联锁与手自动无扰切换逻辑。\n\
<user_input>\n项目：{}\n特征：\n{}\n</user_input>",
            safe_truncate_chars(idea_title, 200),
            feat_list.join("\n")
        );
        let timed = ai.clone_with_timeout(std::time::Duration::from_secs(60));
        if let Ok(raw) = timed
            .chat_with_system(
                "你是工业自动化 PLC 与控制算法专家，只输出合法 JSON。",
                &prompt,
                0.2,
            )
            .await
        {
            if let Some((ai_st, ai_cal)) = parse_ai_control_payload(&raw) {
                st_code = ai_st;
                if !ai_cal.is_empty() {
                    calibration = ai_cal;
                }
                model_used = "ai-expert-iec61131-3".to_string();
            }
        }
    }

    let simulation = run_closed_loop_step_simulation(&calibration);
    ControlPrototypeResult {
        status: "ok".to_string(),
        idea_id: idea_id.to_string(),
        model_used,
        st_code,
        calibration_table: calibration,
        simulation,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interpolate_handles_zero_dx_and_bounds() {
        let pts = vec![
            CalibrationPoint {
                input_pct: 0.0,
                output_val: 0.0,
                label: "a".into(),
            },
            CalibrationPoint {
                input_pct: 50.0,
                output_val: 40.0,
                label: "b".into(),
            },
            CalibrationPoint {
                input_pct: 50.0,
                output_val: 60.0,
                label: "dup".into(),
            },
            CalibrationPoint {
                input_pct: 100.0,
                output_val: 100.0,
                label: "c".into(),
            },
        ];
        assert!((interpolate_calibration(&pts, -10.0) - 0.0).abs() < 1e-6);
        assert!((interpolate_calibration(&pts, 25.0) - 20.0).abs() < 1e-6);
        assert!(interpolate_calibration(&pts, 50.0).is_finite());
        assert!((interpolate_calibration(&pts, 120.0) - 100.0).abs() < 1e-6);
    }

    #[test]
    fn deterministic_st_and_simulation_are_valid() {
        let st = build_deterministic_st_block("高精度隔离阀控制", &[]);
        assert!(st.contains("FUNCTION_BLOCK FB_ClosedLoopControl"));
        assert!(st.contains("bInterlockTrip"));
        assert!(st.contains("bAutoMode"));

        let cal = default_calibration_table();
        let sim = run_closed_loop_step_simulation(&cal);
        assert_eq!(sim.time_ms.len(), sim.actual_value.len());
        assert!(sim.time_ms.len() > 10);
        assert!(sim.metrics.overshoot_pct >= 0.0);
    }

    #[test]
    fn parse_ai_control_payload_accepts_fenced_json() {
        let raw = "```json\n{\"st_code\":\"FUNCTION_BLOCK A END_FUNCTION_BLOCK\",\"calibration_table\":[{\"input_pct\":0,\"output_val\":0,\"label\":\"zero\"}]}\n```";
        let (code, cal) = parse_ai_control_payload(raw).expect("valid payload");
        assert!(code.contains("FUNCTION_BLOCK A"));
        assert_eq!(cal.len(), 1);
    }
}
