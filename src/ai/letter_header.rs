//! 答复书信头字段：从通知书表头抽取 → 回填进成文答复书。
//!
//! # 为什么单独一层
//! 答复书模板的信头原先写死 `【待填写】`／`【从 OA 中提取或标注待填写】`，实际产物里
//! 出现三种形态：模型自己从 OA 里抄对了值、模型照抄了 `【待填写】`、模型写了半截
//! （`日期：2026年【待填写】月【待填写】日`）。同一字段三种形态，交付前无法机械判断
//! "这份能不能直接交"，只能靠人逐行看。
//!
//! # 本层的分工
//! 1. [`LetterHeader::extract`]：**只用通用标签与形状**（`发文序号`、`发文日`、
//!    `申请号`/`专利号`、`发明名称`、`申请人`，以及 CN/US/EP 号形状与 `YYYY年M月D日`）
//!    从输入文本里取值。绝不认具体案件、绝不内置任何案号/人名/特征词。
//! 2. [`LetterHeader::backfill`]：对**已成文**的答复书做确定性替换。两条纪律：
//!    - 抽不到就**不编造**，统一写成 [`UNRESOLVED`]（`【未提取｜需人工补录】`），
//!      并把字段名计数返回；
//!    - 产物里**不允许**残留裸 `【待填写】`——那是模板泄漏，读起来等于"还没做"；
//!      `【未提取｜需人工补录】` 才是可追的形态：它说明系统查过、来源里没有。
//!
//! 签署日的纪律另说：答复书末尾的"日期"是申请人实际签署那天，**不由系统决定**，
//! 所以它永远走 UNRESOLVED，只有开篇"针对贵局于【日期】发出的…"里的【日期】
//! 才是通知书发文日，可以回填。

use crate::ai::case_input::is_placeholder_value;
use crate::ai::case_input::label_value;
use crate::ai::search_report::{extract_date, extract_serial};
use crate::ai::whitespace_search::strip_whitespace;

/// 抽不到时写进产物的可追占位形态（全仓唯一，计数与展示共用它）。
pub const UNRESOLVED: &str = "【未提取｜需人工补录】";

/// 已知值原样用；未知值统一成 [`UNRESOLVED`]。提示词与产物共用这一套措辞，
/// 所以"看到标记"就等于"这里需要人补"，不会和模板残留混淆。
pub fn resolved_value(value: Option<&String>) -> String {
    match value {
        Some(s) if !s.trim().is_empty() => s.trim().to_string(),
        _ => UNRESOLVED.to_string(),
    }
}

/// 通知书表头里可回填的字段。`None` 表示"来源文本没写"，与"来源写了但值是占位符"同义。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LetterHeader {
    /// 申请号（答复书信头习惯写作"专利号"）
    pub application_number: Option<String>,
    /// 发明名称 / 实用新型名称
    pub invention_title: Option<String>,
    /// 申请人 / 专利权人
    pub applicant: Option<String>,
    /// 发文序号（长数字串，用于唯一标识这一份通知书）
    pub dispatch_serial: Option<String>,
    /// 发文日，如 `2026年07月10日`
    pub dispatch_date: Option<String>,
}

/// 回填结果：新文本 + 三个可对账的计数。
#[derive(Debug, Clone, Default)]
pub struct HeaderBackfill {
    /// 回填后的答复书全文
    pub letter: String,
    /// 实际写入真实值的字段数（同一字段多处命中算多次）
    pub filled: usize,
    /// 系统未抽到值的字段名（如 `["发文序号"]`）＝抽取完成度，与产物形态无关
    pub unresolved_fields: Vec<String>,
    /// 成品里 [`UNRESOLVED`] 标记的处数＝需要人工补录的位置数，读者可在正文逐处数出
    pub unresolved_marks: usize,
}

impl LetterHeader {
    /// 从请求 JSON + 通知书原文 + 本专利文本抽取信头字段。
    ///
    /// 优先级：请求 JSON 的显式字段 > 通知书原文标签 > 本专利文本标签。
    /// 任一来源缺失都只是**不填**，不会用另一案件的残留值。
    pub fn extract(
        office_action: &str,
        my_patent_text: &str,
        request_json: Option<&serde_json::Value>,
    ) -> Self {
        let mut header = Self::default();
        if let Some(rj) = request_json {
            for scope in json_scopes(rj) {
                take(scope, &mut header);
            }
        }
        for text in [office_action, my_patent_text] {
            fill_from_labels(&mut header, text);
        }
        header
    }

    /// 已抽到的字段数（0..=5），写进 meta 让复核人一眼看到信头完成度。
    pub fn known_count(&self) -> usize {
        [
            &self.application_number,
            &self.invention_title,
            &self.applicant,
            &self.dispatch_serial,
            &self.dispatch_date,
        ]
        .iter()
        .filter(|v| v.as_ref().map(|s| !s.trim().is_empty()).unwrap_or(false))
        .count()
    }

    /// 注入提示词的「已知案件信息」块。抽不到的字段直接给出 [`UNRESOLVED`]，
    /// 并明确禁止模型写 `【待填写】`——模板里留"待填写"，模型就照抄"待填写"。
    pub fn prompt_block(&self) -> String {
        format!(
            "## 已知案件信息（系统已从通知书表头/上传文本按标签抽取）\n\
             专利号：{app_no}\n\
             发明名称：{title}\n\
             申请人：{applicant}\n\
             发文序号：{serial}\n\
             发文日：{date}\n\n\
             信头纪律：以上五项**必须原样使用**，不得改写、不得自行推断别的号码或日期。\n\
             显示为{unresolved}的字段表示系统没在来源文本里找到，保留该标记即可；\n\
             禁止输出【待填写】或任何同义空位。答复书末尾的签署日期由申请人决定，\n\
             一律写{unresolved}，不要填今天。\n",
            app_no = resolved_value(self.application_number.as_ref()),
            title = resolved_value(self.invention_title.as_ref()),
            applicant = resolved_value(self.applicant.as_ref()),
            serial = resolved_value(self.dispatch_serial.as_ref()),
            date = resolved_value(self.dispatch_date.as_ref()),
            unresolved = UNRESOLVED,
        )
    }

    /// 对成文答复书做确定性回填。见模块头两条纪律。
    pub fn backfill(&self, letter: &str) -> HeaderBackfill {
        let values: [(&'static str, Option<&str>); 5] = [
            ("专利号", self.application_number.as_deref()),
            ("发明名称", self.invention_title.as_deref()),
            ("申请人", self.applicant.as_deref()),
            ("发文序号", self.dispatch_serial.as_deref()),
            ("发文日", self.dispatch_date.as_deref()),
        ];
        // 只报"系统抽不到值的字段"，与后面是否插入/替换无关：抽取完成度和产物形态
        // 是两件事，混在一个数里就看不出"抽到了却没写进正文"这种缺陷。
        let unresolved_fields: Vec<String> = values
            .iter()
            .filter(|(_, v)| v.map(|s| s.trim().is_empty()).unwrap_or(true))
            .map(|(k, _)| (*k).to_string())
            .collect();

        // ① 信头标签行：值为空 / 含【…】/ 是占位词 → 用抽取值覆盖；真实值一律不动。
        let mut filled = 0usize;
        let mut lines: Vec<String> = letter.split('\n').map(|s| s.to_string()).collect();
        let mut seen_keys: Vec<&'static str> = Vec::new();
        for line in lines.iter_mut() {
            let Some((key, start, end)) = header_line_value_range(line) else {
                continue;
            };
            if !seen_keys.contains(&key) {
                seen_keys.push(key);
            }
            let old = line[start..end].trim().to_string();
            let needs_fill = old.is_empty()
                || old.contains('【')
                || is_placeholder_value(&old)
                || old == UNRESOLVED;
            if !needs_fill {
                continue;
            }
            let new = values
                .iter()
                .find(|(k, _)| *k == key)
                .and_then(|(_, v)| *v)
                .map(str::trim)
                .filter(|s| !s.is_empty());
            match new {
                Some(v) => {
                    line.replace_range(start..end, v);
                    filled += 1;
                }
                None => {
                    // 抽不到：无论原来是什么空位形态，统一换成可追形态。
                    // 半截日期（`2026年【…】月【…】日`）由第③步整串收敛，这里只管本行值。
                    if old != UNRESOLVED {
                        line.replace_range(start..end, UNRESOLVED);
                    }
                }
            }
        }

        // ② 抽到但答复书里根本没有该标签行：补插到已出现的信头块之后（找不到块就不插，
        //    宁缺勿把发文序号塞进正文某处）。
        let insert_after = lines
            .iter()
            .enumerate()
            .filter_map(|(idx, line)| header_line_value_range(line).map(|_| idx))
            .next_back();
        let mut extra: Vec<String> = Vec::new();
        for (key, value) in values {
            let Some(v) = value.map(str::trim).filter(|s| !s.is_empty()) else {
                continue;
            };
            if seen_keys.contains(&key) {
                continue;
            }
            extra.push(format!("{key}：{v}"));
            seen_keys.push(key);
        }
        // 插在哪：已有信头块就接在块尾；整封书里一行信头标签都没有，就接在首个非空行
        // （标题）之后。找不到任何非空行（空文本）就不插——**计数只记真正插进去的行**，
        // 先加计数再判断能不能插，就是"展示数说填了 5 处、产物里一个都没有"的假绿灯。
        let insert_at = insert_after.map(|i| i + 1).or_else(|| {
            lines
                .iter()
                .position(|l| !l.trim().is_empty())
                .map(|i| i + 1)
        });
        if let Some(at) = insert_at {
            for (n, line) in extra.into_iter().enumerate() {
                lines.insert(at + n, line);
                filled += 1;
            }
        }
        let mut body = lines.join("\n");

        // ③ 行内兜底：开篇"针对贵局于【日期】发出的…"里的【日期】就是发文日，可回填；
        //    半截日期整体收敛——留下 `2026年` 会被读者当成系统确认过的年份；
        //    其余任何含"待填写"的括号空位都是模板泄漏，一律换成可追形态。
        if let Some(date) = self
            .dispatch_date
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            body = body.replace("【日期】", date);
        }
        body = partial_date_re()
            .replace_all(&body, UNRESOLVED)
            .into_owned();
        body = placeholder_re().replace_all(&body, UNRESOLVED).into_owned();
        // 计数口径：`unresolved_marks` = **成品里**可追占位标记的处数。读者可以在正文里
        // 逐处数出来，数出来必须等于这个数——不数"改了几次"，因为同一行可能改两次、
        // 也可能一次改动覆盖了两个空位，改动次数与产物形态对不上（errors.md ㊶④）。
        let unresolved_marks = body.matches(UNRESOLVED).count();
        debug_assert_eq!(
            placeholder_slots(&body),
            0,
            "回填后仍残留待填写空位：契约被破坏"
        );

        HeaderBackfill {
            letter: body,
            filled,
            unresolved_fields,
            unresolved_marks,
        }
    }
}

/// 信头标签表：每行只认这些**通用**标签，值从冒号（或表格竖线）之后取。
///
/// 同组内**长标签在前**：`发文日` 是 `发文日期` 的前缀，短标签排前面会让长标签永远轮不到。
const HEADER_LABELS: &[(&str, &[&str])] = &[
    ("专利号", &["专利申请号", "申请案号", "专利号", "申请号"]),
    ("发明名称", &["发明创造名称", "实用新型名称", "发明名称"]),
    ("申请人", &["专利权人", "申请人"]),
    ("发文序号", &["发文序号"]),
    ("发文日", &["发文日期", "发文日"]),
];

/// 判断一行是否为信头标签行，返回 `(字段, 值的字节区间)`。
///
/// 只认"行首装饰符 + 标签 + （可选装饰符）+ 分隔符 `：`/`:`/`|`"的形态，因此正文里
/// "申请人认为…"这类句子不会被当成信头行——标签后必须紧跟分隔符。
/// 已知边界：若两个字段写在同一行（`申请号：X 发明名称：Y`），只认第一个标签；
/// 但第二个字段的值不含占位符，回填层的"值非空即不动"纪律保证它不会被覆盖。
fn header_line_value_range(line: &str) -> Option<(&'static str, usize, usize)> {
    let line = line.trim_end_matches(['\r']);
    let rest = line.trim_start_matches(is_line_decor);
    let base = line.len() - rest.len();
    for &(key, labels) in HEADER_LABELS {
        for &label in labels {
            let Some(after) = rest.strip_prefix(label) else {
                continue;
            };
            // 跳过标签与分隔符之间的装饰符；遇到的第一个非装饰字符必须是分隔符
            let mut idx = 0usize;
            let mut separator = None;
            while let Some(ch) = after[idx..].chars().next() {
                match ch {
                    c if is_value_decor(c) => idx += c.len_utf8(),
                    '：' | ':' | '|' => {
                        separator = Some(ch);
                        idx += ch.len_utf8();
                        break;
                    }
                    _ => break,
                }
            }
            if separator.is_none() {
                // 该标签后面不是分隔符 ⇒ 这一行不是这个字段的标签行，
                // 换下一个标签继续试（"发文日期"要先于"发文日"命中，否则会被短标签吃掉）
                continue;
            };
            while let Some(ch) = after[idx..].chars().next() {
                if is_value_decor(ch) {
                    idx += ch.len_utf8();
                } else {
                    break;
                }
            }
            let value_start = base + label.len() + idx;
            let tail = &line[value_start..];
            // 表格形态（`| 专利号 | CN… |`）值止于下一个竖线；普通行取到行尾
            let value_end = match tail.find('|') {
                Some(p) => value_start + p,
                None => line.len(),
            };
            debug_assert!(value_start <= value_end && value_end <= line.len());
            return Some((key, value_start, value_end));
        }
    }
    None
}

/// 行首允许剥掉的版式装饰（markdown 强调、标题、列表、表格分隔、BOM、全角空格）。
fn is_line_decor(ch: char) -> bool {
    matches!(
        ch,
        ' ' | '\t' | '*' | '#' | '>' | '|' | '-' | '•' | '\u{feff}' | '\u{3000}'
    )
}

/// 标签与分隔符之间、以及分隔符之后允许跳过的装饰（不含 `|`，它是分隔符本身）。
fn is_value_decor(ch: char) -> bool {
    matches!(ch, ' ' | '\t' | '*' | '\u{feff}' | '\u{3000}')
}

/// 半截日期（`2026年【待填写】月【待填写】日`）在无法回填时收敛成可追形态，
/// 而不是留下三处独立空位——读者会把"2026年"当成系统确认过的年份。
/// 三种有洞的写法都收敛；其它更冷僻的形态由通用空位规则兜底（仍保证零残留）。
fn partial_date_re() -> &'static regex::Regex {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        regex::Regex::new(
            r"\d{4}年(?:【[^】]*】月【[^】]*】日|【[^】]*】月\d{1,2}日|\d{1,2}月【[^】]*】日)",
        )
        .expect("letter_header: 半截日期正则")
    })
}

/// 任何含"待填写"的括号空位——不只是模板那一种字面量，模型会写成
/// `【从 OA 中提取或标注待填写】`，同一种缺陷不能因为措辞不同而漏网。
fn placeholder_re() -> &'static regex::Regex {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        regex::Regex::new(r"【[^】]{0,60}待填写[^】]{0,60}】").expect("letter_header: 空位正则")
    })
}

/// 产物里还剩多少个"待填写"空位。公开给递交前检查复用：
/// 任何一份被判定 `submission_ready` 的答复书，这个数必须是 0。
pub fn placeholder_slots(text: &str) -> usize {
    placeholder_re().find_iter(text).count()
}

/// 请求 JSON 里可能携带显式字段的作用域，按优先级排列。
/// 顶层只在它本身是对象时用——非对象请求体不许 panic，只是少一个来源。
fn json_scopes(rj: &serde_json::Value) -> Vec<&serde_json::Map<String, serde_json::Value>> {
    let mut scopes = Vec::new();
    for key in ["case_meta", "my_patent"] {
        if let Some(obj) = rj.get(key).and_then(|v| v.as_object()) {
            scopes.push(obj);
        }
    }
    if let Some(obj) = rj.as_object() {
        scopes.push(obj);
    }
    scopes
}

/// 从一个 JSON 作用域取显式字段（已抽到的不覆盖）。
fn take(scope: &serde_json::Map<String, serde_json::Value>, header: &mut LetterHeader) {
    let get = |keys: &[&str]| -> Option<String> {
        keys.iter()
            .filter_map(|k| scope.get(*k).and_then(|v| v.as_str()))
            .map(str::trim)
            .find(|s| !s.is_empty() && !is_placeholder_value(s))
            .map(|s| s.to_string())
    };
    if header.application_number.is_none() {
        header.application_number = get(&["application_number", "patent_number", "app_no"]);
    }
    if header.invention_title.is_none() {
        header.invention_title = get(&["invention_title", "title"]);
    }
    if header.applicant.is_none() {
        header.applicant = get(&["applicant"]);
    }
    if header.dispatch_serial.is_none() {
        header.dispatch_serial = get(&["dispatch_serial", "filing_serial", "serial"]);
    }
    if header.dispatch_date.is_none() {
        header.dispatch_date = get(&["dispatch_date", "oa_date", "filing_date"]);
    }
}

/// 从自由文本按标签补抽取（请求 JSON 未给全时用）。
fn fill_from_labels(header: &mut LetterHeader, text: &str) {
    if text.trim().is_empty() {
        return;
    }
    let head: String = text.chars().take(6000).collect();
    if header.application_number.is_none() {
        header.application_number = label_value(&head, &["申请号", "专利号", "申请案号"]);
    }
    if header.invention_title.is_none() {
        header.invention_title = label_value(&head, &["发明名称", "发明创造名称", "实用新型名称"]);
    }
    if header.applicant.is_none() {
        header.applicant = label_value(&head, &["申请人", "专利权人"]);
    }
    let stripped = strip_whitespace(text);
    if header.dispatch_serial.is_none() {
        header.dispatch_serial = extract_serial(&stripped);
    }
    if header.dispatch_date.is_none() {
        header.dispatch_date = date_after_marker(&stripped);
    }
}

/// 只认"发文日"标记之后的日期，避免把正文里第一个日期（示例/公开日）当成发文日。
fn date_after_marker(stripped: &str) -> Option<String> {
    for marker in ["发文日期", "发文日"] {
        for (pos, _) in stripped.match_indices(marker) {
            let window: String = stripped[pos + marker.len()..].chars().take(40).collect();
            if let Some(date) = extract_date(&window) {
                return Some(date);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 全部为合成夹具：9999 段申请号、"示例申请人"、"一种示例装置"，无真实案件信息。
    const SYNTHETIC_NOTICE: &str = "\
国家知识产权局
发文日：2026年07月10日
发文序号：9999010100000001
申请号：999999999999.9
发明名称：一种示例装置
申请人：示例申请人
正文其余内容。";

    fn synthetic_header() -> LetterHeader {
        LetterHeader::extract(SYNTHETIC_NOTICE, "", None)
    }

    #[test]
    fn extract_reads_all_five_header_fields() {
        let h = synthetic_header();
        assert_eq!(h.application_number.as_deref(), Some("999999999999.9"));
        assert_eq!(h.invention_title.as_deref(), Some("一种示例装置"));
        assert_eq!(h.applicant.as_deref(), Some("示例申请人"));
        assert_eq!(h.dispatch_serial.as_deref(), Some("9999010100000001"));
        assert_eq!(h.dispatch_date.as_deref(), Some("2026年07月10日"));
        assert_eq!(h.known_count(), 5);
    }

    #[test]
    fn extract_does_not_grab_the_first_date_it_sees() {
        // 负控：正文里出现的第一个日期不是发文日 ⇒ 发文日必须抽不到，而不是被当成发文日
        let text = "公开号 CN990000001A 公开日：2026年01月01日\n申请号：999999999999.9\n";
        let h = LetterHeader::extract(text, "", None);
        assert_eq!(h.dispatch_date, None, "无发文日标记时不得用任意日期顶替");
        assert_eq!(h.dispatch_serial, None);
        assert_eq!(h.application_number.as_deref(), Some("999999999999.9"));
    }

    #[test]
    fn extract_prefers_explicit_request_fields_over_text() {
        let req = serde_json::json!({
            "case_meta": {
                "application_number": "9999007654321.0",
                "dispatch_serial": "9999077700000007",
                "dispatch_date": "2026年09月09日"
            }
        });
        let h = LetterHeader::extract(SYNTHETIC_NOTICE, "", Some(&req));
        assert_eq!(h.application_number.as_deref(), Some("9999007654321.0"));
        assert_eq!(h.dispatch_serial.as_deref(), Some("9999077700000007"));
        assert_eq!(h.dispatch_date.as_deref(), Some("2026年09月09日"));
    }

    #[test]
    fn backfill_fills_placeholder_header_lines_and_leaves_no_bare_token() {
        let h = synthetic_header();
        let letter = "\
=== 意见陈述书 ===

**专利号**：【待填写】
**发明名称**：【从 OA 中提取或标注待填写】
申请人：【待填写】

此致

申请人：【待填写】
日期：【待填写】";
        let out = h.backfill(letter);
        assert!(
            out.letter.contains("**专利号**：999999999999.9"),
            "{}",
            out.letter
        );
        assert!(out.letter.contains("**发明名称**：一种示例装置"));
        assert!(out.letter.contains("发文序号：9999010100000001"));
        assert!(out.letter.contains("发文日：2026年07月10日"));
        assert!(!out.letter.contains("待填写"), "产物不得残留裸模板占位");
        assert_eq!(
            out.letter.matches(UNRESOLVED).count(),
            1,
            "只剩签署日一处空位：{}",
            out.letter
        );
        assert_eq!(
            out.filled, 6,
            "专利号/发明名称/申请人×2 四处真值 + 补插发文序号/发文日两行"
        );
        assert_eq!(
            out.unresolved_marks, 1,
            "签署日期由申请人决定 ⇒ 只留一处可追标记"
        );
        assert!(out.letter.contains(&format!("日期：{UNRESOLVED}")));
        assert!(
            out.unresolved_fields.is_empty(),
            "五项全抽到 ⇒ 不该再报未提取：{:?}",
            out.unresolved_fields
        );
    }

    #[test]
    fn backfill_never_overwrites_a_value_the_letter_already_has() {
        let h = synthetic_header();
        let letter = "**专利号**：8888001234567.8\n发明名称：一种示例装置\n";
        let out = h.backfill(letter);
        assert!(
            out.letter.contains("**专利号**：8888001234567.8"),
            "已有真实值时不得覆盖——回填只补空位"
        );
        assert_eq!(out.filled, 3, "只剩补插的申请人/发文序号/发文日三行");
    }

    #[test]
    fn backfill_collapses_half_filled_date_instead_of_leaving_three_holes() {
        // 真实形态：模型自己写了年份，月日是占位（v6 实测）
        let h = LetterHeader::extract("申请号：999999999999.9", "", None);
        let letter = "日期：2026年【待填写】月【待填写】日\n";
        let out = h.backfill(letter);
        assert!(!out.letter.contains("待填写"));
        assert!(
            !out.letter.contains("2026年"),
            "半截日期要整体收敛成可追形态，否则读者会把 2026年 当成系统确认过的值：{}",
            out.letter
        );
        assert_eq!(
            out.unresolved_marks, 1,
            "两个空位收敛成一处标记——产物里需要人补的位置就是一个"
        );
        assert_eq!(out.unresolved_fields.len(), 4, "除专利号外四项都没抽到");
    }

    #[test]
    fn backfill_fills_inline_dispatch_date_in_opening_sentence() {
        let h = synthetic_header();
        let letter = "针对贵局于【日期】发出的第【通知书编号】号审查意见通知书。\n";
        let out = h.backfill(letter);
        assert!(out.letter.contains("针对贵局于2026年07月10日发出的"));
        assert!(
            out.letter.contains("第【通知书编号】号"),
            "通知书编号≠发文序号，不许拿发文序号顶替"
        );
    }

    #[test]
    fn backfill_recognises_markdown_table_header_row() {
        let h = synthetic_header();
        let letter = "| **专利号** | 【待填写】 |\n| 发明名称 | 【待填写】 |\n";
        let out = h.backfill(letter);
        // 值区间取到下一个竖线为止，因此竖线前的那个空格被一起替换掉了；
        // markdown 仍能渲染，形态在这里是确定的，所以把它钉死而不是写成"包含即可"。
        assert!(
            out.letter.contains("| **专利号** | 999999999999.9|"),
            "{}",
            out.letter
        );
        assert!(
            out.letter.contains("| 发明名称 | 一种示例装置|"),
            "{}",
            out.letter
        );
        assert!(!out.letter.contains("待填写"));
    }

    #[test]
    fn prose_lines_are_not_header_lines() {
        for line in [
            "申请人认为本申请具备创造性。",
            "申请号一栏以通知书记载为准。",
            "专利权人已核对全文。",
            "**发文日** 与 **发文序号** 见首页表头",
        ] {
            assert!(
                header_line_value_range(line).is_none(),
                "标签后没有分隔符 ⇒ {line} 不是信头行"
            );
        }
    }

    #[test]
    fn header_block_absent_falls_back_to_just_after_the_title() {
        let h = synthetic_header();
        let letter = "复审请求书\n\n申请人认为本申请具备创造性。\n";
        let out = h.backfill(letter);
        assert_eq!(out.filled, 5, "五项都补进去了");
        assert!(
            out.letter.starts_with(
                "复审请求书\n专利号：999999999999.9\n发明名称：一种示例装置\n\
                 申请人：示例申请人\n发文序号：9999010100000001\n发文日：2026年07月10日\n"
            ),
            "{}",
            out.letter
        );
        assert!(
            out.letter.contains("申请人认为本申请具备创造性。"),
            "正文句子原样保留"
        );
    }

    #[test]
    fn backfill_is_idempotent() {
        let h = synthetic_header();
        let letter = "**专利号**：【待填写】\n正文。\n日期：【待填写】\n";
        let once = h.backfill(letter);
        let twice = h.backfill(&once.letter);
        assert_eq!(once.letter, twice.letter);
        assert_eq!(twice.filled, 0, "第二次不应再改任何一行");
    }

    #[test]
    fn extraction_failure_is_reported_not_silently_passed() {
        let h = LetterHeader::extract("这是一份没有任何表头标签的文本", "", None);
        assert_eq!(h.known_count(), 0);
        let letter = "**专利号**：【待填写】\n发明名称：【待填写】\n申请人：【待填写】\n";
        let out = h.backfill(letter);
        assert_eq!(
            out.unresolved_fields.len(),
            5,
            "五项都要报出来：{:?}",
            out.unresolved_fields
        );
        assert_eq!(out.filled, 0);
        assert_eq!(out.unresolved_marks, 3);
        assert_eq!(
            out.letter.matches(UNRESOLVED).count(),
            out.unresolved_marks,
            "meta 说的数必须能在正文里逐处数出来"
        );
        assert_eq!(placeholder_slots(&out.letter), 0, "零残留待填写空位");
    }

    #[test]
    fn prompt_block_states_known_values_and_forbids_bare_placeholder() {
        let h = LetterHeader::extract("申请号：999999999999.9\n发文日：2026年07月10日\n", "", None);
        let block = h.prompt_block();
        assert!(block.contains("专利号：999999999999.9"));
        assert!(block.contains("发文日：2026年07月10日"));
        assert!(block.contains(UNRESOLVED), "抽不到的字段要给出可追形态");
        assert!(block.contains("禁止输出【待填写】"));
    }
}
