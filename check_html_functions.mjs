// 硬防线：静态扫描所有模板 HTML，确保每个 onclick/onchange 引用的函数都有定义
// 防止"按钮在、函数没了"的静默丢失（如 42c726e / 9f1a14b 两次重构事故）
// 用法：node check_html_functions.mjs
// 退出码：0=全部通过，1=发现未定义函数

import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';

const templatesDir = join(process.cwd(), 'templates');
const files = readdirSync(templatesDir).filter(f => f.endsWith('.html'));
let totalIssues = 0;

// 已知由 /static/*.js 或其它页面提供的全局函数（跨文件共享）
const globalFunctions = new Set([
    // i18n.js / cad.js 提供
    't', 'renderNavbar', 'renderSidebar', 'applyI18nCommon', 'reapplyPage',
    'esc', 'DOMPurify', 'scrollTo', 'showStatus', 'closeActiveSSE',
    'InnoForgeCad', 'renderMarkdown', 'toggleDimension',
]);

for (const file of files) {
    const html = readFileSync(join(templatesDir, file), 'utf8');
    const issues = scanFile(file, html);
    if (issues.length > 0) {
        totalIssues += issues.length;
        console.log(`\n❌ ${file}:`);
        for (const issue of issues) console.log(`   ${issue}`);
    }
}

if (totalIssues === 0) {
    console.log(`✅ 静态扫描通过：${files.length} 个模板，所有 on* 事件引用的函数均有定义`);
} else {
    console.error(`\n❌ 发现 ${totalIssues} 个未定义函数引用，请先修复再编译！`);
    process.exit(1);
}

function scanFile(file, html) {
    // 提取所有内联事件处理器：onclick="fn()" / onchange="fn()" / onkeydown=... / onmouseover=...
    const eventRe = /on(?:click|change|keydown|keyup|input|mouseover|mouseout|mouseenter|mouseleave|focus|blur|load|submit|paste)="([^"]+)"/g;
    const invoked = new Set();
    let m;
    while ((m = eventRe.exec(html)) !== null) {
        const handler = m[1];
        // 提取 handler 中调用的函数名：只匹配裸函数调用（前面不是 . 或对象引用）
        // 排除 event.preventDefault() / this.closest() / x.replace() 这类方法调用
        const fnRe = /(?:^|[^\w$.])([A-Za-z_$][\w$]*)\s*\(/g;
        let fm;
        while ((fm = fnRe.exec(handler)) !== null) {
            const name = fm[1];
            if (shouldSkip(name)) continue;
            invoked.add(name);
        }
    }

    // 提取 <script> 块里定义的所有函数（function name( 或 name = function / name: function / async function name）
    const scriptRe = /<script>([\s\S]*?)<\/script>/g;
    const defined = new Set();
    let sm;
    while ((sm = scriptRe.exec(html)) !== null) {
        const body = sm[1];
        collectDefined(body, defined);
    }

    // 检查：每个被调用但未定义的函数
    const issues = [];
    for (const name of invoked) {
        if (!defined.has(name) && !isGloballyDefined(name)) {
            issues.push(`on* 引用了未定义函数: ${name}`);
        }
    }
    return issues;
}

function collectDefined(body, defined) {
    const patterns = [
        /function\s+([A-Za-z_$][\w$]*)\s*\(/g,              // function foo(
        /(?:var|let|const)\s+([A-Za-z_$][\w$]*)\s*=\s*(?:async\s*)?function/g, // var foo = function
        /([A-Za-z_$][\w$]*)\s*:\s*(?:async\s*)?function/g,  // foo: function
        /async\s+function\s+([A-Za-z_$][\w$]*)\s*\(/g,       // async function foo(
    ];
    for (const re of patterns) {
        let m;
        while ((m = re.exec(body)) !== null) defined.add(m[1]);
    }
}

function shouldSkip(name) {
    const skip = new Set([
        // DOM / 浏览器 API
        'alert', 'confirm', 'prompt', 'fetch', 'console', 'window', 'document',
        'setTimeout', 'setInterval', 'clearTimeout', 'clearInterval', 'JSON',
        'encodeURIComponent', 'decodeURIComponent', 'encodeURI', 'decodeURI',
        'String', 'Number', 'Boolean', 'Array', 'Object', 'Math', 'Date',
        'parseInt', 'parseFloat', 'isNaN', 'isFinite', 'RegExp', 'Error',
        'event', 'this', 'new', 'typeof', 'if', 'return', 'function',
        'URLSearchParams', 'FileReader', 'EventSource', 'FormData',
        'requestAnimationFrame', 'cancelAnimationFrame',
        // 模板字符串插值等误报
        'data', 'function', 'i18nLang', 'rgba', 'rgb', 'linear-gradient', 'hsl',
        'hsla', 'var', 'calc', 'url', 'translate', 'rotate', 'scale',
    ]);
    return skip.has(name) || name.startsWith('window.');
}

function isGloballyDefined(name) {
    return globalFunctions.has(name);
}
