// tests/dom_render_106.mjs
// #106 DOM-level assertions: extract the actual rendering code from
// templates/office_action_response.html, execute it with a mock DOM,
// and assert on the rendered HTML output (not API JSON).
//
// Covers two rendering blocks (qualityScore / qualityScoreOa):
//   Block 1 ~ line 1348, Block 2 ~ line 1586
//
// Failure path (parsed=false + non-empty error):
//   - rendered HTML must contain oa.quality.parseFailed bilingual text
//   - alert must NOT be called (guard must not swallow the failure UI)
//   - no undefined / NaN / 0/10 in output
//
// Success path (parsed=true):
//   - rendered HTML must contain score value, /10, dimension names, grade
//   - no undefined / NaN in output
//
// Output: one JSON object per line (pass/fail/summary), parseable by Rust.

import { readFileSync } from 'node:fs';

// ── 1. Read template & i18n ──────────────────────────────────────────────

const templatePath = 'templates/office_action_response.html';
const i18nPath = 'static/i18n.js';

const template = readFileSync(templatePath, 'utf8');
const i18nJs = readFileSync(i18nPath, 'utf8');

// Extract I18N_COMMON from i18n.js via indirect eval (controlled scope).
// i18n.js references browser globals (localStorage, document, window);
// provide minimal stubs so it executes and we can harvest I18N_COMMON.
const extractI18n = new Function(`
  var localStorage = { getItem: () => "zh", setItem: () => {} };
  var document = {
    addEventListener: () => {},
    getElementById: () => null,
    createElement: () => ({ style: {}, appendChild: () => {} }),
    body: { appendChild: () => {} }
  };
  var window = {
    addEventListener: () => {},
    DOMPurify: { sanitize: (s) => s }
  };
  ${i18nJs}
  return I18N_COMMON;
`);
const I18N_COMMON = extractI18n();

function makeT(lang) {
    const dict = I18N_COMMON[lang] || I18N_COMMON.zh;
    return (key) => dict[key] || key;
}

// ── 2. Mock helpers ──────────────────────────────────────────────────────

function escapeHtml(s) {
    return String(s || '').replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
}

function renderRadarChartMock() {
    return '<svg class="radar-mock" width="240" height="240"></svg>';
}

const DOMPurifyMock = { sanitize: (s) => String(s || '') };
function renderMarkdownMock(s) { return String(s || ''); }

// ── 3. Extract rendering blocks from template ───────────────────────────

function extractRenderBlocks(src) {
    // Extract from the line AFTER 'var data = await resp.json();' (which
    // includes the guard) to the last 'result.appendChild(div);' before
    // '} catch(e)'.  We identify quality-score blocks by checking for
    // 'data.parsed' in the segment.
    const dataMarker = 'var data = await resp.json();';
    const catchMarker = '} catch(e)';
    const appendMarker = 'result.appendChild(div);';
    const parsedMarker = 'data.parsed';
    const blocks = [];
    let from = 0;
    while (true) {
        const dataPos = src.indexOf(dataMarker, from);
        if (dataPos === -1) break;
        const lineEnd = src.indexOf('\n', dataPos) + 1;
        const catchPos = src.indexOf(catchMarker, lineEnd);
        if (catchPos === -1) break;
        const segment = src.substring(lineEnd, catchPos);
        if (!segment.includes(parsedMarker)) {
            from = catchPos + catchMarker.length;
            continue;
        }
        const lastAppend = segment.lastIndexOf(appendMarker);
        if (lastAppend === -1) {
            from = catchPos + catchMarker.length;
            continue;
        }
        blocks.push(segment.substring(0, lastAppend + appendMarker.length));
        from = catchPos + catchMarker.length;
    }
    return blocks;
}

// ── 4. Mock DOM environment ──────────────────────────────────────────────

function runBlock(blockCode, data, t) {
    const alertCalls = [];
    let renderedHtml = '';

    const mockResult = {
        appendChild: (d) => { renderedHtml = d.innerHTML || ''; }
    };
    const mockDocument = {
        getElementById: (id) => {
            if (id === 'result') return mockResult;
            return { disabled: false, innerHTML: '' };
        },
        createElement: () => ({ style: {}, id: '', innerHTML: '' })
    };

    const fn = new Function(
        'data', 'document', 'alert', 't', 'escapeHtml',
        'renderRadarChart', 'DOMPurify', 'renderMarkdown',
        blockCode
    );
    fn(
        data, mockDocument,
        (...args) => { alertCalls.push(args.map(String).join(' ')); },
        t, escapeHtml, renderRadarChartMock, DOMPurifyMock, renderMarkdownMock
    );

    return { html: renderedHtml, alertCalls };
}

// ── 5. Test data (synthetic, no case data) ───────────────────────────────

const failureData = {
    status: 'ok',
    parsed: false,
    error: 'Failed to parse model output as JSON: expected value at line 1 column 1',
    raw: 'This is not JSON'
};

const successData = {
    status: 'ok',
    parsed: true,
    score: 7.5,
    max: 10,
    dimensions: [
        { name: '技术新颖性', score: 8, comment: '' },
        { name: '答复完整性', score: 7, comment: '' },
        { name: '法律合规性', score: 9, comment: '' },
        { name: '逻辑清晰度', score: 6, comment: '' },
        { name: '对比文件区分度', score: 7, comment: '' },
        { name: '修改可行性', score: 8, comment: '' }
    ],
    grade: 'B',
    weaknesses: '逻辑清晰度有待加强',
    suggestions: '建议增加对比文件分析'
};

// ── 6. Run assertions ────────────────────────────────────────────────────

let passed = 0;
let failed = 0;

function pass(name) {
    passed++;
    console.log(JSON.stringify({ kind: 'pass', name }));
}
function fail(name, reason) {
    failed++;
    console.log(JSON.stringify({ kind: 'fail', name, reason }));
}
function assert(name, cond, reason) {
    if (cond) pass(name); else fail(name, reason);
}

const blocks = extractRenderBlocks(template);
assert('blocks-extracted', blocks.length >= 2,
      `expected >=2 render blocks, got ${blocks.length}`);

const tZh = makeT('zh');
const tEn = makeT('en');
const parseFailedZh = tZh('oa.quality.parseFailed');
const parseFailedEn = tEn('oa.quality.parseFailed');

// Verify the i18n values are non-trivial (not the key itself)
assert('i18n-zh-nontrivial', parseFailedZh !== 'oa.quality.parseFailed' && parseFailedZh.length > 4,
      `zh parseFailed looks wrong: "${parseFailedZh}"`);
assert('i18n-en-nontrivial', parseFailedEn !== 'oa.quality.parseFailed' && parseFailedEn.length > 4,
      `en parseFailed looks wrong: "${parseFailedEn}"`);

for (let bi = 0; bi < blocks.length; bi++) {
    const B = `B${bi + 1}`;

    // ── Failure path: parsed=false + non-empty error ──
    const fr = runBlock(blocks[bi], failureData, tZh);
    const frEn = runBlock(blocks[bi], failureData, tEn);

    // Must contain bilingual parseFailed text
    assert(`${B}-fail-parseFailed-zh`,
           fr.html.includes(parseFailedZh),
           `missing zh parseFailed "${parseFailedZh}" in html (len=${fr.html.length})`);

    assert(`${B}-fail-parseFailed-en`,
           frEn.html.includes(parseFailedEn),
           `missing en parseFailed "${parseFailedEn}" in html (len=${frEn.html.length})`);

    // Alert must NOT fire (guard must not swallow the failure UI)
    assert(`${B}-fail-noAlert`,
           fr.alertCalls.length === 0,
           `alert called ${fr.alertCalls.length}x: ${fr.alertCalls.slice(0, 2).join(' | ')}`);

    // No undefined / NaN / 0/10
    assert(`${B}-fail-noUndefined`,
           !fr.html.includes('undefined'),
           'html contains "undefined"');
    assert(`${B}-fail-noNaN`,
           !fr.html.includes('NaN'),
           'html contains "NaN"');
    assert(`${B}-fail-noZeroTen`,
           !fr.html.includes('0/10'),
           'html contains "0/10"');

    // ── Success path: parsed=true ──
    const sr = runBlock(blocks[bi], successData, tZh);

    // Must contain score value
    assert(`${B}-ok-score`,
           sr.html.includes('7.5'),
           'html missing score 7.5');

    // Must contain /10 (block 1 UI shows score/max; block 2 shows score only)
    if (bi === 0) {
        assert(`${B}-ok-maxScore`,
               sr.html.includes('/10'),
               'html missing "/10"');
    }

    // Must contain all six dimension names
    for (const dim of successData.dimensions) {
        assert(`${B}-ok-dim-${dim.name}`,
               sr.html.includes(dim.name),
               `html missing dimension "${dim.name}"`);
    }

    // Must contain grade
    assert(`${B}-ok-grade`,
           sr.html.includes('B'),
           'html missing grade "B"');

    // No undefined / NaN
    assert(`${B}-ok-noUndefined`,
           !sr.html.includes('undefined'),
           'html contains "undefined"');
    assert(`${B}-ok-noNaN`,
           !sr.html.includes('NaN'),
           'html contains "NaN"');

    // ── Regression: alert not called on failure ──
    assert(`${B}-regress-noAlertOnFail`,
           fr.alertCalls.length === 0,
           `regression: alert called on parsed=false path`);
}

// ── 7. Summary ───────────────────────────────────────────────────────────
console.log(JSON.stringify({ kind: 'summary', passed, failed, total: passed + failed }));
process.exit(failed > 0 ? 1 : 0);
