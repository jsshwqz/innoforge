// ESLint 扁平配置（flat config）。ESLint v9 起不再读取 `.eslintrc.*`，
// 本文件由 `.eslintrc.json` 逐条等价迁移而来：规则、等级、忽略模式一一对应，未新增也未削弱任何检查。
// CI 的调用方式保持不变：`npx eslint static/i18n.js`。
// 参考 docs/records/2026-07-13-oa-data-integrity-retrospective.md —— 当时已记录"ESLint 10 与 .eslintrc.json 不兼容"。

// 浏览器环境全局名（对应旧配置 "env": { "browser": true }）。
// 只读声明，命中 no-redeclare 的 builtinGlobals 检查时才生效。
const browserGlobals = {
  // 宿主对象
  window: "readonly",
  document: "readonly",
  navigator: "readonly",
  location: "readonly",
  history: "readonly",
  self: "readonly",
  top: "readonly",
  parent: "readonly",
  frameElement: "readonly",
  // 存储
  localStorage: "readonly",
  sessionStorage: "readonly",
  indexedDB: "readonly",
  // 定时与帧
  setTimeout: "readonly",
  clearTimeout: "readonly",
  setInterval: "readonly",
  clearInterval: "readonly",
  requestAnimationFrame: "readonly",
  cancelAnimationFrame: "readonly",
  // 网络
  fetch: "readonly",
  XMLHttpRequest: "readonly",
  Headers: "readonly",
  Request: "readonly",
  Response: "readonly",
  FormData: "readonly",
  Blob: "readonly",
  File: "readonly",
  FileReader: "readonly",
  URL: "readonly",
  URLSearchParams: "readonly",
  WebSocket: "readonly",
  AbortController: "readonly",
  AbortSignal: "readonly",
  // 事件与 DOM
  Event: "readonly",
  CustomEvent: "readonly",
  MouseEvent: "readonly",
  KeyboardEvent: "readonly",
  DragEvent: "readonly",
  ClipboardEvent: "readonly",
  Node: "readonly",
  Element: "readonly",
  HTMLElement: "readonly",
  EventSource: "readonly",
  MutationObserver: "readonly",
  IntersectionObserver: "readonly",
  ResizeObserver: "readonly",
  DOMParser: "readonly",
  XMLSerializer: "readonly",
  Image: "readonly",
  Audio: "readonly",
  // 其它宿主能力
  console: "readonly",
  alert: "readonly",
  confirm: "readonly",
  prompt: "readonly",
  getComputedStyle: "readonly",
  matchMedia: "readonly",
  crypto: "readonly",
  performance: "readonly",
  atob: "readonly",
  btoa: "readonly",
  // 第三方：DOMPurify 由 <script> 注入（见 static/purify.min.js 与 i18n.js 全局兜底）
  DOMPurify: "readonly",
};

module.exports = [
  // 全局忽略（对应旧配置的 ignorePatterns；扁平配置里必须单独成项才全局生效）
  {
    ignores: ["node_modules/", "target/", "*.min.js"],
  },
  {
    languageOptions: {
      ecmaVersion: "latest",
      sourceType: "script",
      globals: browserGlobals,
    },
    rules: {
      "no-redeclare": ["error", { builtinGlobals: true }],
      "no-var": "off",
      "no-const-assign": "error",
      "no-undef": "warn",
      "no-unused-vars": "warn",
      "no-unexpected-multiline": "error",
      "no-extra-semi": "error",
      "no-useless-escape": "warn",
    },
  },
];
