//! 源级熔断冷却 / Per-source circuit-breaker cooldown（MA6a）
//!
//! 本模块落地 `FailKind::cools_down()` 的**首个生产消费点**（spec §1
//! 「Quota → 标记冷却并切换；Auth → 冷却该源并切换」；该判定自 MA1 写定后一直
//! 只有面板镜像与断言，无控制流消费，MA2b/MA4a 均如实标注归属 MA5/MA6，本包收口）。
//!
//! ## 语义（刻意选择，接入方必读）
//! - **键是 `SourceKind` 而非查询词**：冷却是源级、全查询共享的——上游限流封的是
//!   凭证/IP，不是某个关键词，换词重试只会加重封禁。
//! - **状态是进程内**（重启即清空）：不写 DB、不动 migrations、不动 `AppState` 形状。
//!   本项目是本地单用户工具，进程生命周期 ≪ 任何冷却时长，持久化只会带来
//!   「重启后带着旧封禁惩罚自己」的复杂度，没有对应收益。
//! - **单次 `cools_down()` 失败即冷却该源**，不采 spec §6 早期设想的「连续 3 次才冷却」：
//!   本仓实证同 IP 连打 3 发（<8s 间隔）即触发 Google 503 封禁（见
//!   `providers::google_patents_xhr` 模块头 EVIDENCE），第 2、3 次「确认性」打击恰好
//!   落在恶性循环最疼的位置；上游返回 Quota/Auth 时限流已经成立，阈值计数器省下的
//!   那两发请求不值这个风险。该取舍已回写规格书 §6。
//!
//! ## 三段分离（可测性设计，与 epo_ops / google_patents_xhr 的假时钟先例同构）
//! 1. [`CooldownTable`]：**纯结构**，所有涉及时间的方法显式收 `now`，测试零 sleep 零真时钟；
//! 2. [`cooldown_duration`]：`SourceKind × FailKind` 的**命名常量表**，依据写在每个常量上；
//! 3. [`global_table`] / [`note_attempts`]：**全局薄壳**（进程内共享，`OnceLock<Mutex<..>>`，
//!   不引入新依赖），只加锁转发，无任何判定逻辑。
//!
//! 生产接入点在 `routes/search.rs::api_search_online`（链前过滤 + 链后回写）。

use crate::search::model::{AttemptReport, AttemptStatus, FailKind, SourceKind};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// SerpAPI（付费源）`Quota`（402/429/配额文案）冷却时长。
/// 依据：SerpAPI 配额按套餐周期重置，429 的服务端限流窗口是分钟级——300s 内重试
/// 大概率仍是白烧一次付费调用；同时不至于长到「用户上午充了额度还要等下午」。
pub const SERPAPI_QUOTA_COOLDOWN: Duration = Duration::from_secs(300);

/// SerpAPI `Auth`（Key 无效）冷却时长。
/// 依据：Key 失效不会自愈，但用户可能中途去设置页换 Key，900s 是刻意留的短上限——
/// 「本次会话不再骚扰上游，下一轮会话前自动恢复试探」，避免配错一次 Key 就永久下线主源。
pub const SERPAPI_AUTH_COOLDOWN: Duration = Duration::from_secs(900);

/// Google Patents XHR（免费无 Key 降级源）`Quota` 冷却时长——**刻意全表最短**。
/// 依据（两端都要满足）：
/// - 必须短：它是唯一免费、无凭证、恒登记的在线源，冷却过长 ≈ 把在线检索整体下线、
///   只剩本地兜底，用户体感从「降级」变成「功能没了」。120s 在实测封禁窗口
///   （同 IP 3 发 / <8s 触发 503，见模块头）之上，足以打断「连打 → 封更久」的恶性循环。
/// - 必须长：远小于封禁自然恢复时间的重试等于主动递刀，实测 <8s 间隔连打即封，
///   1s/5s 级别的冷却没有意义。
pub const XHR_QUOTA_COOLDOWN: Duration = Duration::from_secs(120);

/// EPO OPS `Quota`（403/429 + `X-Throttling-Control`）冷却时长。
/// 依据：OPS 的免费档按月配额 + 分钟级节流；上游若带 `Retry-After`，**源内单次重试
/// 已优先采用上游值**（`providers/epo_ops.rs`），这里登记的是「重试后仍失败」的兜底窗口。
pub const EPO_QUOTA_COOLDOWN: Duration = Duration::from_secs(300);

/// EPO OPS `Auth`（token 换取 401/403 且重取无效）冷却时长。口径同 SerpAPI Auth。
pub const EPO_AUTH_COOLDOWN: Duration = Duration::from_secs(900);

/// `SourceKind × FailKind` → 冷却时长。返回 `None` = 该组合不冷却
/// （Network/Parse 一律 `None`——与 [`FailKind::cools_down`] 同判据，禁止第二套标准；
/// `LocalFts` 一律 `None`——本地兜底不进在线链，冷却它没有语义）。
///
/// XHR `Auth` 复用 [`XHR_QUOTA_COOLDOWN`]：无凭证源理论上不会返回 401/403，
/// 真出现时实质是 Google 侧封禁形态的变体（与 503 反爬同源），不构成「凭证坏了」
/// 那种需要更长静默的语义，故不另设常量。
pub fn cooldown_duration(source: SourceKind, fail: FailKind) -> Option<Duration> {
    if !fail.cools_down() {
        return None;
    }
    match (source, fail) {
        (SourceKind::SerpApi, FailKind::Quota) => Some(SERPAPI_QUOTA_COOLDOWN),
        (SourceKind::SerpApi, FailKind::Auth) => Some(SERPAPI_AUTH_COOLDOWN),
        (SourceKind::GooglePatentsXhr, FailKind::Quota | FailKind::Auth) => {
            Some(XHR_QUOTA_COOLDOWN)
        }
        (SourceKind::EpoOps, FailKind::Quota) => Some(EPO_QUOTA_COOLDOWN),
        (SourceKind::EpoOps, FailKind::Auth) => Some(EPO_AUTH_COOLDOWN),
        (SourceKind::LocalFts, _) => None,
        // cools_down 只有 Quota/Auth 两个 true 值，上面已穷举；此臂恒不可达，
        // 但不写 panic/unwrap（规约 2.7），返回 None 即「宁可不冷却也不炸链」。
        _ => None,
    }
}

/// 纯结构的冷却表：`SourceKind → 冷却截止时刻`。
///
/// derive 说明（对 AGENTS.md 2.2 的有意偏离并在注释中说明）：本结构不实现
/// `Serialize`/`Deserialize`——`Instant` 是进程单调时钟时刻，本就不可能跨进程/跨重启
/// 序列化，且「冷却状态可持久化」恰恰是本模块刻意拒绝的设计（见模块头）。
#[derive(Debug, Default, Clone)]
pub struct CooldownTable {
    until: HashMap<SourceKind, Instant>,
}

impl CooldownTable {
    pub fn new() -> Self {
        Self::default()
    }

    /// 登记一次失败。**仅当 `fail.cools_down()` 才生效**（Network/Parse 不冷却——
    /// 该判据直接调用 [`FailKind::cools_down`]，不在这里复刻第二份 match）。
    /// 时长查 [`cooldown_duration`] 常量表；表内无该组合（返回 None）同样不登记。
    pub fn record(&mut self, source: SourceKind, fail: FailKind, now: Instant) {
        if let Some(d) = cooldown_duration(source, fail) {
            self.until.insert(source, now + d);
        }
    }

    /// 该源真实跑成功一次 → 立刻清零冷却（哪怕还没到期）。
    /// 语义：成功是比任何时长都强的信号，源恢复了就没必要继续惩罚。
    pub fn success(&mut self, source: SourceKind) {
        self.until.remove(&source);
    }

    /// 剩余冷却时长。`None` = 可用（从未冷却或已到期）。到期项不做读时删除：
    /// 本表至多 4 个键（每个 SourceKind 一条），过期条目会在下次 `record` 时被覆盖，
    /// 无堆积风险，保持 `&self` 纯查询也让接线的加锁段更短。
    pub fn remaining(&self, source: SourceKind, now: Instant) -> Option<Duration> {
        let until = *self.until.get(&source)?;
        until.checked_duration_since(now).filter(|d| !d.is_zero())
    }

    /// 消费一组**真实**链上尝试回写冷却表（链后统一回写的入口，routes 侧调用）。
    /// - `Failed(f)` 且 `f.cools_down()` → 登记（走 `record`，非冷却类失败自动 no-op）；
    /// - `Success` → 清零；
    /// - `Skipped` → **不动**。没跑过的尝试不构成任何信号：既不能凭它冷却
    ///   （没出网哪来的限流），也不能凭它清零（没成功哪来的恢复）。冷却源被链前
    ///   过滤后注入的 `Skipped` 记账、以及既有的「未配 Key」`Skipped` 都落在这条上。
    pub fn note_attempts(&mut self, attempts: &[AttemptReport], now: Instant) {
        for a in attempts {
            match a.status {
                AttemptStatus::Success => self.success(a.source),
                AttemptStatus::Failed(kind) => self.record(a.source, kind, now),
                AttemptStatus::Skipped => {}
            }
        }
    }

    /// 测试/自检辅助：当前处于冷却的源数量（到期视作 0）。
    #[cfg(test)]
    fn cooled_count(&self, now: Instant) -> usize {
        self.until
            .keys()
            .filter(|k| self.remaining(**k, now).is_some())
            .count()
    }
}

/// 进程内全局冷却表。`OnceLock` 是 std 方案——`HashMap::new()` 非 const，
/// 不能 `static X: Mutex<CooldownTable> = Mutex::new(HashMap::new())` 直搭；
/// `once_cell` 不在依赖里，本包禁止新增 crate（AGENTS.md 2.2/禁止行为）。
pub fn global_table() -> &'static Mutex<CooldownTable> {
    static COOLDOWNS: OnceLock<Mutex<CooldownTable>> = OnceLock::new();
    COOLDOWNS.get_or_init(|| Mutex::new(CooldownTable::new()))
}

/// 全局薄壳：把本次链上**真实**尝试回写进程内冷却表（加锁 + 真时钟转发，零判定逻辑）。
///
/// 中毒锁用 `into_inner()` 恢复：`CooldownTable` 的全部方法都不会 panic，中毒只可能
/// 来自他处 bug，冷却表损坏的最大后果是多/少冷却一个源一个窗口期，不值得为此炸搜索。
pub fn note_attempts(attempts: &[AttemptReport]) {
    let mut guard = global_table()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    guard.note_attempts(attempts, Instant::now());
}

/// 全局薄壳：查询某源当前剩余冷却时长（真时钟）。`None` = 可用。
#[allow(dead_code)] // 生产路径经 CooldownTable::remaining 直接读表（routes 持锁批量过滤）；
                    // 本单源查询留给 MA6b 诊断面板展示「剩余 Ns」消费（届时撤掉本标注）。
pub fn remaining(source: SourceKind) -> Option<Duration> {
    let guard = global_table()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    guard.remaining(source, Instant::now())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::model::AttemptReport;

    fn base() -> Instant {
        Instant::now()
    }

    fn report(source: SourceKind, status: AttemptStatus) -> AttemptReport {
        AttemptReport {
            source,
            status,
            latency_ms: 1,
            hits: 0,
            error: None,
            hint: None,
        }
    }

    /// ① Quota/Auth 触发冷却，Network/Parse 不触发（`cools_down()` 的首个消费点断言）。
    #[test]
    fn quota_and_auth_cool_down_but_network_and_parse_do_not() {
        let t0 = base();
        for kind in [FailKind::Quota, FailKind::Auth] {
            let mut table = CooldownTable::new();
            table.record(SourceKind::SerpApi, kind, t0);
            assert!(
                table
                    .remaining(SourceKind::SerpApi, t0 + Duration::from_secs(1))
                    .is_some(),
                "{kind:?} 失败必须进入冷却（spec §1）"
            );
        }
        for kind in [FailKind::Network, FailKind::Parse] {
            let mut table = CooldownTable::new();
            table.record(SourceKind::SerpApi, kind, t0);
            assert!(
                table.remaining(SourceKind::SerpApi, t0).is_none(),
                "{kind:?} 不是限流信号，禁止冷却（否则断网一次就下线主源）"
            );
        }
    }

    /// ①-2 时长到期自动恢复可用；到期前一刻仍在冷却（假时钟，零 sleep）。
    #[test]
    fn cooldown_expires_at_the_constant_horizon() {
        let t0 = base();
        let mut table = CooldownTable::new();
        table.record(SourceKind::SerpApi, FailKind::Quota, t0);
        let just_before = t0 + SERPAPI_QUOTA_COOLDOWN - Duration::from_millis(1);
        assert!(table.remaining(SourceKind::SerpApi, just_before).is_some());
        let at_or_after = t0 + SERPAPI_QUOTA_COOLDOWN;
        assert_eq!(
            None,
            table.remaining(SourceKind::SerpApi, at_or_after),
            "到期即自动可用，冷却必须有终点（无限冷却 = 永久下线）"
        );
    }

    /// ①-3 Success 清零：哪怕冷却远未到期。
    #[test]
    fn success_clears_cooldown_immediately() {
        let t0 = base();
        let mut table = CooldownTable::new();
        table.record(SourceKind::EpoOps, FailKind::Auth, t0);
        assert!(table.remaining(SourceKind::EpoOps, t0).is_some());
        table.success(SourceKind::EpoOps);
        assert_eq!(None, table.remaining(SourceKind::EpoOps, t0));
    }

    /// ② 冷却是源级的：不同 SourceKind 互不牵连（SerpAPI 被限流不能顺手下线免费源 XHR）。
    #[test]
    fn cooldowns_are_isolated_per_source() {
        let t0 = base();
        let mut table = CooldownTable::new();
        table.record(SourceKind::SerpApi, FailKind::Quota, t0);
        table.record(SourceKind::EpoOps, FailKind::Auth, t0);
        assert_eq!(2, table.cooled_count(t0));
        assert_eq!(
            None,
            table.remaining(SourceKind::GooglePatentsXhr, t0),
            "XHR 没失败过，一颗子弹都不能算到它头上"
        );
    }

    /// ②-2 Skipped 既不清零也不登记（合成记账不构成信号）。
    #[test]
    fn skipped_attempts_touch_nothing() {
        let t0 = base();
        let mut table = CooldownTable::new();
        table.record(SourceKind::SerpApi, FailKind::Quota, t0);
        let attempts = [
            report(SourceKind::SerpApi, AttemptStatus::Skipped),
            report(SourceKind::EpoOps, AttemptStatus::Skipped),
        ];
        table.note_attempts(&attempts, t0 + Duration::from_secs(1));
        // SerpAPI 冷却仍在（Skipped 不清零）；EpoOps 不因其 Skipped 被冷却。
        assert!(table
            .remaining(SourceKind::SerpApi, t0 + Duration::from_secs(1))
            .is_some());
        assert_eq!(None, table.remaining(SourceKind::EpoOps, t0));
    }

    /// note_attempts 按真实状态分派：Failed(Network) 不冷却、Failed(Quota) 冷却、
    /// Success 清零——一条链混合三种状态各归各的。
    #[test]
    fn note_attempts_dispatches_by_status() {
        let t0 = base();
        let mut table = CooldownTable::new();
        let attempts = [
            report(SourceKind::SerpApi, AttemptStatus::Failed(FailKind::Quota)),
            report(
                SourceKind::GooglePatentsXhr,
                AttemptStatus::Failed(FailKind::Network),
            ),
            report(SourceKind::EpoOps, AttemptStatus::Success),
        ];
        table.note_attempts(&attempts, t0);
        assert!(table.remaining(SourceKind::SerpApi, t0).is_some());
        assert_eq!(None, table.remaining(SourceKind::GooglePatentsXhr, t0));
        assert_eq!(None, table.remaining(SourceKind::EpoOps, t0));
    }

    /// ⑤ 冷却时长常量表的取值锁定（改动常量必须连同注释理由一起改，别让面板/文档失配）。
    #[test]
    fn cooldown_constants_are_locked() {
        assert_eq!(300, SERPAPI_QUOTA_COOLDOWN.as_secs());
        assert_eq!(900, SERPAPI_AUTH_COOLDOWN.as_secs());
        assert_eq!(120, XHR_QUOTA_COOLDOWN.as_secs());
        assert_eq!(300, EPO_QUOTA_COOLDOWN.as_secs());
        assert_eq!(900, EPO_AUTH_COOLDOWN.as_secs());
        // XHR 必须全表最短（见常量注释：唯一免费无 Key 源，冷却过长=在线检索整体下线）。
        assert!(XHR_QUOTA_COOLDOWN < SERPAPI_QUOTA_COOLDOWN);
        assert!(XHR_QUOTA_COOLDOWN < EPO_QUOTA_COOLDOWN);
        // 查表函数与常量一一对应，且 LocalFts 永不冷却。
        assert_eq!(
            Some(SERPAPI_AUTH_COOLDOWN),
            cooldown_duration(SourceKind::SerpApi, FailKind::Auth)
        );
        assert_eq!(
            Some(XHR_QUOTA_COOLDOWN),
            cooldown_duration(SourceKind::GooglePatentsXhr, FailKind::Quota)
        );
        assert_eq!(
            None,
            cooldown_duration(SourceKind::LocalFts, FailKind::Quota)
        );
        assert_eq!(
            None,
            cooldown_duration(SourceKind::SerpApi, FailKind::Parse)
        );
    }

    /// 全局薄壳与纯结构走同一张表：note_attempts 写全局后 remaining 读得到。
    /// （刻意只测「写→读」一对动作，不在并行测试里断言其他源的表状态——全局表跨用例共享。）
    #[test]
    fn global_shell_roundtrips_through_the_shared_table() {
        let before = remaining(SourceKind::EpoOps);
        note_attempts(&[report(
            SourceKind::EpoOps,
            AttemptStatus::Failed(FailKind::Auth),
        )]);
        let after = remaining(SourceKind::EpoOps);
        assert!(
            after.is_some(),
            "全局薄壳必须真的写进共享表（否则 routes 接线是假接线）"
        );
        // 恢复现场，别把 900s 的 EPO 冷却留给并行用例。
        let mut guard = global_table().lock().unwrap_or_else(|p| p.into_inner());
        match before {
            Some(_) => {}
            None => guard.success(SourceKind::EpoOps),
        }
        assert_eq!(before, remaining(SourceKind::EpoOps));
    }
}
