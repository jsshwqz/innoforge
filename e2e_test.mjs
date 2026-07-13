import puppeteer from 'puppeteer';
import { existsSync } from 'node:fs';

const baseUrl = (process.env.INNOFORGE_E2E_BASE_URL || 'http://127.0.0.1:3000').replace(/\/$/, '');
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
    return `${response.status()} ${response.url()}`;
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

async function openPage(browser, path, pageErrors, requestFailures) {
    const page = await browser.newPage();
    page.on('pageerror', error => pageErrors.push(`${path}: ${error.message}`));
    page.on('console', message => {
        if (message.type() === 'error') {
            pageErrors.push(`${path}: console error: ${message.text()}`);
        }
    });
    page.on('requestfailed', request => {
        const failure = request.failure();
        requestFailures.push(`${request.url()} (${failure ? failure.errorText : 'unknown error'})`);
    });
    const response = await page.goto(`${baseUrl}${path}`, {
        waitUntil: 'networkidle0',
        timeout: 20_000,
    });
    return { page, response };
}

async function main() {
    const pageErrors = [];
    const requestFailures = [];
    let browser;

    try {
        const executablePath = findBrowserExecutable();
        browser = await puppeteer.launch({
            headless: true,
            ...(executablePath ? { executablePath } : {}),
        });

        const home = await openPage(browser, '/', pageErrors, requestFailures);
        requireCondition(
            home.response && home.response.ok(),
            '首页 HTTP 可访问',
            home.response ? formatHttp(home.response) : `${baseUrl}/ returned no response`,
        );
        requireCondition(
            await home.page.$('body'),
            '首页页面已渲染',
            `${baseUrl}/ has no body element`,
        );

        const oa = await openPage(browser, '/oa-response', pageErrors, requestFailures);
        requireCondition(
            oa.response && oa.response.ok(),
            'OA 页面 HTTP 可访问',
            oa.response ? formatHttp(oa.response) : `${baseUrl}/oa-response returned no response`,
        );
        requireCondition(
            await oa.page.$('#claims-editor'),
            'OA 修改方案控件已加载',
            `${baseUrl}/oa-response is missing #claims-editor`,
        );

        const validation = await oa.page.evaluate(async () => {
            const response = await fetch('/api/ai/check-amendments', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({}),
            });
            return { status: response.status, body: await response.json() };
        });
        requireCondition(
            validation.status >= 200 && validation.status < 500 && typeof validation.body.error === 'string',
            '审核修改方案接口参数校验可达',
            `/api/ai/check-amendments returned HTTP ${validation.status}: ${JSON.stringify(validation.body)}`,
        );

        let capturedPayload = null;
        await oa.page.setRequestInterception(true);
        oa.page.on('request', request => {
            if (request.url() === `${baseUrl}/api/ai/check-amendments` && request.method() === 'POST') {
                capturedPayload = request.postData();
                request.respond({
                    status: 200,
                    contentType: 'application/json; charset=utf-8',
                    body: JSON.stringify({ error: 'E2E mock: provider was not called' }),
                }).catch(error => fail('长文本请求拦截', error.message));
                return;
            }
            request.continue().catch(error => fail('请求继续', `${request.url()}: ${error.message}`));
        });

        const tailMarker = 'INNOFORGE_E2E_LONG_TEXT_TAIL_9f06b8';
        await oa.page.evaluate(marker => {
            const originalClaims = `权利要求书\n${'技术特征，'.repeat(12_000)}${marker}`;
            window.uploadedData.my = { title: 'E2E 专利', content: originalClaims };
            window.uploadedData.oa = { title: 'E2E OA', content: `审查意见\n${marker}` };
            document.getElementById('claims-editor').value = `修改后权利要求\n${marker}`;
            return window.checkAmendments();
        }, tailMarker);

        let payload;
        try {
            payload = capturedPayload ? JSON.parse(capturedPayload) : null;
        } catch (error) {
            fail('长文本请求体为 JSON', error.message);
        }
        requireCondition(
            payload
                && payload.original_claims.includes(tailMarker)
                && payload.amended_claims.includes(tailMarker)
                && payload.office_action.includes(tailMarker),
            'OA 长文本请求体保留尾标记（未调用真实 AI）',
            capturedPayload ? 'tail marker was missing from one or more request fields' : 'the browser did not issue the amendment-check request',
        );

        await home.page.close();
        await oa.page.close();
    } catch (error) {
        fail('测试运行', error.stack || error.message);
    } finally {
        if (browser) {
            await browser.close();
        }
    }

    if (pageErrors.length > 0) {
        fail('页面异常', pageErrors.join('\n'));
    }
    if (requestFailures.length > 0) {
        fail('页面请求失败', requestFailures.join('\n'));
    }
    if (failures.length > 0) {
        console.error(`E2E FAILED (${passed}/6 passed) against ${baseUrl}`);
        for (const failure of failures) console.error(`- ${failure}`);
        process.exitCode = 1;
        return;
    }

    console.log(`E2E PASSED (6/6) against ${baseUrl}`);
}

main();
