// issue #56 需求 3：i18n 守卫的**运行时**版本。
//
// 旧静态检查只 grep i18n.js 源码里有没有 `|| key` 字样与 key 出现次数——
// 属「实现审查」而非「行为验证」。本脚本加载**真实的** static/i18n.js
//（最小 DOM/localStorage 桩），在两种语言下实际调用 t()：
//   1. 故意请求不存在的 key → 断言返回 key 本身（回退生效），不是 `undefined`；
//   2. 中英两种 i18nLang 各触发一次真实翻译 → 断言两种语言都给出非 undefined
//      的真实文案，且中英文案互不相同（确认真的是双语而非同一份数据）。
//
// i18n.js 顶层只依赖 localStorage（line 1079）；setI18nLang 触达
// window._onI18nLangChange / document.documentElement / querySelectorAll /
// getElementById——全部用最小桩覆盖。

import { readFileSync } from 'node:fs';
import vm from 'node:vm';
import { createRequire } from 'node:module';

const require = createRequire(import.meta.url);
const path = require('node:path');
const repoRoot = path.resolve(path.dirname(new URL(import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, '$1')), '..');
const source = readFileSync(path.join(repoRoot, 'static', 'i18n.js'), 'utf8');

// ── 最小桩（i18n.js 顶层与 setI18nLang 的实际触点）──
const sandbox = {
  localStorage: {
    store: {},
    getItem(key) {
      return Object.prototype.hasOwnProperty.call(this.store, key) ? this.store[key] : null;
    },
    setItem(key, value) {
      this.store[key] = String(value);
    },
  },
  window: { addEventListener: () => {} },
  document: {
    documentElement: {},
    querySelectorAll: () => [],
    getElementById: () => null,
    addEventListener: () => {},
  },
};
sandbox.globalThis = sandbox;
vm.createContext(sandbox);
vm.runInContext(source, sandbox, { filename: 'i18n.js' });

const results = [];
function check(name, fn) {
  try {
    fn();
    results.push({ name, ok: true });
  } catch (e) {
    results.push({ name, ok: false, message: e.message });
  }
}

// ── 1. 不存在的 key：回退到 key 本身，两种语言下都不是 undefined ──
for (const lang of ['zh', 'en']) {
  check(`missing key under lang=${lang} falls back to key string (not undefined)`, () => {
    sandbox.setI18nLang(lang);
    const out = sandbox.t('nonexistent.key.does.not.exist');
    if (out === undefined) throw new Error('t() returned undefined（回退未生效）');
    if (out !== 'nonexistent.key.does.not.exist') {
      throw new Error(`t() 应回退为 key 本身，实际：${out}`);
    }
  });
}

// ── 2. 真实 key：中英两种语言各触发一次真实翻译 ──
// 用全局字典 i18n.js 里真实存在的键（cad.workspaceLabel；页面内联键不在全局字典，
// 走回退返回 key 本身——那是另一个测试已覆盖的正确行为）。
const REAL_KEY = 'cad.workspaceLabel';
check('existing key renders real zh text', () => {
  sandbox.setI18nLang('zh');
  const zh = sandbox.t(REAL_KEY);
  if (!zh || zh === 'undefined' || zh === REAL_KEY) {
    throw new Error(`zh 渲染异常：${zh}`);
  }
});

check('existing key renders real en text', () => {
  sandbox.setI18nLang('en');
  const en = sandbox.t(REAL_KEY);
  if (!en || en === 'undefined' || en === REAL_KEY) {
    throw new Error(`en 渲染异常：${en}`);
  }
});

check('zh and en renderings differ (真双语)', () => {
  sandbox.setI18nLang('zh');
  const zh = sandbox.t(REAL_KEY);
  sandbox.setI18nLang('en');
  const en = sandbox.t(REAL_KEY);
  if (zh === en) {
    throw new Error('中英文案相同——疑似同一份数据冒充双语');
  }
});

// ── 3. 语言切换真实生效（setI18nLang 持久化到 localStorage）──
check('setI18nLang persists via localStorage', () => {
  sandbox.setI18nLang('en');
  if (sandbox.localStorage.getItem('innoforge_ui_lang') !== 'en') {
    throw new Error('setI18nLang 后 localStorage 未持久化 en');
  }
  sandbox.setI18nLang('zh');
  if (sandbox.localStorage.getItem('innoforge_ui_lang') !== 'zh') {
    throw new Error('切回 zh 未持久化');
  }
});

// ── 汇总 ──
const failed = results.filter((r) => !r.ok);
for (const r of results) {
  console.log(`${r.ok ? 'PASS' : 'FAIL'} ${r.name}${r.ok ? '' : ' — ' + r.message}`);
}
if (failed.length) {
  console.error(`i18n runtime check FAILED: ${failed.length}/${results.length}`);
  process.exit(1);
}
console.log(`i18n runtime check PASSED (${results.length} checks)`);
