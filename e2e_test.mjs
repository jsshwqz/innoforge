import puppeteer from 'puppeteer';
import { existsSync } from 'node:fs';

const baseUrl = (process.env.INNOFORGE_E2E_BASE_URL || 'http://127.0.0.1:3000').replace(/\/$/, '');
const expectedPasses = 60;
const failures = [];
let passed = 0;

function pass(name) {
    passed += 1;
    console.log(`PASS ${passed}: ${name}`);
}

function fail(name, detail) {
    failures.push(`${name}: ${detail}`);
}

function requireCondition(condition, name, detail) {
    if (condition) {
        pass(name);
    } else {
        fail(name, detail);
    }
}

function formatHttp(response) {
    return `page=${response.url()} status=${response.status()}`;
}

function findBrowserExecutable() {
    const candidates = [
        process.env.PUPPETEER_EXECUTABLE_PATH,
        'C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe',
        'C:\\Program Files (x86)\\Google\\Chrome\\Application\\chrome.exe',
        'C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe',
    ].filter(Boolean);
    return candidates.find(existsSync);
}

async function replaceInput(page, selector, value) {
    await page.click(selector, { clickCount: 3 });
    await page.keyboard.press('Backspace');
    await page.type(selector, value);
    return page.$eval(selector, (input, expectedValue) => input.value === expectedValue, value);
}

const pageMatrix = [
    {
        path: '/',
        name: 'Home',
        selector: '#idea-input',
        interaction: async page => {
            await page.click('#mode-quick');
            return page.$eval('#mode-quick', button => button.classList.contains('active'));
        },
        interactionName: 'Home mode switch renders locally',
    },
    {
        path: '/search',
        name: 'Search',
        selector: '#search-input',
        interaction: page => replaceInput(page, '#search-input', 'E2E safe query'),
        interactionName: 'Search query accepts local input',
    },
    {
        path: '/patent/1',
        name: 'Patent detail',
        selector: '#tab-abstract',
        interaction: async page => page.evaluate(() => {
            const button = [...document.querySelectorAll('button.tab')]
                .find(candidate => (candidate.getAttribute('onclick') || '').includes("showTab('claims'"));
            if (!button) return false;
            button.click();
            return document.querySelector('#tab-claims')?.classList.contains('active') || false;
        }),
        interactionName: 'Patent detail claim tab switches locally',
    },
    {
        path: '/idea',
        name: 'Idea',
        selector: '#idea-form',
        interaction: page => page.evaluate(() => {
            if (typeof window.switchTab !== 'function') return false;
            window.switchTab('evidence');
            return document.querySelector('#tab-evidence')?.classList.contains('active') || false;
        }),
        interactionName: 'Idea evidence tab switches locally',
    },
    {
        path: '/ai',
        name: 'AI chat',
        selector: '#chat-msg',
        interaction: page => replaceInput(page, '#chat-msg', 'E2E draft only'),
        interactionName: 'AI chat input accepts local draft without sending',
    },
    {
        path: '/compare',
        name: 'Compare',
        selector: '#my-patent',
        interaction: async page => {
            await page.click('#btn-add-ref');
            return page.$eval('#ref-list', list => list.querySelectorAll('.ref-row').length > 0);
        },
        interactionName: 'Compare adds a local reference row',
    },
    {
        path: '/settings',
        name: 'Settings',
        selector: '#ai-api-key',
        interaction: page => page.$eval('#ai-api-key', input => {
            input.value = 'E2E_NOT_A_REAL_KEY';
            input.dispatchEvent(new Event('input', { bubbles: true }));
            return input.value === 'E2E_NOT_A_REAL_KEY';
        }),
        interactionName: 'Settings accepts an unsaved local key draft',
    },
    {
        path: '/oa-response',
        name: 'OA response',
        selector: '#claims-editor',
        interaction: page => page.$eval('#depth-select', select => {
            const alternative = [...select.options].find(option => option.value !== select.value);
            if (!alternative) return false;
            select.value = alternative.value;
            select.dispatchEvent(new Event('change', { bubbles: true }));
            return select.value === alternative.value;
        }),
        interactionName: 'OA depth control accepts a local selection',
    },
];

async function openPage(browser, specification, pageErrors, requestFailures) {
    const page = await browser.newPage();
    const targetUrl = `${baseUrl}${specification.path}`;
    page.on('pageerror', error => {
        pageErrors.push(`page=${page.url() || targetUrl} error=${error.message}`);
    });
    page.on('console', message => {
        if (message.type() === 'error') {
            pageErrors.push(`page=${page.url() || targetUrl} console_error=${message.text()}`);
        }
    });
    page.on('requestfailed', request => {
        const failure = request.failure();
        requestFailures.push(
            `page=${page.url() || targetUrl} request=${request.url()} error=${failure ? failure.errorText : 'unknown error'}`,
        );
    });
    let response = null;
    let navigationError = null;
    try {
        response = await page.goto(targetUrl, {
            waitUntil: 'domcontentloaded',
            timeout: 20_000,
        });
        await new Promise(resolve => setTimeout(resolve, 500));
    } catch (error) {
        navigationError = error.message;
    }
    return { page, response, targetUrl, navigationError };
}

async function runPageMatrix(browser, pageErrors, requestFailures) {
    const openedPages = new Map();

    for (const specification of pageMatrix) {
        const errorStart = pageErrors.length;
        const requestFailureStart = requestFailures.length;
        const opened = await openPage(browser, specification, pageErrors, requestFailures);
        openedPages.set(specification.path, opened.page);

        const patentDetailIsNotFound = specification.path === '/patent/1'
            && opened.response
            && opened.response.status() === 404;

        if (patentDetailIsNotFound) {
            requireCondition(
                opened.response.status() === 404,
                'Patent detail reports an empty local library with HTTP 404',
                formatHttp(opened.response),
            );
            const pageText = await opened.page.$eval('body', body => body.textContent || '');
            requireCondition(
                /Patent not found|专利未找到/.test(pageText),
                'Patent detail shows the not-found prompt for an empty local library',
                `page=${opened.page.url()} expected_prompt="Patent not found"`,
            );
        } else {
            requireCondition(
                opened.response && opened.response.ok(),
                `${specification.name} HTTP is reachable`,
                opened.response
                    ? formatHttp(opened.response)
                    : `page=${opened.targetUrl} status=no_response navigation_error=${opened.navigationError || 'unknown'}`,
            );
            requireCondition(
                await opened.page.$(specification.selector),
                `${specification.name} critical root exists`,
                `page=${opened.page.url()} missing_selector=${specification.selector}`,
            );
        }
        const unexpectedPageErrors = pageErrors.slice(errorStart).filter(error => !(
            patentDetailIsNotFound
            && error.includes('console_error=Failed to load resource: the server responded with a status of 404 (Not Found)')
        ));
        requireCondition(
            unexpectedPageErrors.length === 0,
            `${specification.name} has no browser errors`,
            unexpectedPageErrors.join('\n') || `page=${opened.page.url()} browser_error=unknown`,
        );
        requireCondition(
            requestFailures.length === requestFailureStart,
            `${specification.name} has no failed requests`,
            requestFailures.slice(requestFailureStart).join('\n') || `page=${opened.page.url()} failed_request=unknown`,
        );

        if (patentDetailIsNotFound) {
            requireCondition(
                true,
                'Patent detail interaction is skipped when no local patent exists',
                `page=${opened.page.url()} status=404`,
            );
        } else {
            try {
                requireCondition(
                    await specification.interaction(opened.page),
                    specification.interactionName,
                    `page=${opened.page.url()} interaction_result=unexpected`,
                );
            } catch (error) {
                fail(specification.interactionName, `page=${opened.page.url()} interaction_error=${error.message}`);
            }
        }
    }

    return openedPages;
}

async function checkAmendmentEndpoint(oaPage) {
    const validation = await oaPage.evaluate(async () => {
        const response = await fetch('/api/ai/check-amendments', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({}),
        });
        return { status: response.status, body: await response.json() };
    });
    requireCondition(
        validation.status >= 200 && validation.status < 500 && typeof validation.body.error === 'string',
        'Amendment check validates a malformed request without AI',
        `request=${baseUrl}/api/ai/check-amendments status=${validation.status} body=${JSON.stringify(validation.body)}`,
    );
}

async function checkLongPayloadIntegrity(oaPage) {
    let capturedPayload = null;
    await oaPage.setRequestInterception(true);
    oaPage.on('request', request => {
        if (request.url() === `${baseUrl}/api/ai/check-amendments` && request.method() === 'POST') {
            capturedPayload = request.postData();
            request.respond({
                status: 200,
                contentType: 'application/json; charset=utf-8',
                body: JSON.stringify({ error: 'E2E mock: provider was not called' }),
            }).catch(error => fail('Long OA request is intercepted', `request=${request.url()} error=${error.message}`));
            return;
        }
        request.continue().catch(error => fail('Browser request continues', `request=${request.url()} error=${error.message}`));
    });

    const tailMarker = 'INNOFORGE_E2E_LONG_TEXT_TAIL_9f06b8';
    await oaPage.evaluate(marker => {
        const originalClaims = `Original claims\n${'technical feature,'.repeat(12_000)}${marker}`;
        window.uploadedData.my = { title: 'E2E patent', content: originalClaims };
        window.uploadedData.oa = { title: 'E2E OA', content: `Office action\n${marker}` };
        document.getElementById('claims-editor').value = `Amended claims\n${marker}`;
        return window.checkAmendments();
    }, tailMarker);

    let payload;
    try {
        payload = capturedPayload ? JSON.parse(capturedPayload) : null;
    } catch (error) {
        fail('Long OA request payload is JSON', `request=${baseUrl}/api/ai/check-amendments error=${error.message}`);
    }
    requireCondition(
        payload
            && payload.original_claims.includes(tailMarker)
            && payload.amended_claims.includes(tailMarker)
            && payload.office_action.includes(tailMarker),
        'Long OA payload preserves all tail markers without AI',
        capturedPayload
            ? `request=${baseUrl}/api/ai/check-amendments tail_marker=missing`
            : `request=${baseUrl}/api/ai/check-amendments payload=not_captured`,
    );
}

async function checkDiscussionTranscriptExport(oaPage) {
    const aiRequests = [];
    const trackAiRequest = request => {
        if (request.url().includes('/api/ai/')) aiRequests.push(request.url());
    };
    oaPage.on('request', trackAiRequest);

    try {
        const prepared = await oaPage.evaluate(() => {
            const markers = {
                context: 'INNOFORGE_E2E_TRANSCRIPT_CONTEXT_TAIL_5d8a11',
                user: 'INNOFORGE_E2E_TRANSCRIPT_USER_TAIL_5d8a11',
                assistant: 'INNOFORGE_E2E_TRANSCRIPT_ASSISTANT_TAIL_5d8a11',
            };
            const timestamps = [
                '2026-07-13T01:02:03.000Z',
                '2026-07-13T01:02:04.000Z',
                '2026-07-13T01:02:05.000Z',
            ];
            const codeFence = '```original markdown source```';
            const state = {
                originalCreateObjectURL: URL.createObjectURL,
                originalRevokeObjectURL: URL.revokeObjectURL,
                originalAnchorClick: HTMLAnchorElement.prototype.click,
                blob: null,
                filename: null,
            };
            window.__innoforgeE2eTranscriptState = state;
            URL.createObjectURL = blob => {
                state.blob = blob;
                return 'blob:innoforge-e2e-transcript';
            };
            URL.revokeObjectURL = () => {};
            HTMLAnchorElement.prototype.click = function() {
                state.filename = this.download;
            };
            discussionHistory = [
                ['system', `Initial context\n${markers.context}\n${codeFence}`, timestamps[0]],
                ['user', `User question\n${markers.user}`, timestamps[1]],
                ['assistant', `Assistant answer\n${markers.assistant}`, timestamps[2]],
            ];
            document.getElementById('discussion-panel').classList.remove('hidden');
            showDiscussionExportButton();
            return {
                markers,
                timestamps,
                codeFence,
                labels: [
                    discussionTranscriptRoleLabel('system'),
                    discussionTranscriptRoleLabel('user'),
                    discussionTranscriptRoleLabel('assistant'),
                ],
                notice: t('oar.transcriptNotice'),
                isVisible: document.getElementById('discussion-transcript-export-btn').style.display !== 'none',
            };
        });

        requireCondition(
            prepared.isVisible,
            'Full discussion export becomes visible after a discussion exchange',
            'page=/oa-response selector=#discussion-transcript-export-btn visibility=hidden',
        );
        await oaPage.click('#discussion-transcript-export-btn');
        const exported = await oaPage.evaluate(async () => {
            const state = window.__innoforgeE2eTranscriptState;
            const result = {
                filename: state.filename,
                content: state.blob ? await state.blob.text() : '',
            };
            URL.createObjectURL = state.originalCreateObjectURL;
            URL.revokeObjectURL = state.originalRevokeObjectURL;
            HTMLAnchorElement.prototype.click = state.originalAnchorClick;
            delete window.__innoforgeE2eTranscriptState;
            return result;
        });

        requireCondition(
            typeof exported.filename === 'string' && exported.filename.endsWith('.md'),
            'Full discussion export uses a Markdown filename',
            `page=/oa-response filename=${exported.filename || 'missing'}`,
        );
        requireCondition(
            exported.content.includes(prepared.markers.context)
                && exported.content.includes(prepared.markers.user)
                && exported.content.includes(prepared.markers.assistant),
            'Full discussion export preserves context and every message tail marker',
            'page=/oa-response transcript_tail_marker=missing',
        );
        requireCondition(
            exported.content.includes(prepared.codeFence),
            'Full discussion export preserves original Markdown backticks',
            'page=/oa-response transcript_backticks=missing',
        );
        requireCondition(
            prepared.timestamps.every(timestamp => exported.content.includes(timestamp))
                && prepared.labels.every(label => exported.content.includes(label))
                && exported.content.includes(prepared.notice),
            'Full discussion export includes roles, timestamps, and original-record notice',
            'page=/oa-response transcript_metadata=missing',
        );
        requireCondition(
            aiRequests.length === 0,
            'Full discussion export makes no AI request',
            `page=/oa-response ai_requests=${aiRequests.join(',') || 'none'}`,
        );
    } finally {
        oaPage.off('request', trackAiRequest);
        await oaPage.evaluate(() => {
            const state = window.__innoforgeE2eTranscriptState;
            if (!state) return;
            URL.createObjectURL = state.originalCreateObjectURL;
            URL.revokeObjectURL = state.originalRevokeObjectURL;
            HTMLAnchorElement.prototype.click = state.originalAnchorClick;
            delete window.__innoforgeE2eTranscriptState;
        });
    }
}

async function checkCadControllerStateIsolation(ideaPage) {
    const result = await ideaPage.evaluate(async () => {
        const host = document.createElement('div');
        host.id = 'innoforge-e2e-cad-host';
        document.body.appendChild(host);
        const requests = [];
        const originalFetch = window.fetch;
        let drawAttempt = 0;
        let contextId = 'cad-context-original';
        let history = 'cad-history-original';
        const artifacts = [
            {
                id: 'cad-history-2', revision: 2, assumptions: [],
                validation: { valid: true }, created_at: '2026-08-12T00:00:02Z',
                step_rel_path: 'two.step',
            },
            {
                id: 'cad-history-1', revision: 1, assumptions: [],
                validation: { valid: true }, created_at: '2026-08-12T00:00:01Z',
                step_rel_path: null,
            },
        ];

        window.fetch = async (url, options = {}) => {
            if (String(url).startsWith('/api/cad/artifacts?')) {
                return new Response(JSON.stringify({ artifacts }), {
                    status: 200,
                    headers: { 'Content-Type': 'application/json' },
                });
            }
            if (url === '/api/cad/draw') {
                const body = JSON.parse(options.body);
                requests.push(body);
                drawAttempt += 1;
                if (drawAttempt === 1) {
                    return new Response(JSON.stringify({ code: 'draw_failed', error: 'mock failure' }), {
                        status: 503,
                        headers: { 'Content-Type': 'application/json' },
                    });
                }
                const artifact = {
                    id: `cad-created-${drawAttempt}`,
                    revision: drawAttempt,
                    assumptions: [],
                    validation: { valid: true },
                    created_at: '2026-08-12T00:00:03Z',
                    step_rel_path: null,
                };
                return new Response(JSON.stringify({ artifact, warnings: [] }), {
                    status: 200,
                    headers: { 'Content-Type': 'application/json' },
                });
            }
            return originalFetch(url, options);
        };

        try {
            const controller = window.InnoForgeCad.createController({
                context: () => ({ kind: 'idea', id: contextId }),
                messages: host,
                history: () => history,
            });
            await controller.draw('failed original drawing');
            const retryButton = host.querySelector('.cad-card-degraded button');
            await controller.draw('new independent drawing');
            contextId = 'cad-context-changed';
            history = 'cad-history-changed';
            retryButton.click();
            for (let attempt = 0; attempt < 20 && requests.length < 3; attempt += 1) {
                await new Promise(resolve => setTimeout(resolve, 10));
            }
            const retried = requests[2] || {};
            const retryIsIsolated = retried.prompt === 'failed original drawing'
                && retried.parent_artifact_id === null
                && retried.context_id === 'cad-context-original'
                && retried.conversation_context === 'cad-history-original';

            await controller.restore();
            host.replaceChildren();
            controller.renderHistory();
            const renderedIds = [...host.querySelectorAll('[data-cad-artifact-id]')]
                .map(card => card.dataset.cadArtifactId);
            return {
                retryIsIsolated,
                historyRestored: renderedIds.join(',') === 'cad-history-1,cad-history-2',
                requestCount: requests.length,
            };
        } finally {
            window.fetch = originalFetch;
            host.remove();
        }
    });

    requireCondition(
        result.retryIsIsolated,
        'CAD retry preserves the failed request prompt, context, history, and parent',
        `page=/idea request_count=${result.requestCount} retry_snapshot=changed`,
    );
    requireCondition(
        result.historyRestored,
        'CAD history can be rendered again after chat rebuilds its message container',
        'page=/idea CAD history order or cards were lost after container rebuild',
    );
}

async function checkSettingsDoesNotAutoStartFreeCad(settingsPage) {
    const startRequests = [];
    const observer = request => {
        if (request.url() === `${baseUrl}/api/cad/start`) startRequests.push(request.method());
    };
    settingsPage.on('request', observer);
    try {
        await settingsPage.reload({ waitUntil: 'domcontentloaded', timeout: 20_000 });
        await new Promise(resolve => setTimeout(resolve, 1_000));
        requireCondition(
            startRequests.length === 0,
            'Settings page checks FreeCAD status without starting it automatically',
            `page=/settings start_requests=${startRequests.join(',') || 'none'}`,
        );
    } finally {
        settingsPage.off('request', observer);
    }
}

// 创意页功能完整性：防止"按钮在、函数没了"的静默丢失（历史事故 42c726e / 9f1a14b）
async function checkIdeaPageFunctionIntegrity(ideaPage) {
    const result = await ideaPage.evaluate(() => {
        const requiredFunctions = [
            // 核心交互
            'sendChatMessage', 'stopIdeaGeneration', 'handleChatFileSelect',
            'loadMessages', 'scrollIdeaChatToBottom', 'deleteIdea',
            // 工具函数（曾被误删）
            'showStatus', 'clearResults', 'clearForm', 'showNewIdeaForm',
            'renderMarkdown', 'showDiscussionPanel',
            // 报告功能（曾被误删）
            'openReport', 'switchReportType', 'loadReportTab', 'openReportInNewTab',
            // 讨论导出（曾被误删）
            'exportConclusions', 'summarizeDiscussion',
            // 标签页数据加载（曾被遗漏）
            'loadEvidence', 'loadClaimTree', 'loadFindings', 'loadFeatureCards',
            'loadVersionHistory', 'loadMemory', 'renderOverview',
            // 附件（恢复自 v0.7.2）
            'renderChatAttachments',
        ];
        const missing = requiredFunctions.filter(name => typeof window[name] !== 'function');

        // 删除按钮：历史列表条目应有 ✕
        const historyRows = document.querySelectorAll('.idea-history-item');
        let deleteButtons = 0;
        historyRows.forEach(row => {
            if (row.textContent.includes('✕')) deleteButtons += 1;
        });

        // 滚动按钮存在
        const scrollBtn = !!document.getElementById('scroll-bottom-btn-idea');

        return {
            missing,
            totalHistoryRows: historyRows.length,
            deleteButtons,
            scrollBtn,
            deleteBtnAll: historyRows.length === 0 || deleteButtons === historyRows.length,
        };
    });

    requireCondition(
        result.missing.length === 0,
        'Idea page keeps all critical functions defined',
        `page=/idea missing=${result.missing.join(',') || 'none'}`,
    );
    requireCondition(
        result.scrollBtn,
        'Idea page keeps the scroll-to-bottom button in the chat panel',
        'page=/idea selector=#scroll-bottom-btn-idea missing=true',
    );
    requireCondition(
        result.deleteBtnAll,
        'Idea history entries keep a delete button',
        `page=/idea rows=${result.totalHistoryRows} with_delete=${result.deleteButtons}`,
    );
}

async function checkSearchLanguageFilter(searchPage) {
    // MA3 语言过滤器：#language-filter 存在且默认「自动」（空值），不选时 buildRequest 不下发 language 键
    const defaults = await searchPage.evaluate(() => {
        const select = document.getElementById('language-filter');
        if (!select || typeof window.buildRequest !== 'function') return { exists: false };
        const options = [...select.options].map(option => option.value);
        const body = buildRequest(1);
        return {
            exists: true,
            value: select.value,
            options,
            hasLanguageKey: Object.prototype.hasOwnProperty.call(body, 'language'),
        };
    });
    requireCondition(
        defaults.exists
            && defaults.value === ''
            && defaults.options.join('|') === '|chinese|english|all'
            && !defaults.hasLanguageKey,
        'Search language filter defaults to auto and omits language key',
        `page=/search exists=${defaults.exists} value=${defaults.value} options=${defaults.options && defaults.options.join(',')} language_key=${defaults.hasLanguageKey}`,
    );

    // 显式选择 chinese 后 buildRequest 返回值必须含 language: "chinese"；检查后恢复默认值，避免污染其它用例
    const explicit = await searchPage.evaluate(() => {
        const select = document.getElementById('language-filter');
        if (!select || typeof window.buildRequest !== 'function') return { exists: false };
        select.value = 'chinese';
        select.dispatchEvent(new Event('change', { bubbles: true }));
        const body = buildRequest(1);
        const language = body.language;
        select.value = '';
        select.dispatchEvent(new Event('change', { bubbles: true }));
        return { exists: true, language, restored: !('language' in buildRequest(1)) };
    });
    requireCondition(
        explicit.exists
            && explicit.language === 'chinese'
            && explicit.restored,
        'Search language filter sends explicit chinese selection',
        `page=/search exists=${explicit.exists} language=${explicit.language} restored=${explicit.restored}`,
    );
}

// MA6b：检索诊断面板（此前 e2e 零覆盖）——冷却徽标、本地模式回归、结构化判据、截断纪律。
// 直接调用页内 renderSearchDiagnostics(假 payload) 断言 DOM，无需真出网、无需真 Key。
async function checkSearchDiagnosticsAndCooldown(searchPage) {
    // ① cooldowns 键命中 → 出现含剩余秒数的冷却徽标，面板可见
    const cooldown = await searchPage.evaluate(() => {
        renderSearchDiagnostics({
            attempts: [
                {
                    source: 'serpapi',
                    status: 'Skipped',
                    latency_ms: 0,
                    hits: 0,
                    error: 'serpapi 上游熔断冷却中，剩余 297s（本次不再发起请求）',
                    hint: null,
                },
                {
                    source: 'google_patents_xhr',
                    status: 'Success',
                    latency_ms: 450,
                    hits: 5,
                    error: null,
                    hint: null,
                },
            ],
            cooldowns: [{ source: 'serpapi', remaining_secs: 297 }],
        });
        const box = document.getElementById('search-diagnostics');
        const badges = [...box.querySelectorAll('span')].filter(s => s.style.borderRadius === '10px');
        const secondsBadge = badges.find(s => s.textContent.includes('297'));
        return { visible: box.style.display === 'block', hasSecondsBadge: !!secondsBadge };
    });
    requireCondition(
        cooldown.visible && cooldown.hasSecondsBadge,
        'Search diagnostics renders cooldown badge with remaining seconds from structured cooldowns key',
        `page=/search visible=${cooldown.visible} seconds_badge=${cooldown.hasSecondsBadge}`,
    );

    // ② 本地模式回归：无 attempts / attempts 空数组 → 面板不渲染、清空内容、不抛错
    const localMode = await searchPage.evaluate(() => {
        let threw = false;
        try {
            renderSearchDiagnostics({ patents: [], total: 0, source: 'local' });
            renderSearchDiagnostics({ attempts: [] });
        } catch (error) {
            threw = true;
        }
        const box = document.getElementById('search-diagnostics');
        return { threw, hidden: box.style.display === 'none', empty: box.textContent === '' };
    });
    requireCondition(
        !localMode.threw && localMode.hidden && localMode.empty,
        'Search diagnostics stays hidden and throws nothing without attempts (local mode)',
        `page=/search threw=${localMode.threw} hidden=${localMode.hidden} empty=${localMode.empty}`,
    );

    // ③ 判据是结构化的：error 文案里伪装「冷却中 剩余 480s」字样、但无 cooldowns 键，
    //    不得长出带秒数的冷却徽标（隐式文案匹配被 MA6b 明确禁止）
    const structural = await searchPage.evaluate(() => {
        renderSearchDiagnostics({
            attempts: [
                {
                    source: 'local_fts',
                    status: 'Success',
                    latency_ms: 3,
                    hits: 1,
                    error: '字样伪装：冷却中 剩余 480s',
                    hint: null,
                },
            ],
        });
        const box = document.getElementById('search-diagnostics');
        const badges = [...box.querySelectorAll('span')].filter(s => s.style.borderRadius === '10px');
        const fakeBadge = badges.find(s => s.textContent.includes('480'));
        return { visible: box.style.display === 'block', hasFakeBadge: !!fakeBadge };
    });
    requireCondition(
        structural.visible && !structural.hasFakeBadge,
        'Search diagnostics ignores error-text wording for cooldown badges (structured key only)',
        `page=/search visible=${structural.visible} fake_badge=${structural.hasFakeBadge}`,
    );

    // ④ 截断纪律（AGENTS.md 2.5）：长 error 显示截断但 title 保留全文
    const truncation = await searchPage.evaluate(() => {
        const long = 'x'.repeat(80);
        renderSearchDiagnostics({
            attempts: [
                {
                    source: 'epo_ops',
                    status: { Failed: 'network' },
                    latency_ms: 2000,
                    hits: 0,
                    error: long,
                    hint: null,
                },
            ],
        });
        const box = document.getElementById('search-diagnostics');
        const errSpan = [...box.querySelectorAll('span')].find(s => s.getAttribute('title') === long);
        return {
            found: !!errSpan,
            truncated: !!errSpan && errSpan.textContent.length < long.length,
            ellipsis: !!errSpan && errSpan.textContent.includes('…'),
        };
    });
    requireCondition(
        truncation.found && truncation.truncated && truncation.ellipsis,
        'Search diagnostics truncates long errors for display but keeps full text in title',
        `page=/search found=${truncation.found} truncated=${truncation.truncated} ellipsis=${truncation.ellipsis}`,
    );

    // 复位：不让假数据面板残留在 DOM 里影响其它用例
    await searchPage.evaluate(() => renderSearchDiagnostics({}));
}

async function main() {
    const pageErrors = [];
    const requestFailures = [];
    let browser;
    let openedPages = new Map();

    try {
        const executablePath = findBrowserExecutable();
        // --no-sandbox 仅 Linux（CI 容器以 root 运行常需）；--disable-dev-shm-usage 全平台无害，防 shm 不足。
        const isLinux = process.platform === 'linux';
        browser = await puppeteer.launch({
            headless: true,
            ...(executablePath ? { executablePath } : {}),
            args: ['--disable-dev-shm-usage', ...(isLinux ? ['--no-sandbox'] : [])],
        });

        openedPages = await runPageMatrix(browser, pageErrors, requestFailures);
        const searchPage = openedPages.get('/search');
        if (searchPage) {
            await checkSearchLanguageFilter(searchPage);
            await checkSearchDiagnosticsAndCooldown(searchPage);
        }
        const ideaPage = openedPages.get('/idea');
        if (ideaPage) {
            await checkCadControllerStateIsolation(ideaPage);
            await checkIdeaPageFunctionIntegrity(ideaPage);
        }
        const settingsPage = openedPages.get('/settings');
        if (settingsPage) await checkSettingsDoesNotAutoStartFreeCad(settingsPage);
        const oaPage = openedPages.get('/oa-response');
        if (oaPage) {
            await checkAmendmentEndpoint(oaPage);
            await checkDiscussionTranscriptExport(oaPage);
            await checkLongPayloadIntegrity(oaPage);
        } else {
            fail('OA regression page is available', `page=${baseUrl}/oa-response missing_page_instance=true`);
        }
    } catch (error) {
        fail('E2E test run', error.stack || error.message);
    } finally {
        await Promise.all([...openedPages.values()].map(page => page.close().catch(() => {})));
        if (browser) {
            await browser.close();
        }
    }

    if (passed !== expectedPasses && failures.length === 0) {
        fail('Stable browser regression count', `expected=${expectedPasses} actual=${passed}`);
    }
    if (failures.length > 0) {
        console.error(`E2E FAILED (${passed}/${expectedPasses} passed) against ${baseUrl}`);
        for (const failure of failures) console.error(`- ${failure}`);
        process.exitCode = 1;
        return;
    }

    console.log(`E2E PASSED (${passed}/${expectedPasses}) against ${baseUrl}`);
}

main();
