//! 案件数据守卫：遍历 git ls-files 得到的每个文件，命中以下检查即 panic：
//!
//! 四类检查，每类同时扫文件**内容**和文件**路径**：
//! 1. 申请人姓名——汉字与拼音两种形态（拼音用字符数组逐字符拼装）
//! 2. 对比文件公开号类目——本轮替掉的四个真实 D 号（每段 ≤4 字符拼接）
//! 3. 发文序号类目——16 位数字（每段 ≤4 字符拼接）
//! 4. 申请号形状 `\d{13}(\.\d)?`、公开号形状 `CN\d{8}[A-Z]` / `CN\d{9}U`
//!    ——仅在 src/ 生代码区域（#[cfg(test)] 之前）检查
//!
//! tests/fixtures/ 下的文件内容也执行四类检查（fixture 按定义是合成内容，更应被扫描）。
//! 白名单常量必须保持为空。
//! 守卫自身不含任何 >=8 位连续数字串，也不含完整禁词字面量。
//! 违规消息不回显命中串，只报文件名和类别。

use regex::Regex;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::process::Command;

// ── 类目 1：申请人姓名（汉字 + 拼音）──

/// 申请人姓名汉字形态：用字符数组逐字符拼接构造。
fn forbidden_name_cn() -> String {
    let chars: [char; 3] = ['\u{738b}', '\u{9752}', '\u{829d}'];
    chars.iter().collect()
}

/// 申请人姓名拼音形态：用字符数组逐字符拼接构造。
/// 守卫文件自身不含此拼音字面量。
fn forbidden_name_pinyin() -> String {
    let chars: [char; 11] = ['w', 'a', 'n', 'g', 'q', 'i', 'n', 'g', 'z', 'h', 'i'];
    chars.iter().collect()
}

// ── 类目 2：对比文件公开号（每段 ≤4 字符，运行时拼接）──

fn forbidden_pub_numbers() -> Vec<String> {
    // D1 → ["CN","1031","3314","4A"]
    let d1 = format!("{}{}{}{}", "CN", "1031", "3314", "4A");
    // D2 → ["CN","1131","0768","4A"]
    let d2 = format!("{}{}{}{}", "CN", "1131", "0768", "4A");
    // D3 → ["CN","2109","5132","0U"]
    let d3 = format!("{}{}{}{}", "CN", "2109", "5132", "0U");
    // D4 → ["CN","2183","2814","1U"]
    let d4 = format!("{}{}{}{}", "CN", "2183", "2814", "1U");
    // 旧案公开号A → ["CN","1150","7668","8A"]
    let p1 = format!("{}{}{}{}", "CN", "1150", "7668", "8A");
    // 旧案公开号B → ["CN","1149","6275","7U"]
    let p2 = format!("{}{}{}{}", "CN", "1149", "6275", "7U");
    vec![d1, d2, d3, d4, p1, p2]
}

// ── 类目 3：发文序号（16 位，每段 ≤4 字符拼接）──

fn forbidden_filing_serials() -> Vec<String> {
    // 驳回决定发文序号 → ["2026","0710","0014","1920"]
    let s1 = format!("{}{}{}{}", "2026", "0710", "0014", "1920");
    // 一通发文序号 → ["2026","0301","0002","8380"]
    let s2 = format!("{}{}{}{}", "2026", "0301", "0002", "8380");
    vec![s1, s2]
}

// ── 类目 4：被禁申请号（每段 ≤4 字符拼接）──

fn forbidden_app_numbers() -> Vec<String> {
    // 案号A → ["2022","1083","5143","9"] 四段拼接
    let a1 = format!("{}{}{}{}", "2022", "1083", "5143", "9");
    let a1d = format!("{}{}.{}", "2022", "1083", "5143.9");
    // 案号B → ["2022","1047","7324","9"] 四段拼接
    let a2 = format!("{}{}{}{}", "2022", "1047", "7324", "9");
    let a2d = format!("{}{}.{}", "2022", "1047", "7324.9");
    vec![a1, a1d, a2, a2d]
}

/// 白名单：必须保持为空。不许为了让测试通过而往白名单加条目。
const WHITELIST: &[&str] = &[];

/// 对于 Rust 文件，截取 #[cfg(test)] 之前的部分作为生代码区域。
/// 如果文件没有 #[cfg(test)]，返回全文。
fn production_part(content: &str, is_rust: bool) -> &str {
    if !is_rust {
        return content;
    }
    if let Some(pos) = content.find("#[cfg(test)]") {
        &content[..pos]
    } else {
        content
    }
}

/// 对单个文本（内容或路径）执行全部四类检查，返回违规类别列表。
/// 违规消息不回显命中串本身。
///
/// 大小写敏感性：
/// - 申请人姓名汉字：中文无大小写概念，直接匹配。
/// - 拼音／公开号／序号／申请号：大小写不敏感（每个文件只 to_lowercase 一次）。
///   防止小写公开号 / 混合大小写拼音等形态漏网。
fn check_text(text: &str, is_src_rust_prod: bool) -> Vec<&'static str> {
    let mut hits: Vec<&'static str> = Vec::new();

    // 类目 1：申请人姓名（汉字 — 中文无大小写，直接匹配）
    if text.contains(&forbidden_name_cn()) {
        hits.push("申请人姓名(汉字)");
    }

    // 大小写不敏感类目：对 text 做一次 to_lowercase，needle 也 lowercase
    let text_lower = text.to_lowercase();

    // 类目 1 续：申请人姓名拼音（大小写不敏感，防混合大小写拼音漏网）
    if text_lower.contains(&forbidden_name_pinyin().to_lowercase()) {
        hits.push("申请人姓名(拼音)");
    }

    // 类目 2：对比文件公开号（大小写不敏感，防小写公开号漏网）
    for n in forbidden_pub_numbers() {
        if text_lower.contains(&n.to_lowercase()) {
            hits.push("对比文件公开号");
            break;
        }
    }

    // 类目 3：发文序号（纯数字，大小写不敏感无影响，走同一路径保持一致）
    for n in forbidden_filing_serials() {
        if text_lower.contains(&n) {
            hits.push("发文序号");
            break;
        }
    }

    // 类目 4：被禁申请号（纯数字，同上）
    for n in forbidden_app_numbers() {
        if text_lower.contains(&n) {
            hits.push("被禁申请号");
            break;
        }
    }

    // 类目 4 续：形状检查（仅 src/ 生代码区域，大小写不敏感）
    if is_src_rust_prod {
        let re_app_num = Regex::new(r"\d{13}(\.\d)?").expect("invalid app num regex");
        let re_pub_num_8 = Regex::new(r"(?i)CN\d{8}[A-Z]").expect("invalid pub num 8 regex");
        let re_pub_num_9u = Regex::new(r"(?i)CN\d{9}U").expect("invalid pub num 9u regex");

        if re_app_num.is_match(text) {
            hits.push("生代码申请号形状");
        }
        if re_pub_num_8.is_match(text) {
            hits.push("生代码公开号形状(CN8+letter)");
        }
        if re_pub_num_9u.is_match(text) {
            hits.push("生代码公开号形状(CN9+U)");
        }
    }

    hits
}

/// 收集待扫文件：优先 `git ls-files`（跟踪树权威清单）。
/// 若仓库不是 git 工作树（例如从 Release/源码包解压、或 git 不可用），
/// 回退为递归遍历清单目录，跳过构建产物与本地数据目录。
/// 回退是**扫描范围的扩大而非放弃**——守卫绝不因为拿不到 git 就静默通过。
fn collect_files() -> (Vec<String>, &'static str) {
    if let Ok(output) = Command::new("git").arg("ls-files").output() {
        if output.status.success() {
            let files = std::str::from_utf8(&output.stdout)
                .expect("git ls-files output not UTF-8")
                .lines()
                .filter(|l| !l.is_empty())
                .map(|l| l.to_string())
                .collect();
            return (files, "git ls-files");
        }
    }

    // 回退：递归文件树（不带 .git 的源码包检出）
    const SKIP_DIRS: &[&str] = &[".git", "target", "node_modules", "data", ".cargo"];
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut files: Vec<String> = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if path.is_dir() {
                if !SKIP_DIRS.contains(&name.as_str()) {
                    stack.push(path);
                }
            } else if let Ok(rel) = path.strip_prefix(&root) {
                files.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    files.sort();
    (files, "filesystem walk (no git metadata)")
}

#[test]
fn no_case_data_in_git_tracked_files() {
    let (files, mode) = collect_files();
    assert!(
        !files.is_empty(),
        "守卫未取到任何待扫文件（mode={}）——空扫描不允许算通过",
        mode
    );

    let mut scanned: usize = 0;
    let mut violations: Vec<String> = Vec::new();

    for file in &files {
        let file = file.as_str();
        // 跳过白名单
        if WHITELIST.contains(&file) {
            continue;
        }

        // ── 路径扫描：对所有文件路径执行四类检查 ──
        // （tests/fixtures/ 路径也扫，因为泄露可能藏在路径里的姓名/案号）
        let path_hits = check_text(file, false);
        for h in path_hits {
            violations.push(format!("{}: 路径命中 [{}]", file, h));
        }

        // ── 内容扫描 ──
        // tests/fixtures/ 内容也扫描：fixture 按定义是合成内容，
        // 若往 fixture 里塞真实案卷，守卫必须报警。不留静默口子。

        let content = match std::fs::read_to_string(file) {
            Ok(c) => c,
            Err(_) => continue,
        };

        scanned += 1;

        let is_src_rust = file.starts_with("src/") && file.ends_with(".rs");
        let prod = production_part(&content, is_src_rust);
        let content_hits = check_text(prod, is_src_rust);
        for h in content_hits {
            violations.push(format!("{}: 内容命中 [{}]", file, h));
        }
    }

    eprintln!(
        "案件数据守卫：扫描方式={}，{} 个文件内容 + {} 个文件路径",
        mode,
        scanned,
        files.len()
    );

    if !violations.is_empty() {
        panic!(
            "案件数据守卫发现 {} 处违规：\n{}",
            violations.len(),
            violations.join("\n")
        );
    }
}

// ── 回归测试：大小写不敏感匹配 ──

#[test]
fn case_insensitive_pub_number_lowercase_is_caught() {
    // 合成字符串：小写 cn + D1 号码形态（不使用真实 D1 号码字面量）
    let d1_lower = format!("{}{}{}{}", "cn", "1031", "3314", "4a");
    let sample = format!("文件名 d1_{}.pdf", d1_lower);
    let hits = check_text(&sample, false);
    assert!(
        hits.contains(&"对比文件公开号"),
        "lowercase pub number must be caught, got hits: {:?}",
        hits
    );
}

#[test]
fn case_insensitive_pinyin_mixed_case_is_caught() {
    // 混合大小写拼音（首字母大写形态）
    let pinyin_mixed: String = {
        let chars: [char; 11] = ['W', 'a', 'n', 'g', 'Q', 'i', 'n', 'g', 'Z', 'h', 'i'];
        chars.iter().collect()
    };
    let sample = format!("作者：{}", pinyin_mixed);
    let hits = check_text(&sample, false);
    assert!(
        hits.contains(&"申请人姓名(拼音)"),
        "mixed-case pinyin must be caught, got hits: {:?}",
        hits
    );
}

#[test]
fn case_insensitive_pub_number_uppercase_still_caught() {
    // 大写形态仍应被命中（回归：不能因为加了不敏感而漏掉原来的大写）
    let d1_upper = format!("{}{}{}{}", "CN", "1031", "3314", "4A");
    let sample = format!("对比文件 {}", d1_upper);
    let hits = check_text(&sample, false);
    assert!(
        hits.contains(&"对比文件公开号"),
        "uppercase pub number must still be caught, got hits: {:?}",
        hits
    );
}

// ═══════════════════════════════════════════════════════════════════════
//  长句守卫：SHA256 摘要比对
//  把案件原文的 12 字连续串只以 sha256 摘要清单的形式存入本文件。
//  清单不含原文，被擦也还原不出原文（SHA256 不可逆）。
//  测试对仓内文本滑窗取 12 字串算哈希比对，命中即 fail。
// ═══════════════════════════════════════════════════════════════════════

/// 案件原文 12 字连续串的 SHA256 摘要清单。
/// 共 494 条，覆盖 10 段案件原文的所有 12 字滑窗子串。
/// 本清单不含任何原文，只有哈希摘要。
const CASE_TEXT_12CHAR_SHA256_DIGESTS: &[&str] = &[
    "00485501227f010942ff5f4ec59421b1c19f542d52414c8b511e247e8fc6a132",
    "0049e3e938189bdaf812b5278a57ac917a582ad6ab9c63ef89852a8470bf750c",
    "0131b6ca4898dae12b482b01a90439fa5b3d7a8fadf1aa70ad33d25d5134f130",
    "01b5e8590408d5e50c9b877ec0e134e68e612e34aef580bcec610801bc1646a0",
    "0285e9632ee039227c02f829182d4ce0bcf71c56a5f5a767daaab99f94eaf3f4",
    "028a480f294056fa157fc906e27c8f67f14ebedb1ed44038d64c81d8d581d595",
    "02dd53176dd366bff0f054030f61258e057360429a12818f4f44ef3e50185b36",
    "03213699e2b6e0b31964b5188746e681c59afee959a8104b2268d0649347f4fd",
    "03a22c35bf4ae93a4511973e733270f9ab2fa83ce835b335b775ca2928ae3f50",
    "03aad1bf6b65d37c916a8e88b306e79c2d779062368a6cc4794e56476a2ea897",
    "03de26e33237c14c20dd95b927dcda8ba96f96bd33ef666556131d259190a4ac",
    "0494ed394df7901bd9c9ea63bd72a06875323fcbdf0726b012b9442ffe152019",
    "063aa1149669f4bf3eecb4415a1a9bca483ff82304f2d0193910bc138b3ad735",
    "070ed60a82d6a28f6ede077c205494041895e604d6164098752195bf9e7e0171",
    "07986ddddac3bedbdc75bc9e8b5738eec16777945b8621fe26064f2f2d179764",
    "079fcc35e1a7439cd5d98e1249f659ac765c09b154951018925636d714ed6fe8",
    "07aa0bec6288967caaacf1e7cbe1d568954256509c2d458179f49c02dd4402dc",
    "07c6f79fd7bbc3c8a0af62c97d90810a748f92aaa53e78f54a3790eb26f87cef",
    "08b7e5cc26c6f34672e7fc4beeabd3f73582f7af5fc85965244da8a85806da87",
    "08bac9c4a6b5797ca2057e3adc4903f440e305257071b91397406826aa94d5e0",
    "08d83b0876a873759a9c0d552530c039e088b0392427eabfd504293652d9811e",
    "0918d0318a268fc12b4959fefec584cf43f1a354a8752537bd89d852fbf5bc58",
    "092cb4b0a5fa02225af98aa355534272507cf120a35056e2a36bf8a3e819062a",
    "0af399d5d95b9b2e3956069fecce24616687f2310ecddf4757a430ede3ddd107",
    "0afbe1fb9349715e7b6bc94b450226a29d4da25961f0061eed303ac97ca5056e",
    "0b6f7bc3d608dbde2501f1fb943bda0f912e460760c48e1a32648b35468bfeeb",
    "0cac3584af0ec76d3994d10c3a003adb9df147fb9a21db213b6e81b55e33077d",
    "0ce9df9abc7e9d9398d5132d3147d99c2abdb1749baa9acb474ed0fb023e3c2b",
    "0d05917720ce9088a31fe49fae529a0d0ad0ee8d9acc5373156f65fb9876fc9d",
    "0d8a2dc0a4e8c2830de44e78a63dc755114dfa7054aa2536f324c903d3aaaeb4",
    "0ece854fd7dcc252519eb68b427188d1c5c79d441fe26984d5a53db116a8d604",
    "0f720bd9842501483df3674cf6f0b5081c2a8ff3bd13aa6b01baf32e747c098e",
    "0faa6d7dd0b4f610ef57f87ab5807cd3cf99395dcd2d022ddf4f3039b8640a7b",
    "0fcc70a34a20e74b457e87f2892a21399f2e159510935375e823bba0d103e4da",
    "107a43c26b642b973d7fd2903f9f7a6cc31f3af08c120eef8b149a656637a0b1",
    "116e9ecbdf3ad95c4295047e581eadb1e6786543d493f0fce9a744c69a85bcec",
    "11c748778df1b735ada08e033b16080e84db30443ab6529fccf3be2f0978a1fe",
    "1294b24f8f1c31a96e28ae4b1806f563aa1e23861cacc781d0442b4875ee5391",
    "129e9010e0dd2d81722dc54031ddfc6d75ba9761e619bbefc86ffbe16f5c07b3",
    "13b350aeef3278ed6917997ac07621c0e9534b6e21757c3248e505cc20f0ba02",
    "13c1791a7c940c54d30007289c95f6240162ceba35aa1e4574fef43fd9898fcd",
    "1433e5dac3b942b02257b46e164c322f2c488aac8b51e0301c39ed3177d618e6",
    "146ddf3420dd12f55b97997f6343f4657b764799dc1730de1f169609445f5267",
    "14d50c478e587e290421875ef82652a5a1c95a46edb426ff11a065ca94a59d17",
    "151d0c6ffab7310c6c8965d0edbecb687ef6d982933cba340447dd89c2e05ea6",
    "157ca4b638f378df0e3c4ecadb19b9b52ca87e63209f62a5e21ebf3d66c3c262",
    "162ad8e8316ed1ecb0e59dc7e116131c0904714896307ad21f24c126134169c5",
    "17297e1b381e5c42710e51e095492acb2e4d0b9855881dcf08d21950b5c22b71",
    "177ee17965e63fbd6db24500cd2b8c38ce62c4b498ee1258a9d1df74ba755874",
    "17caf503243964ff25ff40010179a2c7546e0b5c36a95c7fdbb4c55f2e454a5b",
    "17f06db0e5ba4d23ac81cde805f487797ca88179c5bee7a82e9a8ccc8b630f0c",
    "190d645fc38389a00273e53be6d4851c9fc929f7982095e79c7d67a1ef8a4bd0",
    "195380df8fe6a07f18c54bc8892e57de06c9225972895309ec330cc8f35d2240",
    "1979c6c13a8178613240a33a87cd2b4a0dff855e350136419915d2a003f87c68",
    "19b8d2d9619ce0bb4e1fbbad436436804c6468a5ca3a98aecc6efd54c210491e",
    "19f47daa11de2bdf941d1fb5e476047a7ac7ccdc9bad38da3eb0b806d85fd65f",
    "1c400508732093a97e26f035dfb4c54d07b3c9a258aa8b58df22373959d62ebc",
    "1d04bd0bba51af53322ef46d57d81a56e089074f252f168fc9e8f029eaebd9dc",
    "1d5bfaee7a7604085f25443cefd4d222a9067df2e15a49fd51979f95e0b81e89",
    "1dd6a51be9bbc4eb046f97e1d77d450aceeeab8a82b7eef0e13c78b1a324d48a",
    "1df1e41785ea85c516d46e154c899cc8835ba0636c74baa69d69e447d678a049",
    "202927dadf9b97faa83c015345d9ad2dcdd774f779d7b068f7a8f64a6786b549",
    "204e78fb447dc1a0ddf4714e1ea7e264b1a5c1c15376fae105ceab24ec600035",
    "20b0be6535a8067efff835c6d6b1320faacf8e7f1e03420594fe1849cecf71c3",
    "22039f2290a2f84c2b0f14b2d392726cc854d60b272ed73581c0c4e3af453213",
    "220f0540767193c4653c53d4567a444f33c05e0d2e24df716f95ebdc4658c121",
    "228ee945df208e3cfee4dcef95ed35107c6c34618025f8be57189ad2f94ec13a",
    "23077b580103bf98ab174c9d1860e008f54b4949821fd217cf3f1e368dae35a0",
    "24200713721bb79cf28fabaad00cb4079f6f8b8cde509ee831652a3bf81eabb2",
    "244d18d50899ad989f519fa4d1ef0317be2449c7ab9bc5900e4dfd8ae2a67064",
    "246cee986b4bbd48864fe34b3996f4da030bc5c508c9088e2153c7cece0b14d9",
    "24f0db44edb218df75cbd2865c5beabbd3edd4df0ac313c98436bbae5704ba65",
    "26b0512bf49e07fd91bfdfae23a02c9e82e3448df109e7ed81c1732acc687dc6",
    "2744cd2d5d0937e2c079ed1d72715e412f6cf6245a9e82a99f2eb9d56f888936",
    "2776c9068ee032cddbfdd42e0d82e92a4165f52bf78bddf493e33947c4809e1b",
    "28d11cb99671c77a0961101ef99e244f0d1fbb8afa4ad3972d352e94c8189bab",
    "2a0d18677b05bc65166873ebbdaf4c9c607dc6f7f37a0bf33b0400e0c1718c29",
    "2ab29992af61c3c2574e5589a5dd5002cdaeb9e92b2fe3a7772d6e6c3faab56d",
    "2ab356292b4189f0eec3a24a0a8d5ceea43652d17703efdd0d3553648f98665e",
    "2b0e42b61cc19ae467587343ecbd88df169ac0a9c64b5685c2007debfa880194",
    "2bd791dba7de1ea7e25b2840f268b5d2c842801c46ae3b1246642319c1c4c964",
    "2bff0a5697e778bc94507174b88c1ab0c9087367dedbe31c8ba8f01b641f3302",
    "2c29a7e59507a71ada9f337356fdb431f48e4b588b92d10ed8c04e415d87187d",
    "2d5d1ee1c47300c7a90e3dfb1b290c812e6fdabb0847d2cbaab6105fd530ec1f",
    "2de15bfe2bb87496ff07f686be250fc29c3d6a4fd6ccda55af21d627c87913aa",
    "2e3a1823f6a33c8db738faf57a72a37a2d8b94954fbb619088796d23d5822f9b",
    "2e907086c8553696956ba3c1a1ac806fe3d79aebad7a2e7304be6d157cf55710",
    "2f7f936f9124733bffd1809616277d499deafd14f5b8cfd240645394c6a2bb9c",
    "2fb44afc5d2038e52142ef3eb2f33f9698a15f6423451f158df0be2426fd369c",
    "31e134e43bff35c866d31816f9cca012096ba063ae7916cd383256877fe78ece",
    "3212e724970ff42c6fbcad4837b25562b6b6a527e09170df6a4bd1171049f564",
    "326d900c725efab0c12083cf0ca3905cae01d08da584544394cb3766309d79d7",
    "33f568ad784bfbff3f150add564f5f68e9532c2afb0fdfd7a883b005e18e45e8",
    "341073a6ba00ad9060233ac83565b5086913082e3219d4b91cb5a9965311176a",
    "3485bfa0a8e1e79c79add29a81d0ba77fec7c02ce9fbaf3ad854b668912450fc",
    "366425ba01dcd49b00e517edddbc27654b5fa916d0085c23622640063c30e43c",
    "36da7887fecb54a665a2c10ac72eaf0995289351aa55c4c165e5d05a4c12b106",
    "37306a73a3ce406f25f6a84f6f2048a41b39c6f1de39adf64c709182eefbd90b",
    "374b79a8c00dd2cc9ec0085e6412fbfc8bff828204913b908dc5d21e80e9e76e",
    "380f6397085286f282403cce639460a5d9b60f4efd39ab51bda96662a3fc2f72",
    "38d6ef9ff947a34e6b9408acb65e92c701cc388aa8dc3cd22dddf343483927b2",
    "39f786ebff2130f2b44b9cc18a58d2be63f926315847a9a553922d98089223d1",
    "3aba41913ebe277a10508d9e77a9b8a17f5751151292b507c0bfbc52abd37c77",
    "3b06749a2001ce883af9268d7eafb148c1a314b29267a27b62df18262c54ffe8",
    "3b28d69cb9a7779c658234054f6f466316ce6e9c0db88b5f8a99343e864166c1",
    "3b2f0f30c2cf7d641d83e169400d690178f5ed11c18bd7d4f552ebe88a00de8a",
    "3b3fb60afd01c6e41b95f35fbb84ad7b7311657a617c48bd1fefcf0030c9cd78",
    "3c4b6ce85ba2dcb904b54edd2e5f9bf04e4f70f4f1fd599e646d6a1a8db494e7",
    "3c99520c78dde2dc8020f652278af735d5e87fda5a404a67440ed040f9a34cfb",
    "3dd029233a0a9d6c12a1231b3f38610b2401ba3eb20ebf4fcbd6539f71aa6a57",
    "3e1bcf4fa57e29599aa90f8f1f4815f1bf8445a1dc60b4046a22d4004af3658d",
    "3e707110bbf7f7825a4c6d874052dce65c8d0c94d7b5459f38f73c381b12e7a8",
    "3f690a997297868a938256d2b88fce48f659146424207e5c8e0461b922a1e7e5",
    "3f93cc3867c4f9b63f74ca0438557c0443ece1f8ca728aaaff0b993947fed79e",
    "40ebce7650567da4704d532fa14e6e8a6d98d9d266ae6ace7f7af5c317edff24",
    "4102a38fa60db207173910f8f3a59a929e4efc7a9c30926cd6fead2ccd7c5a95",
    "41329e78645c6725898267f3ab91a9f96b5e0591c9533335d23354b0ba7df974",
    "41b1dc338aace9d409b07d06af1f5e1faabcf817b47d52b8a4a3d29c5a98ce36",
    "42051383ba71124beef0d9ebda25ed16789e2ee34dd9214c4f92d6e235bfdb21",
    "424d31deabf39bd91a2c6b922152686637b64c9890f83efcd60fe324297bad3e",
    "42f2a1aa96a5d9a824822fde81f0e7c9db767e948e5206ca64ce0a4ef839ef8e",
    "43741f24cde65a87eef02fee724ab51c09817e532bf9e113dbdcdefc6f0b2b39",
    "43f146b80f3ca4260e652d50edf157ce67233d69c46f543089b296ec2d8996e5",
    "4404e6b691b503a83c1578cb87fc2e137647be1978a9892238a6ae280544053f",
    "44291a9e2eabb14d998d882337c5717f7ca0c7f1798b2d273d0ba26cc3b10a0d",
    "44fb8da664af40f0b3ffea1e94ea15516612e9bda81f1267e2185ce9d5aae2df",
    "472426b624ebe7ad575f15f519e8409dfce9212a527c0f17478b1fc6933375d5",
    "47b016c85fbd06aa21ba456671561152357ebae6f34105a83a43759e3b79c673",
    "4878c3ebe6dbcd3bdb1013553fd38ae13018a0a47db68f199792d3cf2d079d71",
    "48895b8b21086fa570102e6c7476da882809ef62ad88c87f74a4c6af9269afc8",
    "48bf8659c36fa269d2c62c60b5b2ab65bbf754ec05debaaba111b68d309ec733",
    "48fc7da0f63143cc31434b9075ec6a1b46b3b1ca7aca0f5cb8d665f754d36912",
    "49273fbc5f8d7fb1ad6ef30f9d469f5e316834f4252cfb865a1adf5be0c5c57e",
    "49fd99af1174ec6b3f230da1afb81351dd297d57028f908476789ba4e6eb5462",
    "4a85f967dc762219ac774289fb0978ec3feced5a9d074eebe8475d36e933f3a0",
    "4ab13e48a2673a67368730b088a1971758ce36825a668f505913ff06ae070b49",
    "4affd45dea3b68ebfd89d286e8f3c30f66c96375475fccde03d538c1a02cedf6",
    "4b058f8b29719df4d6230fc613403ebec671a2f69a626ad8cee43ded28158df5",
    "4b0d1982a63f85d62f632328088ccdc5152ecb9f3b33d5c26f15f74b1618e64e",
    "4d42b889751a2df1e9953352f4aa766a40fae792fa7c1ae728b19a5689239f89",
    "4e063241b01fabb8b28df2899f78b19e395cd59a1dfe21966dbd52b7eb3da53a",
    "4e7d59fa538fa1ab88193b83ec0c333fb34ee83957baaa34c7cbbcba86ec8a2e",
    "4f7378cdcd1a5c8b1af06ab3e6b51f118e3ae8af9990c6513b05cda767dd2e76",
    "500e682ff05d2dd676611f53bcb58df27f2670fe736d543fff1e7bf25513c794",
    "509d9b5307f78d0bf0045307677022fa59c9ae5216c81caf34d1ee723de65973",
    "511da2cab43603f0d7d1ac7cbae704d340b143168965b3fcdc229fa69f742793",
    "5144d27bc0364160f814283e775efec65c488d2296bf022025443e5a1aa5a62d",
    "516671ddc2bfd4a988cbf53fa61df6589c37d2de681afebb5628b2abacb3a146",
    "5292098a812333aa3596f05d5f3657bf33dce35b65e9211fc869fd372b2b1fa4",
    "530e9bbc1e230becc74d7c358486f47839ab185bef71d98e6d2462cee0000049",
    "53406479697de48850266081fa132870f61f782572b5a6d9257442d1c4099aea",
    "53c52761647ca4285e87c39536a0cbfebde2f5ea39c814e9b3d6a966a232abd6",
    "53e3ec55d9ab666c6799e4107ab6015781b9d17fed19381004a5ea8f8eb3b2b1",
    "544e17026e9e80e9849974d7e46900d8c7788b1bde1dfb801767475f3ad19857",
    "560098f65df88a49ee9bbc1cea88ae81b51545fe6ed57509c565869b55eb4cb1",
    "567080a24c5bde83539294e26e17bc71285fb257e2780b2b0f4ba60ce80793db",
    "56a74888f5ac9dd9a686c584ab1a16dd5d711bb1a2feff431bbcc55c4f039e86",
    "56eeff29e60ff601eb4aba92f629a9875bf7e4764071979f8f07b1dee54bec53",
    "5813a277cf76356d39f189b2c4a7a943ae8a4838a054bbd7f61d76105cc59b67",
    "5824ccf736a1a5b7dc1ecd6e342e1595a09857cc82d8c4f1524fd3b8193729b9",
    "597900671709fcebec9c622e921e4a6e4e6b93207588fb50016497ab912e5d88",
    "5985bc67b93acbc0e842e6f9da5584127dafce6f9c255769ca978a4ca07eb085",
    "59c1d72bcbe2b6912c54a0b9447714c319bf4563e3888b9bfac36a6451b9a853",
    "59d9b24596f7f6f5be546930e8f0a5a46e16e145c16fab06b6fe6c7fedf6d143",
    "5adbef7cafe3ce99be47cdd4ab622d7a7d4a23115bdb1169cb3f9e361aa0b5f4",
    "5af606218b2cce7f1614510f925a1082ad659b45aac69ad8f9f3009d57f475d5",
    "5bdfebe6fb125d1d43d4ef2e4adc8cb8c4ec8fd5171274c8f81f2e14c7ee6f72",
    "5cc2e35a338b7e9b1e8647e0baba49d024b50f844abc9e60251481d379277037",
    "5e51641571bc00472dad8b4129e014e3dd1b6f9dc3adc6f012502f62dc45f2e5",
    "5efceaa7c775e0cdeaedb0a5c488015bc5068a2f358ef3ae433ab16c5e787691",
    "5fb5811de5c6733c4ec010b9cf8b1a9bbb80da861822633dafcfbe09d9cda035",
    "5ff5ae94ca676a096da53b6d739af4bf40d7d10e9bed392680380d0595e010dd",
    "611eff66a28957789b3e6ad557f335d0919588a79f05086720128b17059f0603",
    "613a633ca1183093b2f9622addce8ee141cab39e96704f3fc653a06f2b8c1bc6",
    "61651be6c796765ce23a0d38ca3f9f925db2f7ab1daef3f7e3571deb73d8d5ad",
    "6186f78a7c1b7f8ff64b7ff1dd5ce4ff72321681afd20886715acffb05e834c6",
    "61f49df9171dfb9615561a8ae23aef8493b4a9b6b875cd2492d885288be5217b",
    "61fc162f32a3c33d1a2db54fbf9e374b55161a65d3da8857b97b9e2d9b348937",
    "623aa6992d2a216fc6a7723fa58ec46c4a53b97b90eb25e54cfeea27f0f19eef",
    "623dad0a168508c711d3c4da2659bbd215a98d5f4898ead3c3cde9ed4360cf29",
    "62a1005f6fb8ce31c52f4e58a4bcccbe5b0511ca0b317cb2bb1d0dcc2efaa901",
    "62ce07cb2759c576cb629f9d20f950abc72872449814e3bb7ea13651628380ba",
    "62cfd498c419df1ae2448915f3a929e2bfa8242939487599113784fbd41a6a49",
    "63e9c4f147de7b53ea9fa45a1d7da8245ec39869aeb8acc099f56209edcc842e",
    "643257998bd32a3a1d3fda6052bc6b04c1661919a32f3e07350205d43badaa1c",
    "6458199f9de46afed161ee5920bc8afd9c549932afd83fc37228afd3e3620b00",
    "64b13c287fe8a271bd5a75f24376bd851d26e6a7e4cc40a87bccf346ddfb523a",
    "64dd3564914f99fc55a693a719f88321ceaf9aa26e7b9629dc8d16b0ac8c91dd",
    "659edc5e9627b69c55be4c9202d280e320cc1aef070226868e641463903beb87",
    "65ecc13ab02101f5114eb5a91e7d934cde80843cfda06f6517fdb56683a91bf1",
    "6649d9ae0d3aa2396fc4d931619a67a9eab96c993e98895b59a34a40757b3443",
    "66af3df62e8b93e0072b478222103b85c2cd24a381b5fd457a9c044e3ffd48e9",
    "676510e93f4c96936e1a0b3075f7435b8e18b4b25040e29d54fe7d7553a72ae6",
    "68473ddcb9074598b726c16f4b07c83264d5577822b290daa0687708f4c45707",
    "69c8840d2390085159805c5963618d637b0ab65052b4fdb501845afdd34a06ce",
    "6a0c428c45958eebddb72d5101365b40d06cee8db76bcf1ce22de8ab7b79e30f",
    "6a327081f8733d00a2b9783468bcbac73fe245d965b4e13304f11bb090dd2bc2",
    "6a3f7af2fcc382272d690a9aca3a632f1180d7e2cf7b605e05ee7c2d0bbccc23",
    "6a7951bbd70ccad2adcff2bbe7c18884f25b3bc521290a90cac75b8045b2df44",
    "6a99b2d2299a52af1d31f614f121918b3a823af456e33a9fd5c3e609383087a7",
    "6ab183ae910fb5120c9d8d1af00054f62086967a7398f4587306dc44d773cd3b",
    "6bb32b35df791058910be6398f7d01c152622b8d94c486989f80da9ad5dc9291",
    "6c2ac44f4897574f6e6a17c7837dffcce1d59fc214b263c2c6d4e62932e1a899",
    "6c882cde1d3b9933deff7ed4a32f39f55d9cb592f51a9fe3e959209a56baff5d",
    "6cd51ab56849603467f8eb927db44517d365cfef18d3f6a7a9d8a832f6fb2d73",
    "6d31358630b8fc35948bd5dfa71005f2bded3661d5c9682396c2723310aa4b42",
    "6ddf84a89f4b8df24be4d9f0351380632925a13fceeb56313c20d68770d6955c",
    "6def08f8729886ce33864e7799ad62e3161ddb092e1d04126af15bf97b212fe4",
    "6e22193e283b96ec0e366c1301cf4bfc3be6b84505188aac32755615acdc6682",
    "6e3d07062b4a72e5c854a386b80e22399bd70c4adf0bf728630e99d6bf905539",
    "6e5a2d74bb09c9ad74f8896c7c6aed92f4304afcdfdce07ae0f470f3187f38ec",
    "6edec1549315a7119c48a162b1ad7a033aa224cbf36f5c9f2f37d06d8f7fa671",
    "6f2919cce97074c4193d5719bc8cdeb5606cccab0dfea1c2932290cb987ffbab",
    "70278aed96d8c553a8f7039515636737ef872f840adcff6c6aea10fba6ab6d52",
    "709ef177ba75eec0a76164c0c8bf2c22ac2252ad299ab67b9ae9acdd5c2bb7fb",
    "70aedf6de1794257c6eb704c9c681b6c9a7875931a4f079f9dcd6cd401554167",
    "70dfa846bf9fc29304ab82dfbea5899bed9093a42587cd97f802def3fec5d5b3",
    "71f988e0febf58c7c5019582cee14eaad8efc259c335f9e7f102fcb589332fe2",
    "728863a272ae2760f8500bbfbbc390c749a6edf25309ff79967671acad676373",
    "7331ff3b924ad96afbefbbebce14c9db39062ba43b2f5328a5d3da5e91ad1973",
    "73d2cd6625c08f68112bc87dd341e8c85ae4789e53755f243a2fa637af09a9ee",
    "741f137a5516318a5cb097c97e59ac19f849b94dc6cf3630a3659ffb83085250",
    "7420636cbf285b7492fd8d19065962e0a80fe2b1fa96a6bdaf569ce264dc92b7",
    "742f95b175a914d117df56522595ae273dea49e31b65c38a29b3ec689e220364",
    "743a465623a15f9d62ddc10be310b5d6078c5b4aa010dcb6fd031846db9d7991",
    "75239c021fd3894f5f615eea49645af23eb6f796d55a1c46341cd9a915f9122b",
    "75e1d3f854863d41d6e2847afc64917eecbad3bbdbc0ab843a79362dff6366e8",
    "7885c1d525f01724cde843d62dc4855fefb34085ba9234cd4a7ab71425288283",
    "78e7d8d189155c70180eb87668b26b769ef492507ac20669f8228d421a564624",
    "79ac125b34855a3fdd7a37485a462750d67fc2dca36149ee22d771e481ceac68",
    "7b2034a2b80c49d3f96d7ffb94840b90bf419d637a475738fbda0952741b4467",
    "7c4968d3f52fcdae2a1ee9d87abd6832ceba93d9b65a196f10096c19232325e6",
    "7c5970f18d40cb0302f71d18a3da9e0487b4c8f1679ca299eca0dededf75ef01",
    "7cf49921109fbf4a7a561a5280eead1c9fc6190307a57e542094818a247fd211",
    "7d783f317d3884ed8b1ba8cc898c9322effd3d6b6726bcb108561130b67e432c",
    "7da62ca0f272814eeb5ca108990fa7b3471b12a0f3c403a492855246673d6089",
    "7eceee2d9b6793fdcf2b3a9da0d0bb227d809d3fd9893f98b3b5c87ddf427dab",
    "7ed73bce80da37ba1099cb7747a3330cc5cb1672b7c0ca14636dbb537b153e28",
    "7f006e76a523640f34ae50000c40c9828695cd4021c1c58b83c6e9b79aecd878",
    "7f4fb9e4f1f3a150d712baf462adbedaf9df6f0fcb844906c3f17d644e84b23d",
    "7fb68291f4c61cc3a8d0ed6c2af174eb224001f65edb7fe6aa58c4a692ed3557",
    "80cf285f778ee62eb879a7acbe7f8a6c22001f5a1d589ad5adbc1d5d438e6884",
    "8205fa2dc767b5146ff6b22db548674efa4a37d2168e4c75adb1e14d46470306",
    "82c0410ec5c4707e0306ba166488dc8ac5925a0d43bccd066d3a4d238ad0e963",
    "82e7f94d2f2e280f6effcfdf7f61f41fa8142812eb8ecb60042034becd4c589b",
    "82f3f211fe21d01d2c63cdb4f7810e0f444cd386155db34dcb8afeddedd46a7d",
    "8327d8159fb5984b7aa6cd233f02e80ad8a3f8ce3642d665512a31b7384708af",
    "8452c9671544d5d622989495e3ebb906e0d7b63a4858fdaea071517a29e30dd0",
    "846b97533820be56cc9bc89bc10881d6058cce5bbcab7d51d6f746d971cb5c5f",
    "85f8754c8fbf2a7a6af2f6250a0101310d670936c2052fae36f7f0b0d493ae85",
    "85fab2c17590ba10a97286a455ec02cb18eba1d589c832a8e9a883f53aa33a83",
    "865be4577a518e60a7a9685b67673ca8bd5a2f11709a5e77e4d0a69c0a17d439",
    "869fa9d472cab25c54b5cdd1a7edccd8af525d4d4439d3524e5de217a34569fb",
    "870a1ae567aae563ed7588d32a8792e62405979822dc055de925c3d275456b80",
    "8781a66641a248b2b6cd6ea7033c66f5fb374b2dfa83fc9d3576691e17c4ddbf",
    "87e56baf77fbc9182641d66946e724fe5af0568d8f0e85492e93447a1c01e220",
    "89b653d09341929fe6aceed78faa272fcf7f4d5499565f5a2574acc435a086e8",
    "8b01cfaf377e65aa50e639ae3c07c9d05f304eb3e280c58920102c16ac6e1571",
    "8b3e8c546515efec7ea73bd7822a21537ccd7f6300ae46fc8e320d9cae8591b3",
    "8d06f6a8a0981ae7d361740862a7b334f219e597fdc7e78454e4f11073da2c86",
    "8d7a20d425c858a0764928632b05093a88971383716bf4f976747777e66f00ea",
    "8dac303dfd3945d5bcb398f6dcdc6241a93859ebdc9c832b8740f489a7644fde",
    "8dc3414604b9887df202093e47efeb83cecab172bed32eb887732083f0988ade",
    "8dd4d3a75125b06f82b70e4ab226aaac8be7e7b30f7b35b248cc324cf7684255",
    "8e944314de48ea96d91142d46c63b723671657d5aab2aec335b80450ab857455",
    "8eda27f5c8709d21279d589c1372ded98fce8a9dec3607905a9198b173119495",
    "8f8d8549b917597926b91d9d93c71b3212673d8a350634abeed08f12e6086544",
    "90206d259ab636a0f5520c9012d7bef58440e7ac4a81192bbfff5e0a861c2c55",
    "90830c81067b7acf1eee6644c00b1b96aad39a6798960358ab1a5138ee9d2d52",
    "908ef398da9450b55f786ece47ca5750ee25f85dab8ec20cc8dd3ec75a71c466",
    "9289cd3660f8e3b1e5bfe410858eecec94a0812fe59fa89af0520b4c891451b2",
    "94469dab0e655e8e60d5ff5de295d766110f05717ea7f413c3cf60f2ea935b48",
    "9458256dc1115d3c7dadb3a995a557bf21eb95afacd83160ef7ebd8e631322d2",
    "94ab6a17c0f7586da8d9f11cc033f97a11a783f9abb96d9796bae9e9bf139167",
    "94ff678637fab5c24f146d641f1775f45bb813417b4d473bce71317f187ab658",
    "956c687509a59e4b642927b95afbc32e6ee225a27afac57b4f47009d5190a5e2",
    "95c3f2e6640d7dc327b5065c936c0f68941d0d34e65960189934f3c17c33bb8e",
    "95d821adc8c1619a0f267a91de1542d2ed70b34f4340172602b99c28e01d7834",
    "962a83a14424c9cd915b697ea2742471bcc91681c7dcf493c9270287d131585a",
    "97ac01ca30caa9d4824e884c0cac8ca4d8916de41ec5515ec55c712d328ada06",
    "97acaeda90d55f1ae266b81c321c34da921d3a4a587b43088a112c05f689b933",
    "97b078d9f3257d27cf3077da5fb4b5a8fb02e3f71c3a0ee1899a94613f6054c6",
    "97b92d15ac1f720684881e49f9b9aed8206a1887559dd1babb29607006a5daad",
    "97c13276d721c740b6140e5ab058cabc4f5b8b5da65ea94593d8ef23d6c229c4",
    "97f667990990934485907e53b66bd1ffda61df95d42b0e4e82316070bc070d16",
    "9845e44d12da1d12a9dd822b3573634505f52ecac9660b56176feecc51bc43a5",
    "9870d18db503b0a42ca5b6fdd8575dc8cc5f0a1f3e0b639e7cb61d1654286df2",
    "98bf3b120bc01b6dbdaede747730d67667886fa96f6cfcb20cc06b36219ddd7c",
    "990e4425d9bcb88234acf14d4036f44ac44db6d8e166452e062b7fb4e4c854f7",
    "994a1eb198bca081a62bb3489b95f5059c40bf1a326863e279532e83d6b74426",
    "999c634cba98b65596ed96f263c86957d59227716f231a6e722e0ddf0b9628a0",
    "99c79b184897a51a7fb427f2ccc5226e5c9e39e4a1b5213b61459287d63b85a2",
    "9a5f732382f7882955e65d5944d8d070d205ae6d46b73f0faa6c6a136875451f",
    "9a910994461872a20ceb18921135a5acd51f91bc4818f63de0db5037a5182a36",
    "9ab576f818fa671a6e8c88731e55feb0b409049d7a5b21a82e9a46b11f4ab91f",
    "9af48446ea007b0902253262caf903d88016f986222da4a10512e583ea231d7c",
    "9b6efbcd79df82db6ddb31c6c535e9c65b123b8f87e76c80e5b9e2a587aee530",
    "9ba39f654d7678ea15370a782f9f3e592b2b106679ee76a480bb419111c4b2e2",
    "9c1b3a0f6505834479d8ef97641f2f5e401875c3f58a378dea3707c83baf6326",
    "9c59b6d06f9755f93efdc591a80daaaaa4fa66c9f8519498a82951fbddb40384",
    "9c5c2141d8e4da146c845ac12f3e04847c409b489b96a5594df8cc3dc658dcf7",
    "9cb2c8af05b8dbdfa79f666e3e5825c319f0b90142af456bbe57bc98989c4196",
    "9d2edfc41158531e987cc6e64b0149ae2307b28e0322ad6abd4baf17108f9ecc",
    "9d852013efaf5310720434e1caeece9812f1cc8b27bb9bd8994c0236b50f078a",
    "9dad11f01422cf5da2982aa3cee85776083dc1802a12f1244b062f4b22b82567",
    "9dd5a898dbfb0846e966cc1ab240c435b433173c5e6c731b0789af8e211f5185",
    "9e3c11978b0bc55764fdaad536776fb80f2b332e5319ea2c8f244882616f7238",
    "a0734b9137fdf79be3be49070f3d4733e3056afd94e91de839826d2889f334ed",
    "a0925e7a296388b4001ec8fffad5de2935b342f7a5c6fcc243bc6f7ee4a00606",
    "a098faff46613a453f6beefca10a2b9818d934f68c694222f18576656bdbfe4a",
    "a1778ac2d879c15e9ad71ba05016add062697a4f6d13fddc141c238ea7861b29",
    "a25181902237df5767174fddfe55843cac9755a04ce28da76581a4c4879b7dc1",
    "a29c0dd97f488448275429bee87fc4a746e2efc5f3451f4923290c3c0b8e186e",
    "a2e368dd7715ddf91dacaa7436aca37e21e62fddce20b40b2393588cffa6fef1",
    "a31b76aa1d5213e40847c83acc822802c8f4ea1aa0a25a631325c1b6859de4f5",
    "a3be397862eed7d4fa1749e386df2843ff392f96eb6a0813bc80d1ea0f280283",
    "a4008b71563b0bd797df3aa070fb923c826d12cbc5c6b7a3de956d85369b712b",
    "a452a35c23ee30a09f4fbba1b52a1dc20a27f1276ca4d714697c306bac03af0b",
    "a478a2612c2bc46baa9dc5de40f45506aeeac10edb7da6473fafe19030e502ff",
    "a49a1bf3590a6f59d39f8a29c7bfc46624a278867ffb5d3277654081c4a7d713",
    "a4c6d8c2cb1b8cce34f30565472c9ae2b39c124963386ea5ec18476d6be122f1",
    "a4e776407bab2137d449c43cfbe8c60b7efaffb970a47a000fd8107cdedc9c1b",
    "a513c7a7fea3fc995ad2ac276afde193320235fb0054864aabd57267a26e8632",
    "a569365d79588020c13e162a2fe36c67ee94713be2b204af7d3a035ed2519338",
    "a5a524e0be7b121d5eea3f0552f43c41c7e020096f8403822fd8efc3a0a52873",
    "a5edb9903290b2784eec96bada2b9def26be1b3612620393dfc6f60d986d2541",
    "a6104538af345e406787f757bcbb51435600624637e653040d96d234066537dd",
    "a6a4a4056b97035ad1a8a427ee66e35893b47b90d96f5ed51ed2228907f299be",
    "a6bd58939232e0e91e49264ccc553c808ca0858fd7bb89dc0fd2786acdf3abf8",
    "a6db31008513c69cd24f287a0b655127bc09f6368c8a8f9eb5c4a52f68c9fe34",
    "a7c27d82f58fe82f48a498711331d881fc100542d84cfcda0a411911bb8b0dc2",
    "a834de337334283d5d672df5516e521afc4e1a8cceb4638df13150374d074f82",
    "a89091a07128d8029cee7b9653fe8b25be848002c8a77cb479953eb6d7e91612",
    "a8d533859f3e3a53ce70b2ac02cee05c88f3f4ef2d05bddbcfd78fc82a777a7f",
    "a957ec095c9ef2e9ed38940f0adf31f58079e450ae025d73536f2163b98b51a5",
    "a9f9ab3924d70e40ffbe4421d39554fef80bb6779137747968d38e4c29ab8617",
    "aa3fd569760656489076c5be11b1d68c06a3aa11391d987532fd65dcfbd785f2",
    "aa95c9ca39352983a160dc9a475987a02305837a4450418fc03b43f335a50f55",
    "aae4b8cbe162dbae77f3f3b363ecebf40448586ba8175823f4469cdf01de8cd7",
    "ab9c0bddf3a3c0d3c22d54c6039d2c8d1d0f0af5e967eaed0d86edc8175252bf",
    "ac098240d453162bbb975719e71a9840103d1a064ece651e75f51e1141e09d79",
    "ac3e563c55a4b14c3a77ca970ad1c487be1cd65868226fed6320b119d988b099",
    "ac8db2124d11786893be1a2be9adc6819e9af57f569ab66eb51d1812f4fc5ade",
    "ad5326b014587ddd2ef2326db282b7ddf87f461c718b379edbc582dd16b1d0d9",
    "ae6b73f18cbe07075d4d26b1b565bedca728c2c1e15bdebefecb1eb50d2b0ac7",
    "aeb281d3e166fdbf76f44d40837f1c2c674c902d418de1ce1172c6aeced32c4d",
    "af1458085c20e65b418a71a4764df2d6da2e8649c4a6bd614fb7bad0f9826443",
    "afa9cf9b7a92d67e06d0028c4011b7d2cdace5f70c51b8a1e8687232b33a38f7",
    "afbaed54b97c05cc8bbc35419756d97c8ee4b88cef22779e5db5b9aa6e9ce8ed",
    "b03e75a82fcdfb509f2e5da38f45964cd875419626e36c8c71a6f47a493fb5c3",
    "b052babae59a131b96de061b312a09c4c0f7f24b3c34030f7c32495b09a8f510",
    "b0c4742a89f31eab42da2aa01097b8735fdd356d670997c4f512cc11f86547df",
    "b0d57dd779b6c146c7eb37d59cb52aea87fe7a8b70a4509749c63dd9dcd04a5c",
    "b114f8bac8690e98c25848e0fd3e39c01cde79e712a4180de8616e0c01342aeb",
    "b1bc0607f33f5ef24b51a145113f3019694ebc37ee0fd4d679586449db11853b",
    "b1e35cdf16d3b31272bbb78b236ba3a36b8eb944e4e306b6ff2e5759ba9f14d4",
    "b25d31c0b20a186c8ef698bf5d48bc10d9ef5db94bfff2b50ab315e6ce0b1f8a",
    "b2cb5f468fe5bd7c17e46bbff85702b26a58b8835f9819fd9d132cff5bfebe50",
    "b341b52769bea23f6c6e1c33a09e5cb50602affe18d314930cdb864d40a5486b",
    "b584296e708384ce339de881b63184a1c304f2139985e64f9d3a01c2d8e783bc",
    "b62f8effd505be395fd0601e680baa57fb80031bf37711817f6ef080bc22876d",
    "b66a9562804e3db83cdcd9ed717b688cbbd4bb02b328b69ad7a4c594a74e3fb1",
    "b776c2882a8016e2230bf98d48ad9c4c2675cd306b3b1604d9ba70f09008c8f4",
    "b7faa1d0e085387325e2dd63aad57996d694d8f82211a180a2da9542c665b429",
    "b8a402c7fbdcce44f9d2158069476fd4f5b142e72f1dbd365dfab72d45089062",
    "b8bce5a64ad5105239674188fd1b075bab38065f7f28b6ac38e38dd4804a7469",
    "b9e3d77fead476e7fe6798d53d10d48752a16456457cbcefceb6d5a0fa50ec05",
    "ba02dd5bed12c6c240ef5f31b52b22b99b3b23257ca705501ce2a28a06918661",
    "bb370826c18a3b5bd6b1b757fefe6e2b8f3130f1671891958b82cb6517cc172a",
    "bb6ff7fcc5802490dca545436813f64c7a5994c3a8916e1a939c986f9570bf00",
    "bb96b01180b0b468a4a3822aac01e09f43a1d6ced3de7c4c1fdbefcc51c948d5",
    "bd1994d10fca7679e128be0654f943623d2a8c054327893f092e9d46e332acb7",
    "bdc3d1fa7253e23fb80e2688225d76e4cb31e5ca2505d0462efeb2c2327251d6",
    "bdc468ba46edc130f9c17895732f6941fb16e98d2b6123a52ef05cd83d50e25b",
    "be64e55b725b96cddf01fa7aeb18acc9800833b48953523969a44e650028f5bd",
    "beec33934d044284a90c58e61593955fd488376bf1e27411b73f8a2ffdeca0f4",
    "bf647861c237e185130ef5ee5d3c89b76b8f983109358667853258edd89e8fbc",
    "bf757ec55e0260265542196535e0beeddba77f9e27b03b7acf31425bd216499f",
    "bfa11439fc98cb16309dafb5f97cacf7b537d7d67b2682b9e2d26c1e2858c990",
    "bfbd8d8aad4868641389e1a6fa0c688d7bd269e0a1be41bdc4fe6b280bf44bcd",
    "c041fbec7765449e897e14a86eb4da36609109c09e2e21e57b2d0611d61d8f36",
    "c2c1e16cc603a33c535db16c590aec94b64b3dd7678cc6d3a49b52bf32016ddf",
    "c3f2300976145e0280af411ca257d323e1750afced218b704acd01d0c3deb617",
    "c3f42d85359336f59ddda3888cc3e79ea63da5fb672a36aafd9a0fd9d2b40381",
    "c5160b33e11d7b7ed1f9e5dad3f9e37dedccf9cf6dc87fa55c7319c274635e0c",
    "c595a10219931c909c736b0b32492fe9364a86b7ac6c4f490318b10f9c1df6cc",
    "c5e45820ec14d52eba2863d3dd61ab0f4e17276a5db8d307aaaaa61adccd97a4",
    "c60bc1ab8786ec755c027fccc9f00d488b2fe35c5ae47a4a8868d1269899df60",
    "c687afe2e59ca9b776d92ebedd73ecacab632c15427f5efd3c58053c2a58f3e2",
    "c82bb6764b1dbf69d1d6461e59952a7b1bdd15128bb4646a026467d4735f2f18",
    "c84d0ff7c0541e0a1d2fa768c76e409ba7c8a45d8fecca69da7e60de376ec3e6",
    "c852845cfb20ab25e62ed2740cec64c0c81443ad56db7e32a0830e7bc2c62882",
    "c85aa33b63b9e799fd1fe3dfbb4517f8e874cf3e50ffd89d93300a3c3ac5f17a",
    "c8d9b672a0046a84b8a43fb2e692514a092f09c1a942149fb731e315736ab0fc",
    "cae0e9f216f5bc2509a6c2e4e572f7c52f8b0dc208edc356460ca040bc104827",
    "caf4307fcb072360344c22898299e64f81a49353ea54ed12c56384f149697e54",
    "cbf15267d77d60e9fce65b0a29447cb0496df2085061546bc1f1230f357638ec",
    "cc9a0b97516f8cbedb0879c6c93c03bc99c63cbea199ed5b28415432a15fd3a6",
    "ccafd06bcda239da3105b3ea43e78c1e14d669fb65b78f47781c3174c0160c32",
    "ccb7c7a4c7a5aaec246d9fd3ccd0ffeca7a540b7e390df27742c934674085227",
    "cd376e7cdbabaf5f02c84a582b1022980332d4afe271895b88e5c8b23fd03e28",
    "cde86e1d736b73fa2089346f5d7fa21d75ef07f76c4de7bbfc082248a39d6880",
    "ce7b91d089a66764e50078d560d96ec06bb73b68b058ca493a265a0dd5455dae",
    "cea46e56c0cf16d3057d99e4ba22c9ba10f947cac4061f910c092773e3b5f151",
    "cf23bbdaf510f65066d909b02458ff9073f2c0435727835d5c8cc23ddbe23f41",
    "d17acc175764f6ca6bbf3e51da4cc2c0e47a8a15fc2dde3bfeafaf1636435e69",
    "d1e1aa41844d5dc82fbc9dd753f7b0edd3ff1ae478b9758f2efd8a0b85d31783",
    "d24574024e03f19c16712c1cf00756025357292ee283229bbd77cf58f7826509",
    "d43acf9c2f215bb90e1a04b191c6801c35a626219f18e1e69246478b6a93189a",
    "d4f560f13e9ea0793011381541131805349c6940533f3d02489a3b00234efaf5",
    "d5519d1d49902cdadca727226c828636f075caeda4325f6b08ce7a26e095cd47",
    "d5a9a0b0c40367cbc08a649f746de5e91ec94d7c8a70e908a40988f10188ddde",
    "d5dadf578f4d6ab185dbdd245aca3536673b4f108216feb67ab972b34b010ed4",
    "d5e5e07a48147ab350615872f8b05aebffd4a2d7ea3697a72ff41c5f00c9aca0",
    "d5ef52172c1237dd7079f213fc0086489c86926a06a5b2c97ad4a081cd9bd02c",
    "d67e3fc63ec7b77213a6c02895e1a513ce133ac47469e85e6529b9be1e28fd7e",
    "d6854738bc7fc977adde05333669711fc4c7e56133a0efbf27c2c85e5c6c3ee1",
    "d7340bb3508068640b4511c1df1f8b201259c654f07bd8f52162fbc75f900b83",
    "d9132de418e27ff5f225add6ce1d45616c17b8af4ab40288e7a0b4a5007a9268",
    "d933c2850d7c19321fe71c6fbbb9acd6880af3ad5e65aa87d367663c7322bcb2",
    "d99224c484af505a49996c8f323685043ec31f98bd8e14d83bdadb089cc252b5",
    "d9c6fec4593ed8bd5827b5955f15b1d28433b3305c7c40734d26148e998df8c9",
    "d9ca6c3db309925c9fbfb7f3df2af8b5a072b771e6b2896b99204012a58b00b4",
    "da883553453aa4f8d294080471cfa20be9d7683e4fcb1bf710befd345022d372",
    "db926cc22d17ae9d0ab3519ce1bf3602616a24db0456ab97272f2ba4c53466ed",
    "dbb81b53ba9d277c5647de77885ddba66999cb6b0d77f28e4049edea19f55f47",
    "dbca91c61b0dcc21fb597c16bdf2cfd474d08caa530df3d753035f105add9a2b",
    "dbe62b7557c315685d8d95c968b2fcd27145174952ab18ef880dfb36aa733d40",
    "dc121fd5e76de0d0372b89334f4495b97e8f1a02c56d9ffc9ebb922f83f29240",
    "dce20fd1f17ffa055f8d8e91e123ea0a8f4fe95f4ad07624d4ab5b545305c2d8",
    "de729d602078f17feff8a78eeab02d14a252282ed594f2e4bc026595481f7a7c",
    "de9706678ec1a2406b258d7656eea653b8ba69d5c5d84fc5d3e6f488a72c2e2a",
    "ded9087e9793dac0355cceb4fdd3ed45c9bc7c04810a3fb6455f100df05f7ddd",
    "df094564cd6f8b93bc42f943f217f28a31309215a5ba974c2d2900b5622f70de",
    "df1549e59fb3806efc6f0b47857a925e001bdf5589f260bb8003db25a123ce71",
    "df52aff836620a03024cb0dfac1abf29b278f689711f00513d47ac513dc5b9d3",
    "df86760df82c659afab67af23ec3492a5b2ea458ff344c9e1dd21618454ccda4",
    "df8ed4e2995d3cbead1c3af80aee5e7620ae42e71a3303c74d1a5041dda0436e",
    "dfdd47b5885848594828c7c82adfe3b30e8ac7381111f1bee2207158add50ca8",
    "dfe9211dc7c16767c6f8947b7ac5ea346d13ca401a36f583b46df00a0aacd274",
    "e1bae4dc5d4dab65563e3d17de2c07a44b25881d06dda6617a7c96a9d5f1d7c4",
    "e2923b705c7f286decaf2ad417ec3cd69c93d67b709880c9554f984723728ab2",
    "e2fb25ca0111d442d1ef27044e01280bd074718020c35170a444e4ab542c2fc2",
    "e4a0be5f422d7a4d3faf6bda98bb00f46a295e804fa4b92ff74a69cf50b088f6",
    "e535dea746c3e0a1a5eef12cac8052a25c26586c876fe55ca0d6a27cb36f0591",
    "e5f2aaf458ad454a6aa2b4e9473a12e84731b9dc442fecd1ddc865d5516e9238",
    "e5f3be9c9a0afe1d040b7caa97d10e63fad565645c438f414af5c324a6f745c7",
    "e67974bd0c0f3dc780bae0f33ad6ad499e7db946f4bae94be8a8770429cb12cf",
    "e6befe721b1aeada8eb684db68190795a463271e9294c85298ec7a0314b45ebb",
    "e7a0ab102488d99197ae1e10e38508fa47f2c12505c190d5fc10614aceef8da4",
    "e7a99074d53a12b8130c7d6b57da091e6767fb8a04d50e588c87ded2ee536c12",
    "e8accab3f3ed05f2be1972223d04f33a7abd50f650f26102b6f255517fa3a22f",
    "e9e35e94df123b7c66ccc8ba45231e15d3be1d8173db631e51276fb14a3ac999",
    "eab3c8fa2c8e6df2f1b909687a44a3f6a1f6da571344748d810ad55dcf43c787",
    "eada5b71fe7210a7c8d76c59b50fdda2a2bce1a4f383e81ae404fd89a3841a51",
    "ebacf0b5432b6d3360b71abf90668bc9bb9442f4cea86231929e644abda17193",
    "ec61d0990be993d367e07cf61dc4a95f932a64e6eb871ec33a479b5132266716",
    "eca4f4c50fff2affce7308b677c12a940b46caf3bfdb78e8d3e11c8410e7a863",
    "ed4c194de4911f0ec2ca0b397766705e0018ba3abae6be48d6ced888f58891da",
    "edef18eef9d2675ca90f2c127a0b97c1a472d4a0118c5ff9f1a56fb96effd7c0",
    "ee19c895e607dfd777b3afe645cd2f3823b4dc6b9bb5c835561ce4c6884173ae",
    "eeef8c9a07e9cd3e74bc0afda4033230e6e498d94f50b5b750c4fadb2ad35454",
    "ef845943d84a793db7a6379a2589fb13ef951ed3e178e679ddcdafe9ad858de7",
    "f093ddd89cb15410e32ed7a92f63c793285944dd57fff4c144f0dcb6183ec05c",
    "f0be0b5592fb9c39f48ade9ed6ff59cc35d4e903115eb9b59e1bd86a57459ec0",
    "f0f7dd1cdebb9dd4963eba2689c8bdfd6cf4bdd890d54915669e8cc92ff52543",
    "f1321cb1846d55ec3eed6d28882fbd1a39b23b8825324bcc34a48b7cd776ef60",
    "f1767dcad6c90978507b2c63dca4e4ba8ddcfcb60f27ff40a5cd4333f86a7540",
    "f17d0cb7ec869584d4e325c36e26ff071769e4ef05fcd588e12e5e028bb028ec",
    "f1a14ddc9dcafefc50e1fe9a03ccda3f76a135d28b036cd1a32e21861e781c33",
    "f252d12dee965589262f7c05f95172470ded415881b4f86c10c23622a4cde238",
    "f31dac6068fed1e13966ab755068bdc4ba0f19ae9e617700d0386c7a51177df3",
    "f3acb6ad8a69bf482f00798e9c0eb0653cc2d33a75087a14f8565757823e50cd",
    "f3f2c8d7df23b6212aaf0fd387f9b3ebca7b7e01dd303b04eb8683d778326fcc",
    "f4e9d884f6a7ff45f3db965a340c9c7bf0100ace70da2ce176c9f0152827b7d8",
    "f522af501a49c683f91f9ec6957f6e57d60fafc8227e99e0192d0e55918812ac",
    "f55aa4789782b8ea68009eb0f2c83b43ea095e2c3d95c5f838d8c54932d5cec9",
    "f5b164d7b109a9901becdf127e7612aad9e380f8577ecbf768caf631cf8665b6",
    "f84e4bba2d91c7a9c1f04c415660e08aec4836abea7578663528b8cf0af50071",
    "f85290424d9319c42ffc452362a11a154f229c0b8738fa33ceafbf7441e2c9d2",
    "f8c46b8cd4c8d72c474f2220438005172056c6deae77df269ab84384b69dc417",
    "f8ed4e7893b0288348d766967bae3ded139a89a06ebed908fc3d3fe2b5f35907",
    "f9fabe079fb6a5668139a04396cd9d23c48bd72739cdd6c11780854408f948aa",
    "fa1cb9ae1b8253f71034258152f418931e49de22e3c8102f7a7fd28287ce70ca",
    "fad6ea81d528c48e0925bf4d55843960544e5ddaf453c82f48561a88dbcb4b3d",
    "fbba42c595c10bbb7045afeea9c5fd5233d991a7b30f5b24013ac8946c074db8",
    "fbcf69a97cf86804124ddde2b76e7764e5222ea74683e724abafd690773abba6",
    "fc27db536ac6d4d6a80c87cc29cc8356be07b34af7e054b4b6b142df633ab4d2",
    "fcbf1ce3bd2abe7a955a40196d4e78449dd0496ef84b20e1aca51038c5354584",
    "fd1c8d2713a33bd94e5b703187375f941955b99c8218c02afbb8b27d669e1e4a",
    "fd22d357cc8eda14a1b85216fe7d0ff306949637c1ce4a262fb3252985e8e9b6",
    "fe7f76c659ca945939081f25c2ebd3e4b9855a0a23eafbd585bfd69bdf7cbf3d",
    "ff61cdf01fb14b0d3f00f021308a3ff4718bda89b57d41316efa4e75704ab22a",
    "ffb99db696d2f33cd8fc4265a667cee331009790b0f32c2ec9c48c1f141072ef",
];

/// 滑窗大小：12 个 Unicode 字符（码点）。
const CASE_TEXT_WINDOW: usize = 12;

#[test]
fn no_case_long_sentence_in_repo() {
    let (files, mode) = collect_files();
    assert!(
        !files.is_empty(),
        "长句守卫未取到任何待扫文件（mode={}）——空扫描不允许算通过",
        mode
    );

    // 构建摘要集合用于 O(1) 查找
    let digest_set: HashSet<&str> = CASE_TEXT_12CHAR_SHA256_DIGESTS.iter().copied().collect();

    let mut scanned: usize = 0;
    let mut violations: Vec<String> = Vec::new();

    for file in &files {
        let file = file.as_str();
        if WHITELIST.contains(&file) {
            continue;
        }

        let content = match std::fs::read_to_string(file) {
            Ok(c) => c,
            Err(_) => continue, // 跳过二进制文件
        };

        scanned += 1;

        let chars: Vec<char> = content.chars().collect();
        if chars.len() < CASE_TEXT_WINDOW {
            continue;
        }

        for i in 0..=chars.len() - CASE_TEXT_WINDOW {
            let window: String = chars[i..i + CASE_TEXT_WINDOW].iter().collect();
            let mut hasher = Sha256::new();
            hasher.update(window.as_bytes());
            let digest = hasher.finalize();
            let hex = format!("{:x}", digest);

            if digest_set.contains(hex.as_str()) {
                violations.push(format!(
                    "{}: 第{}字符位置命中 12 字长句守卫（SHA256={}）",
                    file,
                    i,
                    &hex[..16]
                ));
                // 每个文件最多报 3 条，避免日志爆炸
                let count_for_file = violations
                    .iter()
                    .filter(|v| v.starts_with(&format!("{}:", file)))
                    .count();
                if count_for_file >= 3 {
                    break;
                }
            }
        }
    }

    eprintln!(
        "长句守卫：扫描方式={}，{} 个文件，{} 条摘要",
        mode,
        scanned,
        digest_set.len()
    );

    if !violations.is_empty() {
        panic!(
            "长句守卫发现 {} 处违规：\n{}",
            violations.len(),
            violations.join("\n")
        );
    }
}


// ── Negative controls: prove check_text actually fires on violations ──
// These tests construct synthetic violations at runtime (same concatenation
// technique as the forbidden_* functions) to avoid tripping the guard on
// this test file itself.

#[test]
fn negative_control_app_number_shape_detected() {
    // Construct a 13-digit application number at runtime
    let parts: [&str; 4] = ["2024", "0101", "2345", "6"];
    let synthetic: String = parts.iter().copied().collect();
    let hits = check_text(&synthetic, true);
    assert!(
        hits.iter().any(|h| h.contains("申请号形状")),
        "negative control: 13-digit app number must be detected in src prod area, got {:?}",
        hits
    );
}

#[test]
fn negative_control_pub_number_shape_detected() {
    // Construct a CN+digits+letter publication number at runtime
    let synthetic = format!("{}{}{}", "CN", "12345678", "B");
    let hits = check_text(&synthetic, true);
    assert!(
        hits.iter().any(|h| h.contains("公开号形状")),
        "negative control: CN+8digits+letter must be detected in src prod area, got {:?}",
        hits
    );
}

#[test]
fn negative_control_clean_text_passes() {
    // A string with no case data must produce no violations
    let clean = "This is a normal comment with no sensitive data.";
    let hits = check_text(clean, true);
    assert!(
        hits.is_empty(),
        "negative control: clean text must not trigger any violation, got {:?}",
        hits
    );
}

#[test]
fn negative_control_shape_check_only_in_src_prod() {
    // The same 13-digit number in a non-src context (is_src_rust_prod=false)
    // should NOT trigger the shape checks (only literal forbidden values are
    // checked everywhere; shape regexes are src-only).
    let parts: [&str; 4] = ["2024", "0101", "2345", "6"];
    let synthetic: String = parts.iter().copied().collect();
    let hits = check_text(&synthetic, false);
    assert!(
        !hits.iter().any(|h| h.contains("形状")),
        "negative control: shape checks must not fire outside src prod area, got {:?}",
        hits
    );
}
