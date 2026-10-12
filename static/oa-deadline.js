/**
 * OA 答复期限计算 —— 前端单一入口 / Single frontend entry
 *
 * 两个模板（office_action_response.html / index.html）都引用本文件，
 * 不再各自手写三元式。
 *
 * 期限表从后端 /api/oa/deadline-table 拉取（Rust 侧 src/oa_deadline.rs 是唯一数据源），
 * 拉取失败时用本地 fallback（与 Rust 侧同值，仅作离线降级）。
 *
 * 电子送达按《专利法实施细则》第四条第三款：送达日 = 发文日，不加 15 天邮路推定。
 */

// 本地 fallback 期限表（与 src/oa_deadline.rs 同值，仅 API 不可达时降级用）
var OA_DEADLINE_TABLE_FALLBACK = {
    'first_exam':             { kind: 'Months',   value: 4 },
    'second_rejection':       { kind: 'Months',   value: 2 },
    'reject_review':          { kind: 'Months',   value: 3 },
    'reexamination_request':  { kind: 'Months',   value: 3 },
    'reexamination_decision': { kind: 'Months',   value: 3 },
    'admin_lawsuit':          { kind: 'Months',   value: 3 },
    'abnormal':               { kind: 'WorkDays', value: 15 }
};

var _oaDeadlineTable = null;
var _oaDeadlineTableLoaded = false;

/**
 * 异步加载期限表（从后端 API），加载后缓存到 sessionStorage。
 * 页面加载时自动调用；computeOADeadline 在表未加载时用 fallback。
 */
function _loadOaDeadlineTable() {
    if (_oaDeadlineTableLoaded) return;
    // 先尝试 sessionStorage 缓存
    try {
        var cached = sessionStorage.getItem('oa_deadline_table');
        if (cached) {
            _oaDeadlineTable = JSON.parse(cached);
            _oaDeadlineTableLoaded = true;
            return;
        }
    } catch(e) { /* ignore */ }

    // 异步拉取（不阻塞页面）
    fetch('/api/oa/deadline-table').then(function(resp) {
        return resp.json();
    }).then(function(data) {
        if (data && data.deadline_table) {
            _oaDeadlineTable = data.deadline_table;
            _oaDeadlineTableLoaded = true;
            try {
                sessionStorage.setItem('oa_deadline_table', JSON.stringify(_oaDeadlineTable));
            } catch(e) { /* ignore */ }
        }
    }).catch(function() {
        // API 不可达，用 fallback（不设 _oaDeadlineTableLoaded=true，下次重试）
        console.warn('[oa-deadline] API unreachable, using fallback table');
    });
}

/**
 * 获取期限 spec（优先用 API 拉取的表，否则 fallback）
 */
function _getDeadlineSpec(oaType) {
    var table = _oaDeadlineTable || OA_DEADLINE_TABLE_FALLBACK;
    return table[oaType] || { kind: 'Months', value: 4 }; // 未知类型保守 4 个月
}

/**
 * 计算工作日后的日期（跳过周六周日）
 */
function _addWorkDays(startDate, days) {
    var d = new Date(startDate);
    var added = 0;
    while (added < days) {
        d.setDate(d.getDate() + 1);
        var dow = d.getDay(); // 0=Sun, 6=Sat
        if (dow !== 0 && dow !== 6) {
            added++;
        }
    }
    return d;
}

/**
 * 给日期加自然月（处理月末溢出，如 1/31 + 1 月 = 2/28）
 */
function _addMonths(startDate, months) {
    var d = new Date(startDate);
    var targetMonth = d.getMonth() + months;
    var targetYear = d.getFullYear() + Math.floor(targetMonth / 12);
    targetMonth = ((targetMonth % 12) + 12) % 12;
    var day = d.getDate();
    // 月末溢出处理
    var lastDay = new Date(targetYear, targetMonth + 1, 0).getDate();
    var actualDay = Math.min(day, lastDay);
    return new Date(targetYear, targetMonth, actualDay);
}

/**
 * 计算期限 —— 两个模板都调这个函数
 *
 * @param {string} oaType - OA 类型（first_exam/second_rejection/reject_review/reexamination_request/...）
 * @param {string} dateStr - 发文日，ISO 格式 YYYY-MM-DD
 * @returns {{dispatchDate:string, serviceDate:string, deadline:Date, deadlineStr:string, daysLeft:number, totalDays:number, isUrgent:boolean, spec:Object}}
 */
function computeOADeadline(oaType, dateStr) {
    var dispatchDate = new Date(dateStr + 'T00:00:00');
    var spec = _getDeadlineSpec(oaType);

    var deadline;
    if (spec.kind === 'WorkDays') {
        deadline = _addWorkDays(dispatchDate, spec.value);
    } else {
        deadline = _addMonths(dispatchDate, spec.value);
    }

    var now = new Date();
    var daysLeft = Math.ceil((deadline - now) / (1000 * 60 * 60 * 24));
    var totalDays = Math.ceil((deadline - dispatchDate) / (1000 * 60 * 60 * 24));
    var isUrgent = daysLeft <= 7;

    function fmt(d) {
        var y = d.getFullYear();
        var m = String(d.getMonth() + 1).padStart(2, '0');
        var day = String(d.getDate()).padStart(2, '0');
        return y + '-' + m + '-' + day;
    }

    return {
        dispatchDate: dateStr,
        serviceDate: dateStr,  // 电子送达：送达日 = 发文日
        deadline: deadline,
        deadlineStr: fmt(deadline),
        daysLeft: daysLeft,
        totalDays: totalDays,
        isUrgent: isUrgent,
        spec: spec
    };
}

/**
 * 格式化四元组显示文本：发文日 / 送达日 / 届满日 / 剩余天数
 */
function formatDeadlineDisplay(dl) {
    var dispatchStr = dl.dispatchDate;
    var serviceStr = dl.serviceDate;
    var deadlineDisplay = dl.deadline.toLocaleDateString('zh-CN');
    var text = '发文日 ' + dispatchStr + ' · 送达日 ' + serviceStr + ' · 届满日 ' + deadlineDisplay;
    if (dl.daysLeft > 0) {
        text += '（剩余 ' + dl.daysLeft + ' 天）';
    } else if (dl.daysLeft === 0) {
        text += '（今天截止！）';
    } else {
        text += '（已逾期 ' + Math.abs(dl.daysLeft) + ' 天）';
    }
    if (dl.isUrgent && dl.daysLeft >= 0) {
        text += ' ⚠️ 临近法定期限';
    }
    return text;
}

// 页面加载时自动拉取期限表
if (typeof window !== 'undefined') {
    _loadOaDeadlineTable();
}
