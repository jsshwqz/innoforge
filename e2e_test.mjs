import puppeteer from 'puppeteer';
import { existsSync } from 'node:fs';

const baseUrl = (process.env.INNOFORGE_E2E_BASE_URL || 'http://127.0.0.1:3000').replace(/\/$/, '');
const expectedPasses = 48;
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

async function main() {
    const pageErrors = [];
    const requestFailures = [];
    let browser;
    let openedPages = new Map();

    try {
        const executablePath = findBrowserExecutable();
        browser = await puppeteer.launch({
            headless: true,
            ...(executablePath ? { executablePath } : {}),
        });

        openedPages = await runPageMatrix(browser, pageErrors, requestFailures);
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
