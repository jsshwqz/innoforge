//! 一次性探针（**不提交**）：把核验门剩余未支持特征按"形态"归类计数，
//! 只打印布尔与长度，绝不打印正文——案件专有文字不进日志、不进聊天。
use std::fs;

fn bucket(text: &str) -> (&'static str, usize) {
    let cites_statute =
        text.contains("专利法") || text.contains("实施细则") || text.contains("审查指南");
    let declares_scope =
        text.contains("未超出") || text.contains("未引入") || text.contains("逐字源自");
    let talks_about_document =
        text.contains("权利要求书") || text.contains("说明书") || text.contains("答复书");
    let speech_act = text.contains("申请人")
        || text.contains("请求")
        || text.contains("核实")
        || text.contains("确认");
    let narrates_amendment = text.contains("本次修改")
        || text.contains("上述修改")
        || text.contains("如下修改")
        || text.contains("并入")
        || text.contains("合并");
    let ends_with_colon = text.trim_end().ends_with('：') || text.trim_end().ends_with(':');
    let has_component = text.matches(['（', '(']).count() > 0;
    let tag = if cites_statute {
        "引用法条"
    } else if declares_scope {
        "修改范围声明"
    } else if narrates_amendment && talks_about_document {
        "修改动作叙述"
    } else if speech_act && talks_about_document {
        "程序性陈述"
    } else if ends_with_colon {
        "引出标签"
    } else if has_component {
        "带标号技术句(需人工判断)"
    } else {
        "其它散文"
    };
    (tag, text.chars().count())
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let (letter, original) = (
        fs::read_to_string(&a[0]).unwrap(),
        fs::read_to_string(&a[1]).unwrap(),
    );
    let current = a.get(2).map(|p| fs::read_to_string(p).unwrap()).unwrap_or_default();
    let g = innoforge::evidence_kernel::amendment_gate::gate_amendment_text_with_current_claims(&letter, &original, &current);
    println!(
        "status={} features={} unsupported={} downgraded={} ready={} marks={} broadened={} checked={}",
        g.status.as_str(),
        g.checked_feature_count,
        g.unsupported_count,
        g.assertions_downgraded,
        g.submission_ready,
        g.gated_text.matches("【未证实").count(),
        g.broadened.len(),
        g.current_claims_checked,
    );
    let mut tally: Vec<(&'static str, usize, usize)> = Vec::new();
    for u in &g.unsupported {
        let (tag, len) = bucket(&u.text);
        if let Some(row) = tally.iter_mut().find(|r| r.0 == tag) {
            row.1 += 1;
            row.2 += len;
        } else {
            tally.push((tag, 1, len));
        }
        println!(
            "  feature claim={} len={} reason={} bucket={tag}",
            u.claim_no, len, u.reason
        );
    }
    println!("--- 形态汇总（条数 / 总字数）---");
    for (tag, n, chars) in &tally {
        println!("{tag}: {n} 条 / {chars} 字");
    }
}
