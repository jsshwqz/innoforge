// 硬防线：静态扫描所有模板 HTML，确保每个 onclick/onchange 等内联事件引用的函数都有定义。
// 着眼未来：扫描器自动发现 templates/ 下所有 *.html（新增页面自动纳入），不靠硬编码页面清单。
// 规则 1 (ERROR)：任何 on* 引用的函数未定义（按钮在、函数没了）——历史事故 42c726e / 9f1a14b。
// 规则 2 (ERROR)：基线中存在的引用/定义在当前被移除（整块功能消失）——静态扫描原理盲区由基线补上。
// 新增（当前有、基线无）→ INFO 不报错，允许开发新功能，提示 --refresh 更新基线。
// 基线缺失且未 --refresh → ERROR（防止基线被静默删除）。
// 用法：node check_html_functions.mjs   /   node check_html_functions.mjs --refresh

import { readFileSync, writeFileSync, existsSync, mkdirSync, readdirSync, statSync } from 'node:fs';
import { join, dirname } from 'node:path';

const templatesDir = join(process.cwd(), 'templates');
const manifestPath = join(process.cwd(), 'docs', 'functions-manifest.json');
const refresh = process.argv.includes('--refresh');

const globalFunctions = new Set([
    't','renderNavbar','renderSidebar','applyI18nCommon','reapplyPage',
    'esc','DOMPurify','scrollTo','showStatus','closeActiveSSE',
    'InnoForgeCad','renderMarkdown','toggleDimension',
]);
const issues = []; const infos = [];

function collectTemplates() {
    if (!existsSync(templatesDir) || !statSync(templatesDir).isDirectory()) return [];
    return readdirSync(templatesDir).filter(f => f.endsWith('.html')).sort();
}
function buildSnapshot() {
    const pages = {};
    for (const file of collectTemplates()) {
        const html = readFileSync(join(templatesDir, file), 'utf8');
        const refs = new Set(); const defs = new Set();
        extractRefs(html, refs); extractDefs(html, defs);
        pages[file] = { references: [...refs].sort(), definitions: [...defs].sort() };
    }
    return pages;
}
function extractRefs(html, invoked) {
    const eventRe = /on(?:click|change|keydown|keyup|input|mouseover|mouseout|mouseenter|mouseleave|focus|blur|load|submit|paste)="([^"]+)"/g;
    let m;
    while ((m = eventRe.exec(html)) !== null) {
        const fnRe = /(?:^|[^\w$\.])([A-Za-z_$][\w$]*)\s*\(/g;
        let fm;
        while ((fm = fnRe.exec(m[1])) !== null) { if (!shouldSkip(fm[1])) invoked.add(fm[1]); }
    }
}
function extractDefs(html, defined) {
    const scriptRe = /<script>([\s\S]*?)<\/script>/g;
    let sm; while ((sm = scriptRe.exec(html)) !== null) collectDefined(sm[1], defined);
}
function collectDefined(body, defined) {
    for (const re of [
        /function\s+([A-Za-z_$][\w$]*)\s*\(/g,
        /(?:var|let|const)\s+([A-Za-z_$][\w$]*)\s*=\s*(?:async\s*)?function/g,
        /([A-Za-z_$][\w$]*)\s*:\s*(?:async\s*)?function/g,
        /async\s+function\s+([A-Za-z_$][\w$]*)\s*\(/g,
    ]) { let m; while ((m = re.exec(body)) !== null) defined.add(m[1]); }
}
function shouldSkip(name) {
    const skip = new Set([
        'alert','confirm','prompt','fetch','console','window','document',
        'setTimeout','setInterval','clearTimeout','clearInterval','JSON',
        'encodeURIComponent','decodeURIComponent','encodeURI','decodeURI',
        'String','Number','Boolean','Array','Object','Math','Date',
        'parseInt','parseFloat','isNaN','isFinite','RegExp','Error',
        'event','this','new','typeof','if','return','function',
        'URLSearchParams','FileReader','EventSource','FormData',
        'requestAnimationFrame','cancelAnimationFrame',
        'data','function','i18nLang','rgba','rgb','linear-gradient',
        'hsl','hsla','var','calc','url','translate','rotate','scale',
    ]);
    return skip.has(name) || name.startsWith('window.');
}
function loadBaseline() {
    if (!existsSync(manifestPath)) return null;
    try { return JSON.parse(readFileSync(manifestPath, 'utf8')); }
    catch (e) { issues.push({ page: '(manifest)', text: '基线文件损坏: ' + e.message }); return null; }
}
function saveBaseline(pages) {
    mkdirSync(dirname(manifestPath), { recursive: true });
    writeFileSync(manifestPath, JSON.stringify({ version: 1, generatedAt: new Date().toISOString(), pages }, null, 2) + '\n', 'utf8');
}
function compare(snapshot) {
    const baseline = loadBaseline();
    const basePages = baseline ? baseline.pages : null;
    if (!basePages && !refresh) {
        issues.push({ page: '(manifest)', text: '基线缺失。运行 node check_html_functions.mjs --refresh 生成基线并提交' });
    }
    const allPages = new Set(Object.keys(snapshot));
    if (basePages) for (const p of Object.keys(basePages)) allPages.add(p);
    for (const page of [...allPages].sort()) {
        const cur = snapshot[page] || { references: [], definitions: [] };
        const base = basePages && basePages[page] ? basePages[page] : null;
        const curRefs = new Set(cur.references), curDefs = new Set(cur.definitions);
        const baseRefs = base ? new Set(base.references) : null;
        const baseDefs = base ? new Set(base.definitions) : null;
        for (const ref of cur.references) {
            if (!curDefs.has(ref) && !globalFunctions.has(ref))
                issues.push({ page, text: 'on* 引用未定义函数: ' + ref });
        }
        if (!base) { infos.push({ page, text: '新增页面/无基线，跑 --refresh 纳入基线' }); continue; }
        for (const ref of base.references) {
            if (!curRefs.has(ref)) issues.push({ page, text: '基线中引用已移除（整块功能可能丢失）: ' + ref });
        }
        for (const def of base.definitions) {
            if (!curDefs.has(def)) issues.push({ page, text: '基线中定义已移除: ' + def });
        }
        const newRefs = [...curRefs].filter(r => !baseRefs.has(r));
        const newDefs = [...curDefs].filter(d => !baseDefs.has(d));
        if (newRefs.length) infos.push({ page, text: '新增引用: ' + newRefs.join(', ') });
        if (newDefs.length) infos.push({ page, text: '新增定义: ' + newDefs.join(', ') });
    }
}
function main() {
    const snapshot = buildSnapshot();
    compare(snapshot);
    if (refresh) {
        saveBaseline(snapshot);
        console.log('✅ 基线已刷新：' + Object.keys(snapshot).length + ' 个页面 → ' + manifestPath);
        if (issues.length) {
            console.warn('⚠️  注意：当前状态仍有未定义引用，刷新后请修复：');
            for (const i of issues) console.warn('   ' + i.page + ': ' + i.text);
        }
        if (issues.length) process.exit(1);
        return;
    }
    if (issues.length) {
        console.error('\n❌ 发现 ' + issues.length + ' 个问题：');
        for (const i of issues) console.error('   ' + i.page + ': ' + i.text);
        console.error('\n请先修复，或开发完新功能后运行 --refresh 更新基线。');
        process.exit(1);
    }
    if (infos.length) {
        console.log('\nℹ️  基线待更新（' + infos.length + ' 条新增，不阻断；跑 --refresh 纳入基线）：');
        for (const i of infos) console.log('   ' + i.page + ': ' + i.text);
    }
    console.log('✅ 静态扫描通过：' + Object.keys(snapshot).length + ' 个模板，所有 on* 引用均有定义，基线一致');
}

main();