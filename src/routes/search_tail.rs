use super::*;

#[cfg(test)]
mod tests {
    use super::*;

    /// **队列 #1 出参形状锁（2026-11-08）**：网页回退的诚实承诺——`patents` 恒空、
    /// `web_results` 携带 title/snippet/url、`source`=`firecrawl_web`、hint/cooldowns
    /// 空则整键省略（与胜者路径同一口径）。
    #[test]
    fn firecrawl_web_results_json_locks_honest_shape() {
        let req = SearchRequest {
            query: "流量调节".into(),
            page: 1,
            page_size: 20,
            country: None,
            search_type: None,
            language: None,
            region: None,
            assignee: None,
            exact_assignee: false,
            date_from: None,
            date_to: None,
            sort_by: None,
            ipc: None,
            cpc: None,
        };
        let hits = vec![
            crate::search::providers::firecrawl::FirecrawlWebHit {
                title: "一种数字式流量智能调节系统 - Google Patents".into(),
                snippet: "本发明涉及流量调节领域。".into(),
                url: "https://patents.google.com/patent/CN114962757A/zh".into(),
            },
            crate::search::providers::firecrawl::FirecrawlWebHit {
                title: "论坛讨论帖".into(),
                snippet: "".into(),
                url: "https://example.com/thread/1".into(),
            },
        ];
        let attempts = serde_json::json!([{"source": "firecrawl_web", "status": "success"}]);
        let out = firecrawl_web_results_json(&hits, &req, attempts.clone(), None, &[]);
        // 诚实承诺：patents 恒空、网页命中不进专利数组
        assert_eq!(serde_json::json!([]), out["patents"]);
        let web = out["web_results"]
            .as_array()
            .expect("web_results 必须为数组");
        assert_eq!(2, web.len());
        assert_eq!(
            "https://patents.google.com/patent/CN114962757A/zh",
            web[0]["url"].as_str().unwrap()
        );
        assert_eq!("论坛讨论帖", web[1]["title"].as_str().unwrap());
        // source 与总数
        assert_eq!("firecrawl_web", out["source"].as_str().unwrap());
        assert_eq!(2, out["total"].as_u64().unwrap());
        // 回退说明必须点明「非专利著录」（来源标注义务在文案层也要可见）
        assert!(
            out["message"].as_str().unwrap().contains("非专利著录"),
            "{:?}",
            out["message"]
        );
        // attempts 原样带出（诊断面板可见 firecrawl_web 行）
        assert_eq!(attempts, out["attempts"]);
        // 空则整键省略：hint / cooldowns 不得出现
        assert!(out.get("hint").is_none(), "无 hint 时整键省略");
        assert!(out.get("cooldowns").is_none(), "无冷却时整键省略");
        // page / page_size 回显请求值（网页回退无真实翻页，不伪造口径）
        assert_eq!(1, out["page"].as_u64().unwrap());
        assert_eq!(20, out["page_size"].as_u64().unwrap());
        // 有 hint 时必须带出
        let out2 = firecrawl_web_results_json(&hits, &req, attempts, Some("H".into()), &[]);
        assert_eq!("H", out2["hint"].as_str().unwrap());
    }

    /// **2026-11-05 在线排序锁**：`apply_online_sort` 让下拉「最新/最早」在在线路径
    /// 真实生效（旧实现从不消费 `sort_by`，上游即便按日期返回也被混合分降序打回）；
    /// `relevance`/缺省必须**不动**输入顺序（现状=混合分降序由 dedup 保证）。
    #[test]
    fn apply_online_sort_honors_new_old_and_leaves_relevance() {
        let mk = |pn: &str, date: &str| PatentSummary {
            id: pn.into(),
            patent_number: pn.into(),
            title: pn.into(),
            abstract_text: String::new(),
            applicant: String::new(),
            inventor: String::new(),
            filing_date: date.into(),
            country: "CN".into(),
            relevance_score: None,
            score_source: None,
            matched_fields: None,
        };
        let base = || vec![mk("A", "2015-01-30"), mk("B", "2023-07-12"), mk("C", "")];

        let mut v = base();
        apply_online_sort(&mut v, Some("new"));
        assert_eq!(
            vec!["B", "A", "C"],
            v.iter()
                .map(|p| p.patent_number.as_str())
                .collect::<Vec<_>>(),
            "最新：申请日降序，空日期沉底"
        );

        let mut v = base();
        apply_online_sort(&mut v, Some("old"));
        assert_eq!(
            vec!["C", "A", "B"],
            v.iter()
                .map(|p| p.patent_number.as_str())
                .collect::<Vec<_>>(),
            "最早：申请日升序"
        );

        let mut v = base();
        apply_online_sort(&mut v, Some("relevance"));
        apply_online_sort(&mut v, None);
        assert_eq!(
            vec!["A", "B", "C"],
            v.iter()
                .map(|p| p.patent_number.as_str())
                .collect::<Vec<_>>(),
            "relevance/缺省不得重排（保持 dedup 的混合分降序）"
        );
    }

    // ── `/api/search/online` 的四条终态响应形状（MA1 抽出前后对照，逐字来自
    // `git show b08f1c5:src/routes/search.rs` 的 `api_search_online`）──
    //
    // | 路径 | 字段 | 锁定该形状的测试 |
    // |---|---|---|
    // | 空查询 | `patents/total/page/page_size/message` | `api_search_online` 的 early-return（未改） |
    // | 精确直查命中 | `patents/total=1/page=1/page_size=10/source=serpapi_exact` | `providers::serpapi::exact_lookup_response_shape_is_locked`；**MA6c 起进入该分支前多一道熔断冷却判据**（`exact_lookup_allowed`：SerpAPI 冷却中 ⇒ 跳过直查、落入降级链），由本文件 `exact_lookup_*` 三条用例锁 |
    // | 在线命中 | `patents/total/page/page_size=10/source=serpapi(+hint)` | `online_hit_response_keeps_legacy_shape` |
    // | 本地兜底 | `patents/total/page/page_size/source=local/hint` | 兜底段（未改） |
    //
    // 上游请求参数与结果映射的一致性证据在 `src/search/` 侧：
    // `query::render_q_matches_pre_migration_implementation`、
    // `query::resolve_lang_matches_pre_migration_region_flags`、
    // `providers::serpapi::search_url_matches_legacy_builder`、
    // `providers::serpapi::outcome_patents_match_legacy_mapping`。

    /// **MA2b 链形状（任务清单 2-③）+ Batch 1 学术补源**：未配 `EPO_KEY/EPO_SECRET`
    /// 时链里**压根不登记** EpoOps，因此 `attempts` 里不会出现该源（与「登记了但
    /// Skipped」是两种形状，后者见
    /// `search::chain::case6_epo_without_credentials_skips_and_sends_nothing`）。
    /// 配齐后追加在 OpenAlex 之前，顺序即降级优先级（spec §6）。
    /// OpenAlex（Batch 1 任务 1.5）免费无 Key，**恒登记链尾**。
    #[test]
    fn online_chain_registers_epo_only_with_credentials() {
        let db = Arc::new(Database::init(":memory:").expect("in-memory db"));
        let creds = || Some(("ck".to_string(), "cs".to_string()));

        let with_epo = SourceChain::new(online_chain_providers(
            &db,
            Some("serp-key".to_string()),
            creds(),
        ));
        assert_eq!(
            vec![
                SourceKind::SerpApi,
                SourceKind::GooglePatentsXhr,
                SourceKind::EpoOps,
                SourceKind::OpenAlex
            ],
            with_epo.kinds(),
            "四源齐配时的链形状 = 优先级顺序（OpenAlex 恒登记链尾）"
        );

        let without_epo = SourceChain::new(online_chain_providers(
            &db,
            Some("serp-key".to_string()),
            None,
        ));
        assert_eq!(
            vec![
                SourceKind::SerpApi,
                SourceKind::GooglePatentsXhr,
                SourceKind::OpenAlex
            ],
            without_epo.kinds(),
            "未配 EPO 凭证即不登记该源（与 SerpAPI 同语义）；OpenAlex 仍在"
        );

        // SerpAPI 缺 Key 只摘掉它自己，不影响 EPO/OpenAlex 的登记与相对顺序。
        let without_serpapi = SourceChain::new(online_chain_providers(&db, None, creds()));
        assert_eq!(
            vec![
                SourceKind::GooglePatentsXhr,
                SourceKind::EpoOps,
                SourceKind::OpenAlex
            ],
            without_serpapi.kinds()
        );

        // 专利源全不可用（两 Key 皆无）时链仍非空：免费的 XHR 直抓 + OpenAlex 恒登记，
        // 学术文献兜底之后才轮到本地库。
        let bare = SourceChain::new(online_chain_providers(&db, None, None));
        assert_eq!(
            vec![SourceKind::GooglePatentsXhr, SourceKind::OpenAlex],
            bare.kinds()
        );
        assert!(!bare.is_empty());
    }

    // ── MA6a：源级熔断冷却的路由层接线（链前过滤 + Skipped 织回，假时钟零联网）──

    /// 测试夹具：一条最简 attempt 报告（weave/过滤用例只关心 source+status 的走向）。
    fn brk_report(source: SourceKind, status: AttemptStatus) -> AttemptReport {
        AttemptReport {
            source,
            status,
            latency_ms: 7,
            hits: 0,
            error: None,
            hint: None,
        }
    }

    /// **MA6a 用例③**：处于冷却的源被摘出链（`SourceChain` 拿不到它 → 结构上不可能出网），
    /// 但其 `Skipped` 记账按**原登记序**织回 attempts 原位；其余源不受牵连、正常择胜。
    /// 同时锁定「冷却原因放 error、hint 恒 None」——hint 会经 `SearchOutcome::hint()`
    /// 泄漏到出参顶层 `hint` 键，改动响应形状（取证见 `cooldown_skipped_report` 注释）。
    #[test]
    fn cooled_source_leaves_chain_but_keeps_skipped_attempt_in_place() {
        let db = Arc::new(Database::init(":memory:").expect("in-memory db"));
        let now = Instant::now();
        let mut table = CooldownTable::new();
        table.record(SourceKind::SerpApi, FailKind::Quota, now);

        let providers = online_chain_providers(
            &db,
            Some("serp-key".to_string()),
            Some(("ck".to_string(), "cs".to_string())),
        );
        let order: Vec<SourceKind> = providers.iter().map(|p| p.kind()).collect();
        assert_eq!(
            vec![
                SourceKind::SerpApi,
                SourceKind::GooglePatentsXhr,
                SourceKind::EpoOps,
                SourceKind::OpenAlex
            ],
            order
        );

        let (active, cooled) = filter_cooled_providers(providers, &table, now);
        assert_eq!(
            vec![
                SourceKind::GooglePatentsXhr,
                SourceKind::EpoOps,
                SourceKind::OpenAlex
            ],
            active.iter().map(|p| p.kind()).collect::<Vec<_>>(),
            "冷却源必须摘到链外——链只跑手里的 providers，摘掉即零请求"
        );
        assert_eq!(
            vec![(SourceKind::SerpApi, Duration::from_secs(300))],
            cooled
        );

        // 链只跑了 XHR（成功）、EPO（网络失败）与 OpenAlex（成功），
        // attempts 本里没有 SerpAPI 的位置
        let chain_attempts = vec![
            brk_report(SourceKind::GooglePatentsXhr, AttemptStatus::Success),
            brk_report(SourceKind::EpoOps, AttemptStatus::Failed(FailKind::Network)),
            brk_report(SourceKind::OpenAlex, AttemptStatus::Success),
        ];
        let merged = weave_cooled_attempts(&order, &cooled, chain_attempts);

        assert_eq!(
            4,
            merged.len(),
            "冷却源不能从面板消失（伪装成没配 Key 是误读）"
        );
        assert_eq!(
            SourceKind::SerpApi,
            merged[0].source,
            "织回位置 = 原登记序第一位"
        );
        assert_eq!(AttemptStatus::Skipped, merged[0].status);
        assert_eq!(0, merged[0].latency_ms, "没发请求不得凭空记耗时");
        assert_eq!(0, merged[0].hits);
        let err = merged[0]
            .error
            .clone()
            .expect("Skipped 必须说明原因（error 字段，口径同缺 Key Skipped）");
        assert!(err.contains("serpapi"), "error 要点名源: {err}");
        assert!(err.contains("冷却"), "要说清是熔断冷却: {err}");
        assert!(err.contains("300s"), "剩余时长向上取整进文案: {err}");
        assert_eq!(
            None, merged[0].hint,
            "hint 必须留空，否则会污染出参顶层 hint 键"
        );
        // 真跑过的那三路原样保留、顺序不变
        assert_eq!(
            vec![
                (SourceKind::GooglePatentsXhr, AttemptStatus::Success),
                (SourceKind::EpoOps, AttemptStatus::Failed(FailKind::Network)),
                (SourceKind::OpenAlex, AttemptStatus::Success),
            ],
            merged[1..]
                .iter()
                .map(|a| (a.source, a.status))
                .collect::<Vec<_>>()
        );
    }

    /// **MA6a 用例④**：全部源都在冷却 → 链为空 → 本地兜底接管，
    /// 出参 `source:"local"` 等既有键与现状逐字一致，attempts 为「三源 Skipped + local_fts」。
    #[test]
    fn all_sources_cooled_yields_empty_chain_and_identical_local_shape() {
        let db = Database::init(":memory:").expect("in-memory db");
        // 兜底要有命中才走「source:local」出参档（零命中会落到 google_url 空结果末档，
        // 那是另一条既有形状）——本用例锁的是「冷却不改变兜底形状」，故种一条可命中专利。
        seed_patent(&db, "999", "固态电池");
        let now = Instant::now();
        let mut table = CooldownTable::new();
        table.record(SourceKind::SerpApi, FailKind::Quota, now);
        table.record(SourceKind::GooglePatentsXhr, FailKind::Quota, now);
        table.record(SourceKind::EpoOps, FailKind::Auth, now);

        let order = vec![
            SourceKind::SerpApi,
            SourceKind::GooglePatentsXhr,
            SourceKind::EpoOps,
        ];
        let cooled: Vec<(SourceKind, Duration)> = order
            .iter()
            .filter_map(|k| table.remaining(*k, now).map(|d| (*k, d)))
            .collect();
        assert_eq!(3, cooled.len());

        // 空链 = `SourceChain::new(vec![])` 的既有合法形状（chain.rs 文档已锁），零请求零 attempts
        let mut outcome = SearchOutcome {
            results: vec![],
            attempts: weave_cooled_attempts(&order, &cooled, vec![]),
            upstream_total: None,
        };
        assert_eq!(3, outcome.attempts.len());
        for a in &outcome.attempts {
            assert_eq!(AttemptStatus::Skipped, a.status);
        }

        let req = fallback_req("固态电池");
        let out = local_fallback_json(
            &db,
            &req,
            &Some(SearchType::Mixed),
            outcome.hint(),
            &mut outcome,
            &cooled,
        );

        // 兜底出参既有键逐字不变（MA4a 口径）：source 仍为 "local"；顶层 hint 是
        // 既有默认文案——冷却 Skipped 的 hint 恒 None，不得把它带进顶层键（现状即默认文案）
        assert_eq!(json!("local"), out["source"]);
        assert_eq!(json!(1), out["total"], "种入的可命中专利必须照常返回");
        assert_eq!(1, out["patents"].as_array().expect("patents").len());
        assert_eq!(
            json!("国外在线源暂时未返回结果，已回退本地缓存。建议配置 SerpAPI 提升命中率。"),
            out["hint"],
            "冷却记账不得经 hint() 通道改写顶层提示"
        );
        // 本地兜底自身照常追加为末条（MA4a 记账逻辑与 MA6a 过滤互不干扰）
        let attempts = out["attempts"].as_array().expect("attempts 键照旧存在");
        assert_eq!(4, attempts.len());
        assert_eq!(json!("local_fts"), attempts[3]["source"]);
        assert_eq!(json!("Success"), attempts[3]["status"]);
        assert_eq!(json!(1), attempts[3]["hits"]);
        assert_eq!(json!("Skipped"), attempts[0]["status"]);
        // MA6b：全冷却兜底档同样带出 `cooldowns` 键，顺序 = 原登记序、秒数 = 向上取整。
        // （此用例同时是「冷却不改变既有键」的证明：上面各既有键断言逐字未动。）
        assert_eq!(
            json!([
                {"source": "serpapi", "remaining_secs": 300},
                {"source": "google_patents_xhr", "remaining_secs": 120},
                {"source": "epo_ops", "remaining_secs": 900}
            ]),
            out["cooldowns"],
            "cooldowns 必须走真实 local_fallback_json 出参，而不是测试里复刻的假形状"
        );
    }

    /// 无冷却时 weave 必须是纯直通（不复制重排也不改顺序）——保证 MA6a 对
    /// 「一切正常」的热路径零行为差异。
    #[test]
    fn weave_is_passthrough_without_cooldowns() {
        let order = vec![
            SourceKind::SerpApi,
            SourceKind::GooglePatentsXhr,
            SourceKind::EpoOps,
        ];
        let chain_attempts = vec![
            brk_report(SourceKind::SerpApi, AttemptStatus::Success),
            brk_report(SourceKind::GooglePatentsXhr, AttemptStatus::Success),
        ];
        let merged = weave_cooled_attempts(&order, &[], chain_attempts.clone());
        assert_eq!(chain_attempts, merged);
    }

    /// **MA6c 用例①**：SerpAPI 处于熔断冷却时，专利号直查判据必须为 `false`
    /// （handler 据此跳过 details、自然落入降级链——链前过滤会把它摘链，冷却事实由
    /// `cooldowns` 键与织回的 `Skipped` 如实呈现，直查路径不重复记账）。
    /// 同一用例锁「未冷却时判据逐字等于 MA6c 之前的两层 `if`」：
    /// 非 PatentNumber ⇒ false、没配 Key ⇒ false、两者齐备且未冷却 ⇒ true。
    #[test]
    fn exact_lookup_is_blocked_while_serpapi_is_cooling() {
        let t0 = Instant::now();
        let pn = Some(SearchType::PatentNumber);
        let mut table = CooldownTable::new();

        // 未冷却档：旧的两条判据行为逐字不变
        assert!(
            exact_lookup_allowed(pn.as_ref(), true, &table, t0),
            "没冷却 ⇒ 专利号直查照旧出网（本包不得改变这一档）"
        );
        assert!(
            !exact_lookup_allowed(None, true, &table, t0),
            "不是专利号直查 ⇒ 判据为 false（旧判据①，来自 handler 的 matches! 那层）"
        );
        assert!(
            !exact_lookup_allowed(pn.as_ref(), false, &table, t0),
            "没配 SerpAPI Key ⇒ 判据为 false（旧判据②）"
        );

        // 冷却档：本包补的缺口
        table.record(SourceKind::SerpApi, FailKind::Quota, t0);
        assert!(
            !exact_lookup_allowed(pn.as_ref(), true, &table, t0 + Duration::from_secs(1)),
            "冷却中的 SerpAPI 不得再被直查白撞一发付费配额（还和面板上「serpapi 冷却中」自相矛盾）"
        );
        // 到期自动放行：冷却必须有终点，否则精确查号永久下线
        assert!(
            exact_lookup_allowed(
                pn.as_ref(),
                true,
                &table,
                t0 + breaker::SERPAPI_QUOTA_COOLDOWN
            ),
            "冷却到期即恢复直查（与链前过滤同一时长常量，禁止第二套标准）"
        );
    }

    /// **MA6c 用例②**：只挡该挡的源。details 是 SerpAPI 独占能力（XHR/EPO 的
    /// `lookup_exact` 恒 `None`），别的源冷却不得连带掐掉专利号直查；
    /// 反证 SerpAPI 自己一进冷却同一入参立刻为 `false`。
    #[test]
    fn exact_lookup_gating_is_serpapi_only() {
        let t0 = Instant::now();
        let mut table = CooldownTable::new();
        table.record(SourceKind::GooglePatentsXhr, FailKind::Quota, t0);
        table.record(SourceKind::EpoOps, FailKind::Auth, t0);
        assert!(
            exact_lookup_allowed(Some(&SearchType::PatentNumber), true, &table, t0),
            "冷却是源级的：无关源在冷却不能把 SerpAPI 独占的查号能力一起下线"
        );
        table.record(SourceKind::SerpApi, FailKind::Auth, t0);
        assert!(
            !exact_lookup_allowed(Some(&SearchType::PatentNumber), true, &table, t0),
            "同源（SerpApi）一冷却，同一入参立刻被挡"
        );
    }

    /// **MA6c 用例③（真接线）**：handler 读的是 `breaker::global_table()` 这张进程内共享表，
    /// 不是本地复刻的表。用链后回写的**同一个入口** `breaker::note_attempts` 往全局表
    /// 写一次 SerpAPI `Failed(Quota)`，再走 handler 用的同一条「加锁 + `exact_lookup_allowed`」
    /// 路径，必须读到 `false`。
    ///
    /// 纪律：全局表跨用例共享（std Mutex 不可重入，guard 也不得出块），故本用例
    /// **先把取值收进局部变量、恢复现场之后再断言**，保证即便断言失败也不会把
    /// 300s 的 SerpAPI 冷却留给同一进程里的并行用例。
    #[test]
    fn exact_lookup_gate_reads_the_shared_global_table() {
        let before = {
            let guard = breaker::global_table()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            guard.remaining(SourceKind::SerpApi, Instant::now())
        };

        breaker::note_attempts(&[brk_report(
            SourceKind::SerpApi,
            AttemptStatus::Failed(FailKind::Quota),
        )]);

        let (gate_during_cooldown, table_says_cooling) = {
            let guard = breaker::global_table()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            (
                exact_lookup_allowed(
                    Some(&SearchType::PatentNumber),
                    true,
                    &guard,
                    Instant::now(),
                ),
                guard
                    .remaining(SourceKind::SerpApi, Instant::now())
                    .is_some(),
            )
        };

        if before.is_none() {
            let mut guard = breaker::global_table()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            guard.success(SourceKind::SerpApi);
        }

        assert!(
            !gate_during_cooldown,
            "链后回写的真实失败必须同时挡住降级链与直查（否则本包接线是假接线）"
        );
        assert!(table_says_cooling, "note_attempts 必须真的写进了共享表");
        let restored = {
            let guard = breaker::global_table()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            guard
                .remaining(SourceKind::SerpApi, Instant::now())
                .is_some()
        };
        assert_eq!(
            before.is_some(),
            restored,
            "现场必须恢复：不给并行用例留下 SerpAPI 冷却"
        );
    }

    /// 未配置 Key 的路径必须与「源跑过但零命中」走同一条后续控制流：
    /// hint 文案逐字保持旧值，且不得被误判为在线命中。
    #[test]
    fn no_key_outcome_keeps_legacy_hint_and_falls_back() {
        let outcome = serpapi_skipped_outcome("未配置有效 SerpAPI Key，已自动尝试下游回退。");
        assert_eq!(
            Some("未配置有效 SerpAPI Key，已自动尝试下游回退。".to_string()),
            outcome.hint()
        );
        assert!(!outcome.succeeded_from(SourceKind::SerpApi));
        assert!(outcome.summaries().is_empty());
        assert_eq!(None, outcome.upstream_total);
    }

    /// 在线命中时 `SearchOutcome` 能还原出与旧代码同形的 JSON 片段
    /// （`total` 取上游 `search_information.total_results`，`page_size` 恒为 10）。
    #[test]
    fn online_hit_response_keeps_legacy_shape() {
        let summary = PatentSummary {
            id: "saved-1".to_string(),
            patent_number: "CN1123456A".to_string(),
            title: "固态电池".to_string(),
            abstract_text: "一种全固态电池结构。".to_string(),
            applicant: "某研究院".to_string(),
            inventor: "张三".to_string(),
            filing_date: "2024-01-01".to_string(),
            country: "CN".to_string(),
            relevance_score: Some(87.2),
            score_source: Some("hybrid(pos:98+content:80)".to_string()),
            matched_fields: None,
        };
        let outcome = SearchOutcome {
            results: crate::search::merge::merged_from(SourceKind::SerpApi, vec![summary.clone()]),
            attempts: vec![AttemptReport {
                source: SourceKind::SerpApi,
                status: AttemptStatus::Success,
                latency_ms: 120,
                hits: 1,
                error: None,
                hint: None,
            }],
            upstream_total: Some(1427),
        };

        assert!(outcome.succeeded_from(SourceKind::SerpApi));
        let mut out = json!({
            "patents": outcome.summaries(),
            "total": outcome.upstream_total.unwrap_or(0),
            "page": 2usize,
            "page_size": 10,
            "source": SourceKind::SerpApi.as_str()
        });
        if let Some(h) = outcome.hint() {
            out["hint"] = json!(h);
        }
        assert_eq!(
            json!({
                "patents": [summary],
                "total": 1427,
                "page": 2,
                "page_size": 10,
                "source": "serpapi"
            }),
            out
        );
        // hint 缺失时不得凭空多出该键（旧代码只在 Some 时插入）
        assert!(out.get("hint").is_none());
    }

    /// **MA5b 出参字面量锁（前端诊断面板的对账依据）**：`attempts` 键的 JSON 形状
    /// 逐字段冻结在此——前端 `templates/search.html` 的 `renderSearchDiagnostics` 按同一约定解析：
    /// - `source`：小写稳定串（`serpapi` / `google_patents_xhr` / `epo_ops` / `local_fts`）
    /// - `status`：serde 外部标签表示 —— 单元变体是裸字符串 `"Success"` / `"Skipped"`，
    ///   带值变体是单键对象 `{"Failed":"quota"}`（`FailKind` 为 snake_case）
    /// - 字段名：`latency_ms` / `hits` / `error` / `hint`（后两个可为 null）
    ///
    /// 改动 `AttemptStatus`/`FailKind` 的 serde 表示会红，前端必须同步。
    #[test]
    fn attempts_json_locks_frontend_panel_literals() {
        use crate::search::model::FailKind;
        let outcome = SearchOutcome {
            results: vec![],
            attempts: vec![
                AttemptReport {
                    source: SourceKind::SerpApi,
                    status: AttemptStatus::Failed(FailKind::Quota),
                    latency_ms: 812,
                    hits: 0,
                    error: Some("HTTP 429 配额耗尽".to_string()),
                    hint: Some("请配置 SerpAPI Key".to_string()),
                },
                AttemptReport {
                    source: SourceKind::GooglePatentsXhr,
                    status: AttemptStatus::Success,
                    latency_ms: 2330,
                    hits: 10,
                    error: None,
                    hint: None,
                },
                AttemptReport {
                    source: SourceKind::SerpApi,
                    status: AttemptStatus::Skipped,
                    latency_ms: 0,
                    hits: 0,
                    error: None,
                    hint: None,
                },
            ],
            upstream_total: None,
        };
        assert_eq!(
            json!([
                {
                    "source": "serpapi",
                    "status": {"Failed": "quota"},
                    "latency_ms": 812,
                    "hits": 0,
                    "error": "HTTP 429 配额耗尽",
                    "hint": "请配置 SerpAPI Key"
                },
                {
                    "source": "google_patents_xhr",
                    "status": "Success",
                    "latency_ms": 2330,
                    "hits": 10,
                    "error": null,
                    "hint": null
                },
                {
                    "source": "serpapi",
                    "status": "Skipped",
                    "latency_ms": 0,
                    "hits": 0,
                    "error": null,
                    "hint": null
                }
            ]),
            attempts_json(&outcome)
        );
    }

    /// **MA5b 总开关（只增键、不改键）**：命中/兜底/空结果三条 return 路径都追加
    /// `attempts` 键，其余既有键的集合与取值口径逐字不变。空链（理论上不可达，但
    /// 面板要能区分「没有 attempts」与「数组为空」）序列化为 `[]`。
    #[test]
    fn attempts_key_is_additive_on_all_online_paths() {
        let outcome = SearchOutcome {
            results: vec![],
            attempts: vec![],
            upstream_total: None,
        };
        assert_eq!(json!([]), attempts_json(&outcome));

        // 在线命中路径：旧五键逐字保留 + 新增 attempts（键集合显式冻结）
        let winner = json!({
            "patents": outcome.summaries(),
            "total": outcome.upstream_total.unwrap_or(0),
            "page": 1,
            "page_size": 10,
            "source": SourceKind::SerpApi.as_str(),
            "attempts": attempts_json(&outcome)
        });
        let mut keys: Vec<&str> = winner
            .as_object()
            .map(|m| m.keys().map(|k| k.as_str()).collect())
            .unwrap_or_default();
        // serde_json::Map 不保证插入序（未开 preserve_order），按键集合冻结
        keys.sort_unstable();
        assert_eq!(
            vec![
                "attempts",
                "page",
                "page_size",
                "patents",
                "source",
                "total"
            ],
            keys
        );

        // 本地兜底与空结果路径同样只增 attempts，不动 source/hint/message/google_url
        let fallback = json!({
            "patents": [], "total": 0, "page": 1, "page_size": 20,
            "google_url": "https://patents.google.com/?q=x&oq=x",
            "message": "未找到结果，可尝试在 Google Patents 上搜索",
            "attempts": attempts_json(&outcome)
        });
        assert_eq!(json!([]), fallback["attempts"]);
        assert!(fallback.get("message").is_some());
    }

    // ── MA6b：`cooldowns` 顶层出参键（诊断面板冷却可视化的结构化判据）────────────

    /// **MA6b 字面量锁**：`cooldowns` 的 JSON 形状逐字段冻结——`source` 用与 `attempts`
    /// 行同源的 `SourceKind::as_str()` 小写稳定串（前端 `DIAG_SOURCE_LABELS` 按同一键表
    /// 解析），`remaining_secs` 向上取整（296.5s → 297，与 `cooldown_skipped_report` 的
    /// error 文案共用 [`cooldown_remaining_secs`]，两处秒数永不移位）；顺序 = 原登记顺序；
    /// **空 = `None`（调用方整键省略，无冷却时旧响应逐字节不变）**。
    /// 前端判据只认这个结构化键，不用正则匹配中文 error 文案（隐式契约随文案腐烂）。
    #[test]
    fn cooldowns_json_locks_shape_order_and_ceil_secs() {
        assert_eq!(None, cooldowns_json(&[]), "空冷却必须整键省略");
        let cooled = vec![
            (SourceKind::SerpApi, Duration::from_millis(296_500)),
            (SourceKind::GooglePatentsXhr, Duration::from_millis(119_001)),
            (SourceKind::EpoOps, Duration::from_secs(900)),
        ];
        assert_eq!(
            json!([
                {"source": "serpapi", "remaining_secs": 297},
                {"source": "google_patents_xhr", "remaining_secs": 120},
                {"source": "epo_ops", "remaining_secs": 900}
            ]),
            cooldowns_json(&cooled).expect("非空冷却必须出键"),
            "形状/取整/顺序三重锁定：数组元素顺序即原登记顺序"
        );
        // 与织回记账的 error 文案同源：同一 Duration 在两处必须给出同一秒数
        let report = cooldown_skipped_report(SourceKind::SerpApi, Duration::from_millis(296_500));
        let err = report.error.expect("冷却 Skipped 必带原因");
        assert!(
            err.contains("剩余 297s"),
            "error 文案与 cooldowns 键同口径: {err}"
        );
    }

    /// **MA6b 三路径对称（在线命中档）**：命中档与兜底两档共用同一
    /// [`cooldowns_json`] helper（源码三处 `if let Some(cd) = ...`，MA5b「四个 return
    /// 分支漏一个」教训的针对性防御）；兜底两档的真实出参形状已由
    /// `all_sources_cooled_yields_empty_chain_and_identical_local_shape`（带冷却）与
    /// `local_fallback_*`（无冷却省略键）直接经真函数锁定，本用例按 MA5b
    /// `attempts_key_is_additive_on_all_online_paths` 先例复刻命中档构造并冻结键集合。
    #[test]
    fn cooldowns_key_on_online_hit_path_freezes_key_set() {
        let outcome = SearchOutcome {
            results: vec![],
            attempts: vec![],
            upstream_total: Some(3),
        };
        let cooled = vec![(SourceKind::SerpApi, Duration::from_secs(297))];
        let mut out = json!({
            "patents": outcome.summaries(),
            "total": outcome.upstream_total.unwrap_or(0),
            "page": 1,
            "page_size": 10,
            "source": SourceKind::GooglePatentsXhr.as_str(),
            "attempts": attempts_json(&outcome)
        });
        if let Some(cd) = cooldowns_json(&cooled) {
            out["cooldowns"] = cd;
        }
        let mut keys: Vec<&str> = out
            .as_object()
            .map(|m| m.keys().map(|k| k.as_str()).collect())
            .unwrap_or_default();
        keys.sort_unstable();
        assert_eq!(
            vec![
                "attempts",
                "cooldowns",
                "page",
                "page_size",
                "patents",
                "source",
                "total"
            ],
            keys,
            "命中档 = MA5b 六键 + cooldowns，不许多出或更少键"
        );
        assert_eq!(
            json!([{"source": "serpapi", "remaining_secs": 297}]),
            out["cooldowns"]
        );
    }

    // ── MA4a：本地 FTS 兜底的链上记账（attempts 末条 = local_fts）──────────────
    //
    // 直接测 [`local_fallback_json`]：内存库 + 手工构造 outcome，不经网络，形状可锁死。

    /// 测试夹具：最小可用的 `SearchRequest`（page/page_size 用端点默认语义）。
    fn fallback_req(query: &str) -> SearchRequest {
        SearchRequest {
            query: query.to_string(),
            page: 1,
            page_size: 20,
            country: None,
            date_from: None,
            date_to: None,
            search_type: None,
            sort_by: None,
            ipc: None,
            cpc: None,
            region: None,
            language: None,
            assignee: None,
            exact_assignee: false,
        }
    }

    /// 测试夹具：一条「链已跑完、三源皆墨」的 outcome（SerpAPI 未配 Key 前置 Skipped，
    /// XHR 真实失败），本地兜底应**追加**在其后而非混入在线源之间。
    fn chain_exhausted_outcome() -> SearchOutcome {
        SearchOutcome {
            results: vec![],
            attempts: vec![
                AttemptReport {
                    source: SourceKind::SerpApi,
                    status: AttemptStatus::Skipped,
                    latency_ms: 0,
                    hits: 0,
                    error: Some("未配置 SERPAPI_KEY，本轮未发起在线请求".to_string()),
                    hint: Some("未配置有效 SerpAPI Key，已自动尝试下游回退。".to_string()),
                },
                AttemptReport {
                    source: SourceKind::GooglePatentsXhr,
                    status: AttemptStatus::Failed(FailKind::Network),
                    latency_ms: 41,
                    hits: 0,
                    error: Some("google_patents_xhr 模拟断网失败".to_string()),
                    hint: None,
                },
            ],
            upstream_total: None,
        }
    }

    /// 测试夹具：可被 search_like（Mixed 路径）以「固态电池」命中的入库专利。
    fn seed_patent(db: &Database, id: &str, title: &str) {
        let p = Patent {
            id: id.to_string(),
            patent_number: format!("CN{}000000A", id),
            title: title.to_string(),
            abstract_text: format!("一种{title}及其制备方法"),
            description: String::new(),
            claims: String::new(),
            applicant: "测试研究院".to_string(),
            inventor: "张三".to_string(),
            filing_date: "2024-01-01".to_string(),
            publication_date: "2024-07-01".to_string(),
            grant_date: None,
            ipc_codes: "H01M".to_string(),
            cpc_codes: "H01M".to_string(),
            priority_date: String::new(),
            country: "CN".to_string(),
            kind_code: "A".to_string(),
            family_id: None,
            legal_status: String::new(),
            citations: "[]".to_string(),
            cited_by: "[]".to_string(),
            source: "test".to_string(),
            raw_json: "{}".to_string(),
            created_at: "2026-01-01 00:00:00".to_string(),
            images: "[]".to_string(),
            pdf_url: String::new(),
        };
        db.insert_patent(&p).expect("seed insert");
    }

    /// **MA4a 核心断言 1（本地兜底命中）**：在线链全部失败后本地库有结果时，
    /// attempts 末条必须是 `local_fts` + Success + hits>0 + latency_ms 已计时；
    /// 且出参 `source=="local"`、默认 hint 文案、`patents[]` 内容逐字不变（旧形状快照）。
    #[test]
    fn local_fallback_hit_appends_local_fts_attempt_and_keeps_legacy_shape() {
        let db = Database::init(":memory:").expect("in-memory db");
        seed_patent(&db, "111", "固态电池");
        let mut outcome = chain_exhausted_outcome();
        let out = local_fallback_json(
            &db,
            &fallback_req("固态电池"),
            &Some(SearchType::Mixed),
            None,
            &mut outcome,
            &[],
        );

        let attempts = out["attempts"].as_array().expect("attempts 是数组");
        assert_eq!(3, attempts.len(), "链上两条 + 本地兜底追加一条");
        let local = &attempts[2];
        assert_eq!(
            json!({"source": "local_fts", "status": "Success", "hits": 1, "error": null, "hint": null}),
            json!({
                "source": local["source"], "status": local["status"], "hits": local["hits"],
                "error": local["error"], "hint": local["hint"],
            }),
            "本地兜底行：local_fts + Success + hits>0，且不携带 hint（不得污染首个非空 hint 语义）"
        );
        assert!(local["latency_ms"].is_number(), "latency_ms 必须计时落账");
        // 追加在尾部而非混入在线源之间
        assert_eq!(
            vec!["serpapi", "google_patents_xhr", "local_fts"],
            attempts
                .iter()
                .map(|a| a["source"].as_str().unwrap_or("?").to_string())
                .collect::<Vec<_>>()
        );

        // 旧形状逐字锁：键集合、source、hint 文案、patents 内容
        let mut keys: Vec<&str> = out
            .as_object()
            .map(|m| m.keys().map(|k| k.as_str()).collect())
            .unwrap_or_default();
        keys.sort_unstable();
        assert_eq!(
            vec![
                "attempts",
                "hint",
                "page",
                "page_size",
                "patents",
                "source",
                "total"
            ],
            keys
        );
        assert_eq!("local", out["source"]);
        assert_eq!(
            "国外在线源暂时未返回结果，已回退本地缓存。建议配置 SerpAPI 提升命中率。",
            out["hint"]
        );
        assert_eq!(1, out["patents"].as_array().expect("patents").len());
        assert_eq!("CN111000000A", out["patents"][0]["patent_number"]);
        assert_eq!("固态电池", out["patents"][0]["title"]);
        // MA6b：无冷却时 `cooldowns` 整键省略（上面键集合冻结即证明——多出该键会直接红）
        assert!(out.get("cooldowns").is_none());
    }

    /// **MA4a 核心断言 2（本地也零命中）**：末档（`patents: []` + `google_url`）里
    /// 仍要能看出「本地兜底跑过、命中 0 条」——如实记 Success + hits=0，禁止伪造。
    #[test]
    fn local_fallback_zero_hits_records_the_run_on_tail_path() {
        let db = Database::init(":memory:").expect("in-memory db");
        seed_patent(&db, "111", "固态电池");
        let mut outcome = chain_exhausted_outcome();
        let out = local_fallback_json(
            &db,
            &fallback_req("不存在的检索词xyz"),
            &Some(SearchType::Mixed),
            Some("在线预算提示".to_string()),
            &mut outcome,
            &[],
        );

        let attempts = out["attempts"].as_array().expect("attempts");
        assert_eq!(3, attempts.len());
        let local = &attempts[2];
        assert_eq!("local_fts", local["source"]);
        assert_eq!("Success", local["status"], "跑过但零命中仍是 Success");
        assert_eq!(0, local["hits"]);
        assert!(local["error"].is_null());

        // 末档旧形状：google_url/message 逐字不变，hint 沿用上游文案（非默认兜底文案）
        assert_eq!(json!([]), out["patents"]);
        assert_eq!(0, out["total"]);
        assert_eq!(
            "https://patents.google.com/?q=%E4%B8%8D%E5%AD%98%E5%9C%A8%E7%9A%84%E6%A3%80%E7%B4%A2%E8%AF%8Dxyz&oq=%E4%B8%8D%E5%AD%98%E5%9C%A8%E7%9A%84%E6%A3%80%E7%B4%A2%E8%AF%8Dxyz",
            out["google_url"]
        );
        assert_eq!("未找到结果，可尝试在 Google Patents 上搜索", out["message"]);
        assert_eq!("在线预算提示", out["hint"]);
        // MA6b：空结果末档同样「无冷却 ⇒ 整键省略」
        assert!(out.get("cooldowns").is_none());
    }

    /// **MA4a 核心断言 3（本地查询失败）**：`search_smart` 报错时记
    /// Failed(Parse)（分类理由见 [`local_fallback_json`] 注释，不扩枚举），
    /// error 非空且点名来源（风格对齐 chain case5），出参仍走末档旧形状。
    #[test]
    fn local_fallback_db_error_records_failed_parse_attempt() {
        let db = Database::init(":memory:").expect("in-memory db");
        seed_patent(&db, "111", "固态电池");
        // 制造真实查询故障：删掉内容表，search_like 的 prepare 必然报 no such table。
        db.conn()
            .execute("DROP TABLE patents", [])
            .expect("drop for failure fixture");
        let mut outcome = chain_exhausted_outcome();
        let out = local_fallback_json(
            &db,
            &fallback_req("固态电池"),
            &Some(SearchType::Mixed),
            None,
            &mut outcome,
            &[],
        );

        let attempts = out["attempts"].as_array().expect("attempts");
        assert_eq!(3, attempts.len());
        let local = &attempts[2];
        assert_eq!("local_fts", local["source"]);
        assert_eq!(
            json!({"Failed": "parse"}),
            local["status"],
            "本地库故障复用 Parse（记 bug 不降级），字面量受 MA5b 锁约束"
        );
        assert_eq!(0, local["hits"]);
        let err = local["error"].as_str().expect("失败必须带非空 error");
        assert!(err.contains("local_fts"), "error 要点名来源: {err}");
        // 失败即无兜底结果，落末档形状
        assert_eq!(json!([]), out["patents"]);
        assert!(
            out.get("hint").is_none(),
            "upstream_hint 为 None 时末档不凭空造 hint"
        );
        assert!(out.get("google_url").is_some());
    }

    /// 夹具：指定申请人的入库专利（MA4b 精确通道用例用；字段形状与 [`seed_patent`] 一致）。
    fn seed_patent_applicant(db: &Database, id: &str, title: &str, applicant: &str) {
        let p = Patent {
            id: id.to_string(),
            patent_number: format!("CN{}000000A", id),
            title: title.to_string(),
            abstract_text: format!("一种{title}及其制备方法"),
            description: String::new(),
            claims: String::new(),
            applicant: applicant.to_string(),
            inventor: "李四".to_string(),
            filing_date: "2024-01-01".to_string(),
            publication_date: "2024-07-01".to_string(),
            grant_date: None,
            ipc_codes: "H01M".to_string(),
            cpc_codes: "H01M".to_string(),
            priority_date: String::new(),
            country: "CN".to_string(),
            kind_code: "A".to_string(),
            family_id: None,
            legal_status: String::new(),
            citations: "[]".to_string(),
            cited_by: "[]".to_string(),
            source: "test".to_string(),
            raw_json: "{}".to_string(),
            created_at: "2026-01-01 00:00:00".to_string(),
            images: "[]".to_string(),
            pdf_url: String::new(),
        };
        db.insert_patent(&p).expect("seed insert");
    }

    /// **MA4b 核心断言（路由级透传）**：`exact_assignee=true` 时本地兜底必须走等值通道——
    /// 「张三」不得带回「张三丰」名下专利；同一请求 `exact_assignee=false` 时两条款全回
    /// （旧 LIKE 行为逐字不变）。attempts 的 local_fts 行如实反映 hits 差异。
    #[test]
    fn local_fallback_exact_assignee_switches_the_equal_channel() {
        let db = Database::init(":memory:").expect("in-memory db");
        seed_patent_applicant(&db, "zs1", "固态电池正极材料", "张三");
        seed_patent_applicant(&db, "zsf1", "固态电池隔膜", "张三丰");

        // exact=true：只回等值行
        let mut exact = fallback_req("张三");
        exact.exact_assignee = true;
        let mut outcome = chain_exhausted_outcome();
        let out = local_fallback_json(
            &db,
            &exact,
            &Some(SearchType::Applicant),
            None,
            &mut outcome,
            &[],
        );
        let patents = out["patents"].as_array().expect("patents");
        assert_eq!(1, patents.len(), "精确通道不得命中张三丰: {out}");
        assert_eq!("张三", out["patents"][0]["applicant"]);
        assert_eq!("CNzs1000000A", out["patents"][0]["patent_number"]);
        assert_eq!(
            1, out["attempts"][2]["hits"],
            "local_fts 记账必须如实反映精确通道 hits"
        );

        // exact=false（缺省形状）：旧模糊行为不变，两条都回
        let fuzzy = fallback_req("张三");
        let mut outcome2 = chain_exhausted_outcome();
        let out2 = local_fallback_json(
            &db,
            &fuzzy,
            &Some(SearchType::Applicant),
            None,
            &mut outcome2,
            &[],
        );
        assert_eq!(2, out2["patents"].as_array().expect("patents").len());
    }
}
/// # issue #101 — 逐项命中判定：该条在哪些列上含任一查询 token。
/// 定向通道（inventor/applicant/patent_number）恒返回该列（通道语义即命中面）；
/// Keyword/Mixed 按小写包含逐列真算——只命中 patent_number 的条只返回
/// ["patent_number"]，证明它不是把列清单原样贴上去（负控语义）。
pub(super) fn item_matched_fields(
    p: &crate::types::search::PatentSummary,
    tokens: &[String],
    detected: &SearchType,
) -> Option<Vec<String>> {
    let fields: [(&str, &String); 5] = [
        ("title", &p.title),
        ("abstract_text", &p.abstract_text),
        ("applicant", &p.applicant),
        ("inventor", &p.inventor),
        ("patent_number", &p.patent_number),
    ];
    match detected {
        SearchType::Inventor => Some(vec!["inventor".to_string()]),
        SearchType::Applicant => Some(vec!["applicant".to_string()]),
        SearchType::PatentNumber => Some(vec!["patent_number".to_string()]),
        SearchType::Keyword | SearchType::Mixed => {
            let hit: Vec<String> = fields
                .iter()
                .filter(|(_, v)| tokens.iter().any(|t| v.to_lowercase().contains(t)))
                .map(|(n, _)| (*n).to_string())
                .collect();
            if hit.is_empty() {
                None
            } else {
                Some(hit)
            }
        }
    }
}

#[cfg(test)]
mod item_matched_fields_tests {
    use super::*;
    use crate::types::search::PatentSummary;

    fn summary(title: &str, applicant: &str, inventor: &str, pn: &str) -> PatentSummary {
        PatentSummary {
            id: "t1".into(),
            patent_number: pn.into(),
            title: title.into(),
            abstract_text: String::new(),
            applicant: applicant.into(),
            inventor: inventor.into(),
            filing_date: String::new(),
            country: "CN".into(),
            relevance_score: None,
            score_source: None,
            matched_fields: None,
        }
    }

    #[test]
    fn per_item_fields_only_include_actually_hit_columns() {
        // 锁的语义（#101 负控核心）：只在 title 命中的条，逐项 matched_fields
        // 只含 title——证明它是逐条真算的，不是把全局列清单贴上去。
        let p = summary("固态电池电解质改进", "某新能源公司", "李四", "CN100000001A");
        let got = item_matched_fields(&p, &["固态电池".to_string()], &SearchType::Mixed).unwrap();
        assert_eq!(got, vec!["title".to_string()], "其余列不含该词，不得出现");
    }

    #[test]
    fn per_item_fields_multi_token_cross_column_union() {
        // 多 token 跨列命中：任一列含任一 token 即计入该列（OR 语义与 #85 排序一致）。
        let p = summary("隔热罩", "某电池公司", "李四", "CN100000001A");
        let got = item_matched_fields(
            &p,
            &["隔热罩".to_string(), "电池".to_string()],
            &SearchType::Mixed,
        )
        .unwrap();
        assert!(got.contains(&"title".to_string()));
        assert!(got.contains(&"applicant".to_string()));
        assert!(!got.contains(&"inventor".to_string()), "李四不含任一 token");
    }

    #[test]
    fn per_item_fields_inventor_channel_is_pinned() {
        // 定向通道：命中面即通道列（即使 title 恰好也含词）。
        let p = summary("李四的电池研究", "某公司", "李四", "CN100000001A");
        let got = item_matched_fields(&p, &["李四".to_string()], &SearchType::Inventor).unwrap();
        assert_eq!(got, vec!["inventor".to_string()]);
    }
}
