//! OA 答复期限计算 —— 单一数据源 / Single source of truth
//!
//! 7 种 oa_type 全覆盖，abnormal 走 15 个工作日。
//! 电子送达按《专利法实施细则》第四条第三款：送达日 = 发文日，不加 15 天邮路推定。
//!
//! 两个模板（office_action_response.html / index.html）通过 `/api/oa/deadline-table`
//! + `static/oa-deadline.js` 引用本模块，不再各自手写三元式。

use axum::Json;
use chrono::{Datelike, Duration, Local, NaiveDate, Weekday};
use serde::{Deserialize, Serialize};

/// 期限类型：自然月 or 工作日
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", content = "value")]
pub enum DeadlineSpec {
    /// 自然月数（如 first_exam=4, second_rejection=2, reject_review=3 ...）
    Months(u32),
    /// 工作日数（如 abnormal=15）
    WorkDays(u32),
}

/// 期限计算结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeadlineResult {
    /// 发文日
    pub dispatch_date: String,
    /// 送达日（= 发文日，电子送达不加邮路推定）
    pub service_date: String,
    /// 届满日（截止日）
    pub deadline: String,
    /// 剩余天数（负数 = 已逾期）
    pub days_left: i64,
    /// 总天数（发文日到届满日）
    pub total_days: i64,
    /// 是否临近法定期限（≤7 天）
    pub is_urgent: bool,
    /// 期限类型描述
    pub spec: DeadlineSpec,
}

/// 期限表 —— 唯一数据源 / single source of truth
///
/// | oa_type | 月数/工作日 | 法律依据 |
/// |---|---|---|
/// | first_exam | 4 个月 | 一审通知书 |
/// | second_rejection | 2 个月 | 二审驳回 |
/// | reject_review | 3 个月 | 复审请求 |
/// | reexamination_request | 3 个月 | 专利法第41条 |
/// | reexamination_decision | 3 个月 | 对复审决定不服 |
/// | admin_lawsuit | 3 个月 | 收到通知日起3个月内起诉 |
/// | abnormal | 15 工作日 | 非正常申请通知书 |
pub fn deadline_spec(oa_type: &str) -> DeadlineSpec {
    match oa_type {
        "first_exam" => DeadlineSpec::Months(4),
        "second_rejection" => DeadlineSpec::Months(2),
        "reject_review" => DeadlineSpec::Months(3),
        "reexamination_request" => DeadlineSpec::Months(3),
        "reexamination_decision" => DeadlineSpec::Months(3),
        "admin_lawsuit" => DeadlineSpec::Months(3),
        "abnormal" => DeadlineSpec::WorkDays(15),
        _ => DeadlineSpec::Months(4), // 未知类型保守取 4 个月
    }
}

/// 计算工作日后的日期（跳过周六周日）
fn add_work_days(start: NaiveDate, days: u32) -> NaiveDate {
    let mut current = start;
    let mut added = 0u32;
    while added < days {
        current += Duration::days(1);
        let wd = current.weekday();
        if wd != Weekday::Sat && wd != Weekday::Sun {
            added += 1;
        }
    }
    current
}

/// 计算期限
///
/// `dispatch_date` = 发文日（ISO 格式 `YYYY-MM-DD`）
/// 送达日 = 发文日（电子送达，细则第四条第三款）
pub fn compute_deadline(oa_type: &str, dispatch_date: NaiveDate) -> DeadlineResult {
    let spec = deadline_spec(oa_type);

    let deadline = match spec {
        DeadlineSpec::Months(m) => {
            // chrono 的 with_month 溢出会自动滚到下月，需手动处理月末
            let mut d = dispatch_date;
            let total_months = d.year() as i64 * 12 + (d.month() as i64 - 1) + m as i64;
            let year = (total_months.div_euclid(12)) as i32;
            let month = (total_months.rem_euclid(12) + 1) as u32;
            let day = d.day();
            // 月末溢出：如 1/31 + 1 月 = 2/28（或 2/29）
            let last_day_of_month = NaiveDate::from_ymd_opt(year, month + 1, 1)
                .unwrap_or_else(|| NaiveDate::from_ymd_opt(year + 1, 1, 1).unwrap())
                - Duration::days(1);
            let actual_day = day.min(last_day_of_month.day());
            d = NaiveDate::from_ymd_opt(year, month, actual_day).unwrap_or(dispatch_date);
            d
        }
        DeadlineSpec::WorkDays(wd) => add_work_days(dispatch_date, wd),
    };

    let now = Local::now().date_naive();
    let days_left = (deadline - now).num_days();
    let total_days = (deadline - dispatch_date).num_days();
    let is_urgent = days_left <= 7;

    DeadlineResult {
        dispatch_date: dispatch_date.to_string(),
        service_date: dispatch_date.to_string(), // 电子送达：送达日 = 发文日
        deadline: deadline.to_string(),
        days_left,
        total_days,
        is_urgent,
        spec,
    }
}

/// API: `GET /api/oa/deadline-table` —— 返回期限表 JSON
pub async fn deadline_table() -> Json<serde_json::Value> {
    let table = serde_json::json!({
        "first_exam":              {"kind": "Months", "value": 4},
        "second_rejection":        {"kind": "Months", "value": 2},
        "reject_review":           {"kind": "Months", "value": 3},
        "reexamination_request":   {"kind": "Months", "value": 3},
        "reexamination_decision":  {"kind": "Months", "value": 3},
        "admin_lawsuit":           {"kind": "Months", "value": 3},
        "abnormal":                {"kind": "WorkDays", "value": 15},
    });
    Json(serde_json::json!({ "deadline_table": table }))
}

/// API: `GET /api/oa/compute-deadline?oa_type=...&date=YYYY-MM-DD`
pub async fn compute_deadline_api(
    axum::extract::Query(params): axum::extract::Query<ComputeDeadlineParams>,
) -> Json<serde_json::Value> {
    let date = match NaiveDate::parse_from_str(&params.date, "%Y-%m-%d") {
        Ok(d) => d,
        Err(_) => {
            return Json(serde_json::json!({
                "error": "Invalid date format, expected YYYY-MM-DD"
            }))
        }
    };
    let result = compute_deadline(&params.oa_type, date);
    Json(serde_json::json!(result))
}

#[derive(Debug, Deserialize)]
pub struct ComputeDeadlineParams {
    pub oa_type: String,
    pub date: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    // === 7 种 oa_type 逐个断言 ===

    #[test]
    fn test_first_exam_4_months() {
        let r = compute_deadline("first_exam", NaiveDate::from_ymd_opt(2026, 1, 15).unwrap());
        assert_eq!(r.deadline, "2026-05-15");
        assert_eq!(r.spec, DeadlineSpec::Months(4));
    }

    #[test]
    fn test_second_rejection_2_months() {
        let r = compute_deadline(
            "second_rejection",
            NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
        );
        assert_eq!(r.deadline, "2026-03-15");
        assert_eq!(r.spec, DeadlineSpec::Months(2));
    }

    #[test]
    fn test_reject_review_3_months() {
        let r = compute_deadline(
            "reject_review",
            NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
        );
        assert_eq!(r.deadline, "2026-04-15");
        assert_eq!(r.spec, DeadlineSpec::Months(3));
    }

    #[test]
    fn test_reexamination_request_3_months() {
        // 回归断言：reexamination_request 必须 = 发文日 + 3 个月
        // 本 bug 的核心：之前落到 else=4，显示 2026/11/10 而非 2026/10/10
        let r = compute_deadline(
            "reexamination_request",
            NaiveDate::from_ymd_opt(2026, 7, 10).unwrap(),
        );
        assert_eq!(r.deadline, "2026-10-10");
        assert_eq!(r.spec, DeadlineSpec::Months(3));
    }

    #[test]
    fn test_reexamination_decision_3_months() {
        let r = compute_deadline(
            "reexamination_decision",
            NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
        );
        assert_eq!(r.deadline, "2026-04-15");
        assert_eq!(r.spec, DeadlineSpec::Months(3));
    }

    #[test]
    fn test_admin_lawsuit_3_months() {
        let r = compute_deadline(
            "admin_lawsuit",
            NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
        );
        assert_eq!(r.deadline, "2026-04-15");
        assert_eq!(r.spec, DeadlineSpec::Months(3));
    }

    #[test]
    fn test_abnormal_15_working_days() {
        // 周五发文 → 15 工作日跨 3 个周末 = 21 自然日
        // 2026-01-02 是周五
        let r = compute_deadline("abnormal", NaiveDate::from_ymd_opt(2026, 1, 2).unwrap());
        // 15 工作日：1/5(Mon)...1/9(Fri)=5, 1/12...1/16=5, 1/19...1/23=5 → 1/23
        assert_eq!(r.deadline, "2026-01-23");
        assert_eq!(r.spec, DeadlineSpec::WorkDays(15));
    }

    // === 边界情况 ===

    #[test]
    fn test_month_overflow_jan31_plus_3_months() {
        // 1/31 + 3 月 = 4/30（不是 5/1）
        let r = compute_deadline(
            "reject_review",
            NaiveDate::from_ymd_opt(2026, 1, 31).unwrap(),
        );
        assert_eq!(r.deadline, "2026-04-30");
    }

    #[test]
    fn test_leap_year_feb29_plus_3_months() {
        // 2024 是闰年，2/29 + 3 月 = 5/29
        let r = compute_deadline(
            "reject_review",
            NaiveDate::from_ymd_opt(2024, 2, 29).unwrap(),
        );
        assert_eq!(r.deadline, "2024-05-29");
    }

    #[test]
    fn test_non_leap_year_feb28_plus_3_months() {
        // 2026 非闰年，2/28 + 3 月 = 5/28
        let r = compute_deadline(
            "reject_review",
            NaiveDate::from_ymd_opt(2026, 2, 28).unwrap(),
        );
        assert_eq!(r.deadline, "2026-05-28");
    }

    #[test]
    fn test_15_work_days_crosses_weekend() {
        // 周一发文 → 次日起 15 工作日（当日不计入，民法总则第201条）
        // 2026-01-05 是周一，1/6(Tue)=1st...1/9(Fri)=4th, 1/12=5th...1/16=9th,
        // 1/19=10th...1/23=14th, 1/26(Mon)=15th
        let r = compute_deadline("abnormal", NaiveDate::from_ymd_opt(2026, 1, 5).unwrap());
        assert_eq!(r.deadline, "2026-01-26");
    }

    #[test]
    fn test_service_date_equals_dispatch_date() {
        // 电子送达：送达日 = 发文日，不加 15 天邮路推定
        let r = compute_deadline(
            "reexamination_request",
            NaiveDate::from_ymd_opt(2026, 7, 10).unwrap(),
        );
        assert_eq!(r.service_date, r.dispatch_date);
    }

    #[test]
    fn test_is_urgent_when_leq_7_days() {
        // 用今天 + 5 天作为发文日，届满日 = +3 月，剩余 >> 7，不紧急
        // 用今天 - 2 月 + 5 天，剩余 ≈ 5 天，紧急
        let now = Local::now().date_naive();
        let dispatch = now - Duration::days(85); // ~2 月 25 天前发文
        let r = compute_deadline("reject_review", dispatch);
        // 3 个月 ≈ 90 天，85 天前发文 → 剩余 ≈ 5 天 → 紧急
        assert!(r.is_urgent || r.days_left <= 0); // 容忍边界
    }

    #[test]
    fn test_unknown_type_defaults_4_months() {
        let r = compute_deadline(
            "unknown_type",
            NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
        );
        assert_eq!(r.deadline, "2026-05-15");
        assert_eq!(r.spec, DeadlineSpec::Months(4));
    }

    #[test]
    fn test_all_seven_types_have_correct_spec() {
        assert_eq!(deadline_spec("first_exam"), DeadlineSpec::Months(4));
        assert_eq!(deadline_spec("second_rejection"), DeadlineSpec::Months(2));
        assert_eq!(deadline_spec("reject_review"), DeadlineSpec::Months(3));
        assert_eq!(
            deadline_spec("reexamination_request"),
            DeadlineSpec::Months(3)
        );
        assert_eq!(
            deadline_spec("reexamination_decision"),
            DeadlineSpec::Months(3)
        );
        assert_eq!(deadline_spec("admin_lawsuit"), DeadlineSpec::Months(3));
        assert_eq!(deadline_spec("abnormal"), DeadlineSpec::WorkDays(15));
    }
}
