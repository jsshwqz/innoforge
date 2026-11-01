// 生成 docs/API-CONTRACTS.md：每条 /api 路由「收什么、回什么」的源码级契约。
//
// 与 gen_route_inventory.js 的分工：
//   - 清单只回答「有哪些端点、处理函数在哪」（docs/API-ROUTES.md）；
//   - 本文件再往下走一层：路径参数、请求体/查询类型（能解析到结构体时列出字段）、
//     响应类型；对 `Json<serde_json::Value>` 这类自由格式端点，从处理函数体里
//     静态抽取它实际读取/返回的键。
//
// 为什么是静态抽取而不是手写：手写的字段说明会和代码漂移，而这正是 #62/#67 的病根。
// 抽取不到的地方会明写「未抽取到」，不假装查过（见 docs/errors.md 的「没查≠查过」）。
//
// 用法：
//   node scripts/gen_api_contracts.js            # 写入 docs/API-CONTRACTS.md
//   node scripts/gen_api_contracts.js --check    # 只比对，不一致 exit 1（守卫用）
const fs = require("fs");
const path = require("path");

const ROOT = path.resolve(__dirname, "..");
const SRC = path.join(ROOT, "src", "common.rs");
const OUT = path.join(ROOT, "docs", "API-CONTRACTS.md");
const METHODS = ["get", "post", "put", "delete", "patch", "head", "options"];

// ---------- 通用文本扫描（跳过注释/字符串，避免把提示词或注释里的括号算进去） ----------

function skipNonCode(t, i) {
  const c = t[i];
  if (c === "/" && t[i + 1] === "/") {
    const nl = t.indexOf("\n", i);
    return nl === -1 ? t.length : nl + 1;
  }
  if (c === "/" && t[i + 1] === "*") {
    let depth = 1;
    let j = i + 2;
    while (j < t.length && depth > 0) {
      if (t[j] === "/" && t[j + 1] === "*") {
        depth++;
        j += 2;
      } else if (t[j] === "*" && t[j + 1] === "/") {
        depth--;
        j += 2;
      } else j++;
    }
    return j;
  }
  // 原始字符串 r"..." / r#"..."#
  if (c === "r" && (t[i + 1] === '"' || t[i + 1] === "#")) {
    let j = i + 1;
    let hashes = 0;
    while (t[j] === "#") {
      hashes++;
      j++;
    }
    if (t[j] === '"') {
      const close = '"' + "#".repeat(hashes);
      const end = t.indexOf(close, j + 1);
      return end === -1 ? t.length : end + close.length;
    }
  }
  if (c === '"') {
    let j = i + 1;
    while (j < t.length) {
      if (t[j] === "\\") j += 2;
      else if (t[j] === '"') return j + 1;
      else j++;
    }
    return t.length;
  }
  // 字符字面量：'{' / '}' / '\''；生命期 'a 不算
  if (c === "'") {
    if (t[i + 1] === "\\") {
      const end = t.indexOf("'", i + 2);
      return end === -1 ? i + 1 : end + 1;
    }
    if (t[i + 2] === "'") return i + 3;
  }
  return -1;
}

function matchPair(t, openIdx) {
  const open = t[openIdx];
  const close = open === "(" ? ")" : open === "{" ? "}" : open === "<" ? ">" : "]";
  let depth = 0;
  for (let j = openIdx; j < t.length; j++) {
    const skip = skipNonCode(t, j);
    if (skip !== -1) {
      j = skip - 1;
      continue;
    }
    if (t[j] === open) depth++;
    else if (t[j] === close) {
      depth--;
      if (depth === 0) return j;
    }
  }
  return -1;
}

// 取 `Head<...>` 的尖括号内文本；不是该形态则返回 null
function genericArgs(s) {
  const m = s.match(/^([A-Za-z_][A-Za-z0-9_:]*)\s*</);
  if (!m) return null;
  const lt = s.indexOf("<");
  const gt = matchPair(s, lt);
  if (gt === -1) return null;
  return { head: m[1], args: s.slice(lt + 1, gt).trim() };
}

function lineOf(t, offset) {
  let line = 1;
  for (let i = 0; i < offset && i < t.length; i++) if (t[i] === "\n") line++;
  return line;
}

// 按顶层逗号切分（泛型/元组/闭包参数不切）
function splitTopLevel(s) {
  const out = [];
  let depth = 0;
  let cur = "";
  for (let i = 0; i < s.length; i++) {
    const c = s[i];
    if ("<([{".includes(c)) depth++;
    else if (">)]}".includes(c)) depth--;
    if (c === "," && depth === 0) {
      out.push(cur.trim());
      cur = "";
    } else cur += c;
  }
  if (cur.trim()) out.push(cur.trim());
  return out;
}

// ---------- 源码索引 ----------

function walk(dir, out = []) {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) walk(p, out);
    else if (e.name.endsWith(".rs")) out.push(p);
  }
  return out;
}

const files = walk(path.join(ROOT, "src")).sort();
const sources = new Map();
for (const f of files) {
  sources.set(f, fs.readFileSync(f, "utf8"));
}
const rel = (f) => path.relative(ROOT, f).replace(/\\/g, "/");

// fn 索引：name -> [{file, at}]
const fnIndex = new Map();
for (const [file, t] of sources) {
  const re = /\bfn\s+([A-Za-z_][A-Za-z0-9_]*)/g;
  let m;
  while ((m = re.exec(t))) {
    if (!fnIndex.has(m[1])) fnIndex.set(m[1], []);
    fnIndex.get(m[1]).push({ file, at: m.index });
  }
}

// 结构体索引：name -> {file, line, serdeRenameAll, fields:[{name,type,optional,jsonName}]}
const structIndex = new Map();
for (const [file, t] of sources) {
  const re = /\b(?:pub(?:\([^)]*\))?\s+)?struct\s+([A-Za-z_][A-Za-z0-9_]*)/g;
  let m;
  while ((m = re.exec(t))) {
    let j = m.index + m[0].length;
    // 跳过泛型参数
    while (j < t.length && t[j] !== "{" && t[j] !== ";" && t[j] !== "(") j++;
    if (t[j] !== "{") continue;
    const end = matchPair(t, j);
    const body = t.slice(j + 1, end);
    const attrs = t.slice(Math.max(0, m.index - 200), m.index);
    const ra = attrs.match(/#\[serde\(rename_all\s*=\s*"([^"]+)"\)\]/);
    const fields = [];
    let pending = [];
    for (const rawLine of body.split("\n")) {
      let line = rawLine.trim();
      if (line.startsWith("//") || line === "") continue;
      // 同一行里先摘属性，再取字段
      while (line.startsWith("#[")) {
        const closeAttr = line.indexOf("]");
        if (closeAttr === -1) break;
        pending.push(line.slice(0, closeAttr + 1));
        line = line.slice(closeAttr + 1).trim();
        if (!line) break;
      }
      if (!line) continue;
      line = line.replace(/\s*\/\/.*$/, "").trim();
      const fm = line.match(/^(?:pub(?:\([^)]*\))?\s+)?([a-z_][A-Za-z0-9_]*)\s*:\s*(.+?),?\s*$/);
      if (fm) {
        let fname = fm[1];
        const ftype = fm[2].replace(/,$/, "").trim();
        const attrText = pending.join(" ");
        const rename = attrText.match(/serde\(rename\s*=\s*"([^"]+)"\)/);
        let jsonName = rename ? rename[1] : fname;
        if (!rename && ra) jsonName = applyRenameAll(fname, ra[1]);
        const optional =
          ftype.startsWith("Option<") || /serde\(default/.test(attrText) || /serde\(skip_serializing_if/.test(attrText);
        fields.push({ name: fname, jsonName, type: ftype, optional });
      }
      pending = [];
    }
    structIndex.set(m[1], { file, line: lineOf(t, m.index), fields, fieldsText: body });
  }
}

function applyRenameAll(name, rule) {
  const parts = name.split("_");
  if (rule === "camelCase") {
    return parts.map((p, i) => (i === 0 ? p : p[0].toUpperCase() + p.slice(1))).join("");
  }
  if (rule === "PascalCase") return parts.map((p) => p[0].toUpperCase() + p.slice(1)).join("");
  if (rule === "kebab-case") return parts.join("-");
  if (rule === "SCREAMING_SNAKE_CASE") return name.toUpperCase();
  if (rule === "snake_case")
    return name
      .replace(/([a-z0-9])([A-Z])/g, "$1_$2")
      .replace(/([A-Z])([A-Z][a-z])/g, "$1_$2")
      .toLowerCase();
  if (rule === "lowercase") return name.toLowerCase();
  if (rule === "UPPERCASE") return name.toUpperCase();
  return name;
}

// 枚举索引：name -> {file, line, variants:[{name,jsonName}]}
const enumIndex = new Map();
for (const [file, t] of sources) {
  const re = /\b(?:pub(?:\([^)]*\))?\s+)?enum\s+([A-Za-z_][A-Za-z0-9_]*)/g;
  let m;
  while ((m = re.exec(t))) {
    let j = m.index + m[0].length;
    while (j < t.length && t[j] !== "{") j++;
    if (j >= t.length) continue;
    const end = matchPair(t, j);
    if (end === -1) continue;
    const body = t.slice(j + 1, end);
    const ra = t.slice(Math.max(0, m.index - 200), m.index).match(/#\[serde\(rename_all\s*=\s*"([^"]+)"\)\]/);
    const variants = [];
    for (const part of splitTopLevel(body)) {
      const attrs = part.match(/^\s*((?:#\[[^\]]*\]\s*)+)/);
      const attrText = attrs ? attrs[1] : "";
      const clean = part.slice(attrText.length).trim();
      const vm = clean.match(/^([A-Za-z_][A-Za-z0-9_]*)/);
      if (!vm) continue;
      const rn = attrText.match(/serde\(rename\s*=\s*"([^"]+)"\)/);
      const jsonName = rn ? rn[1] : ra ? applyRenameAll(vm[1], ra[1]) : vm[1];
      variants.push({ name: vm[1], jsonName });
    }
    enumIndex.set(m[1], { file, line: lineOf(t, m.index), variants });
  }
}

// ---------- 路由抽取 ----------

const src = fs.readFileSync(SRC, "utf8");
function extractRoutes() {
  const out = [];
  let from = 0;
  for (;;) {
    const start = src.indexOf(".route(", from);
    if (start === -1) break;
    from = start + 1;
    const close = matchPair(src, src.indexOf("(", start + ".route".length));
    const seg = src.slice(start, close + 1);
    const pm = seg.match(/\.route\(\s*"([^"]+)"/);
    if (!pm) continue;
    const afterPath = seg.slice(pm.index + pm[0].length);
    const methods = METHODS.filter((mm) => new RegExp(`\\b${mm}\\s*\\(`).test(afterPath)).map((mm) =>
      mm.toUpperCase()
    );
    const hm = afterPath.match(/\b(?:get|post|put|delete|patch|head|options)\s*\(\s*([A-Za-z_][A-Za-z0-9_:]*)/);
    out.push({
      path: pm[1],
      methods: methods.length ? methods : ["?"],
      handler: hm ? hm[1] : "",
      commonLine: lineOf(src, start),
    });
  }
  return out;
}

// ---------- 处理函数解析 ----------

function handlerSite(handler) {
  const name = handler.split("::").pop();
  const list = fnIndex.get(name);
  if (!list || !list.length) return null;
  const lastMod = handler.split("::").slice(-2, -1)[0];
  const preferred = list.find((x) => path.basename(x.file, ".rs") === lastMod);
  return preferred || list[0];
}

function parseHandler(site) {
  const t = sources.get(site.file);
  let i = site.at;
  while (i < t.length && t[i] !== "(") i++;
  const pEnd = matchPair(t, i);
  const sig = t.slice(i + 1, pEnd);
  let b = pEnd + 1;
  while (b < t.length && t[b] !== "{") b++;
  const bEnd = matchPair(t, b);
  const body = t.slice(b + 1, bEnd);
  const retText = t.slice(pEnd + 1, b).trim();
  const ret = retText.replace(/^->\s*/, "").replace(/\s+/g, " ");

  const params = splitTopLevel(sig).map((p) => classifyParam(p));
  return {
    sigLine: lineOf(t, site.at),
    params,
    ret,
    body,
  };
}

function stripPath(s) {
  return s.replace(/^(?:axum::extract|axum)::/, "").trim();
}

function shortType(raw) {
  const t = stripPath(raw).replace(/\s+/g, " ");
  if (/^(?:crate::)?serde_json::Value$|^Value$/.test(t)) return { display: "任意 JSON", arbitrary: true };
  if (/HashMap\s*<\s*String\s*,\s*String\s*>/.test(t)) return { display: "任意键值对", arbitrary: true };
  const last = t.split("::").pop();
  return { display: last, arbitrary: false, full: t };
}

function classifyParam(raw) {
  const p = raw.replace(/^mut\s+/, "").trim();
  // 绑定模式可能是 `Json(req)`、`Path((a, b))`、`State(s)`，括号里还能再套括号
  const bm = p.match(/^([A-Za-z_][A-Za-z0-9_]*)\s*\(/);
  let varName = "";
  let rest = p;
  if (bm) {
    const open = p.indexOf("(");
    const close = matchPair(p, open);
    varName = p.slice(open + 1, close).trim();
    rest = p.slice(close + 1).trim();
    if (!rest.startsWith(":")) return { kind: "other", varName, typeRaw: p, display: p };
  }
  const m = rest.match(/^:\s*([\s\S]+)$/);
  if (!m) {
    // 形如 `String` 的裸提取器（route-level string）
    return { kind: "other", varName: "", typeRaw: p, display: shortType(p).display };
  }
  const typeRaw = m[1].trim();
  const head = stripPath(typeRaw);
  const km = head.match(/^([A-Za-z_][A-Za-z0-9_]*)\s*(?:<([\s\S]*)>)?$/);
  const kindName = km ? km[1] : head;
  const inner = km && km[2] ? km[2].trim() : "";
  if (kindName === "Json")
    return { kind: "json", varName, typeRaw, type: shortType(inner), inner: inner.replace(/\s+/g, " ") };
  if (kindName === "Query") return { kind: "query", varName, typeRaw, type: shortType(inner), inner };
  if (kindName === "Path") return { kind: "path", varName, typeRaw, type: shortType(inner), inner };
  if (kindName === "Form") return { kind: "form", varName, typeRaw, type: shortType(inner), inner };
  if (kindName === "Multipart") return { kind: "multipart", varName, typeRaw, display: "multipart/form-data" };
  if (kindName === "State") return { kind: "state", varName, typeRaw, display: "AppState（服务端注入）" };
  return { kind: "other", varName, typeRaw, display: kindName };
}

function normalizeResponse(ret) {
  const r = ret.replace(/\s+/g, " ");
  if (!r || r === "?" || r === "") return { display: "未解析", raw: "" };
  const sse = /^Sse\s*</.test(r);
  if (sse) return { display: "SSE 事件流（text/event-stream）", raw: r };
  let okType = r;
  let errType = "";
  const rg = genericArgs(r);
  if (rg && /(?:^|::)Result$/.test(rg.head)) {
    const parts = splitTopLevel(rg.args);
    okType = (parts[0] || "").trim();
    errType = (parts[1] || "").trim();
  }
  const jg = genericArgs(okType);
  if (jg && /Json$/.test(jg.head)) {
    const st = shortType(jg.args);
    return {
      display: st.arbitrary ? "JSON（顶层键见字段抽取）" : "JSON " + st.display,
      type: st,
      error: errType,
      raw: r,
    };
  }
  if (/^(?:axum::response::)?Response(?:<[^>]*>)?$/.test(okType) || okType === "impl IntoResponse" || /^\(StatusCode,\s*&'static str\)$/.test(okType)) {
    return { display: "HTTP 响应（动态，状态码/正文见源码）", raw: r };
  }
  return { display: okType, raw: r };
}

// ---------- 自由 JSON 的字段抽取 ----------

function jsonMacroObjects(body) {
  const objs = [];
  const re = /json!\s*\(/g;
  let m;
  while ((m = re.exec(body))) {
    const open = body.indexOf("(", m.index);
    const close = matchPair(body, open);
    let b = open + 1;
    while (b < close && /\s/.test(body[b])) b++;
    if (body[b] !== "{") continue;
    const braceEnd = matchPair(body, b);
    objs.push(body.slice(b, braceEnd + 1));
  }
  return objs;
}

function topLevelKeys(objLiteral) {
  // objLiteral 以 { 开头；取深度 1 的 "key":
  const keys = [];
  let depth = 0;
  for (let i = 0; i < objLiteral.length; i++) {
    const c = objLiteral[i];
    if (c === '"') {
      let j = i + 1;
      let word = "";
      while (j < objLiteral.length && objLiteral[j] !== '"') {
        if (objLiteral[j] === "\\") {
          word += objLiteral[j];
          j++;
        }
        word += objLiteral[j];
        j++;
      }
      const end = j;
      let k = end + 1;
      while (k < objLiteral.length && /\s/.test(objLiteral[k])) k++;
      if (depth === 1 && objLiteral[k] === ":" && /^[A-Za-z0-9_]+$/.test(word)) keys.push(word);
      i = end;
      continue;
    }
    if (c === "{") depth++;
    else if (c === "}") depth--;
  }
  return keys;
}

function extractReqKeys(body, varName) {
  const keys = new Set();
  if (!varName) return keys;
  const esc = varName.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  // req["x"] / req.get("x")
  for (const re of [
    new RegExp(`\\b${esc}\\s*\\[\\s*"([A-Za-z0-9_]+)"\\s*\\]`, "g"),
    new RegExp(`\\b${esc}\\.get\\(\\s*"([A-Za-z0-9_]+)"\\s*\\)`, "g"),
    new RegExp(`\\b[a-z_][a-z0-9_]*\\(\\s*&?${esc}\\s*,\\s*"([A-Za-z0-9_]+)"`, "g"),
  ]) {
    let m;
    while ((m = re.exec(body))) keys.add(m[1]);
  }
  return keys;
}

function extractRespKeys(body) {
  const keys = new Set();
  const statuses = new Set();
  for (const obj of jsonMacroObjects(body)) {
    for (const k of topLevelKeys(obj)) keys.add(k);
  }
  const re = /"status"\s*:\s*"([A-Za-z0-9_]+)"/g;
  let m;
  while ((m = re.exec(body))) statuses.add(m[1]);
  return { keys, statuses };
}

// ---------- 汇总 ----------

const routes = extractRoutes();
const resolved = [];
const unresolved = [];
for (const r of routes) {
  const site = handlerSite(r.handler);
  if (!site) {
    unresolved.push(r);
    continue;
  }
  const info = parseHandler(site);
  const json = info.params.find((p) => p.kind === "json");
  const query = info.params.find((p) => p.kind === "query");
  const pathP = info.params.find((p) => p.kind === "path");
  const multipart = info.params.some((p) => p.kind === "multipart");
  const resp = normalizeResponse(info.ret);
  const reqKeys = json && json.type.arbitrary ? extractReqKeys(info.body, json.varName) : new Set();
  const queryKeys = query && query.type && query.type.arbitrary ? extractReqKeys(info.body, query.varName) : new Set();
  const respKeys = extractRespKeys(info.body);
  const pathParams = [...r.path.matchAll(/:([A-Za-z0-9_]+)/g)].map((x) => x[1]);
  resolved.push({
    ...r,
    site,
    info,
    json,
    query,
    pathP,
    multipart,
    resp,
    reqKeys,
    queryKeys,
    respKeys,
    pathParams,
  });
}

const api = resolved.filter((r) => r.path.startsWith("/api"));
const page = resolved.filter((r) => !r.path.startsWith("/api"));
api.forEach((r, i) => {
  r.no = i + 1;
});

// 结构体引用
const usedStructs = new Map(); // name -> {type, routes:[]}
for (const r of api) {
  for (const [which, p] of [
    ["body", r.json],
    ["query", r.query],
    ["response", r.resp],
  ]) {
    if (!p || !p.type || p.type.arbitrary) continue;
    if (which === "response" && p.type.display === "String") continue;
    const name = p.type.display;
    const entry = usedStructs.get(name) || { name, which: new Set(), routes: [] };
    entry.which.add(which);
    entry.routes.push(r.path);
    usedStructs.set(name, entry);
  }
}

// 统计
const typedBodies = api.filter((r) => r.json && !r.json.type.arbitrary).length;
const arbitraryBodies = api.filter((r) => r.json && r.json.type.arbitrary).length;
const withReqKeys = api.filter((r) => r.reqKeys.size > 0).length;
const withRespKeys = api.filter((r) => r.respKeys.keys.size > 0).length;
const arbitraryNoReq = api.filter((r) => r.json && r.json.type.arbitrary && r.reqKeys.size === 0);
const structResolved = [...usedStructs.keys()].filter((n) => structIndex.has(n)).length;
const structUnresolved = [...usedStructs.keys()].filter((n) => !structIndex.has(n));

// 嵌套类型：只展开一层（字段类型里直接引用的 struct / enum）
const nestedStructs = new Map(); // name -> Set(parent)
const usedEnums = new Map(); // name -> Set(parent)
function scanFieldTypes(parentName, fields) {
  for (const f of fields || []) {
    for (const id of f.type.matchAll(/\b([A-Z][A-Za-z0-9_]*)\b/g)) {
      const n = id[1];
      if (structIndex.has(n) && !usedStructs.has(n)) {
        if (!nestedStructs.has(n)) nestedStructs.set(n, new Set());
        nestedStructs.get(n).add(parentName);
      }
      if (enumIndex.has(n)) {
        if (!usedEnums.has(n)) usedEnums.set(n, new Set());
        usedEnums.get(n).add(parentName);
      }
    }
  }
}
for (const [name] of usedStructs) {
  const def = structIndex.get(name);
  if (def) scanFieldTypes(name, def.fields);
}

const md = [];
md.push("# API 契约（自动生成）");
md.push("");
md.push(
  "> 本文件由 `scripts/gen_api_contracts.js` 从 `src/common.rs` 的路由声明与处理函数源码生成，**不要手改**。"
);
md.push(
  "> `tests/api_contracts_up_to_date.rs` 会用同一脚本的 `--check` 模式比对，代码改了没重跑就红。"
);
md.push("> 叙述性说明、示例与业务语义仍写在 [`docs/API.md`](API.md)。");
md.push("");
md.push(
  `共 **${api.length}** 条 \`/api\` 路由（另有 ${page.length} 条页面/静态路由，见 [API-ROUTES.md](API-ROUTES.md)）。`
);
md.push("");
md.push("## 怎么读");
md.push("");
md.push("- **路径参数**：取自路由模板里的 `:name`；处理函数用 `Path<T>` 接。");
md.push(
  "- **请求体 / 查询**：写出类型名；类型能在 `src/` 里找到 `struct` 定义时，下文按字段列出（含 serde 重命名与是否可省略）。"
);
md.push(
  "- **任意 JSON**：端点的类型是 `serde_json::Value`，字段没有类型约束。本文件从函数体里**静态抽取**它实际读取的键与返回的顶层键；"
);
md.push(
  "  经共享 helper 读取的键可能抽不到——抽不到会写明「未抽取到」，不拿「没写」冒充「没有」。"
);
md.push("- **响应**：能确定 `Json<T>` 的写类型；`Response`/`impl IntoResponse` 这类动态响应只标注形态，状态码与正文见源码行。");
md.push("");
md.push("## 概览");
md.push("");
md.push("| 维度 | 数量 |");
md.push("| --- | --- |");
md.push(`| \`/api\` 路由 | ${api.length} |`);
md.push(`| 请求体是具体结构体 | ${typedBodies} |`);
md.push(`| 请求体是任意 JSON | ${arbitraryBodies} |`);
md.push(`| 其中能从函数体抽到请求键 | ${withReqKeys} |`);
md.push(`| 响应含可识别 JSON 顶层键 | ${withRespKeys} |`);
md.push(`| 引用的请求/查询类型已解析到 struct | ${structResolved}/${usedStructs.size} |`);
md.push(`| 处理函数未找到 | ${unresolved.length} |`);
md.push("");

md.push("## 路由契约总表");
md.push("");
md.push("| # | 方法 | 路径 | 路径参数 | 请求体 | 查询 | 响应 | 处理函数 | 源码 |");
md.push("| --- | --- | --- | --- | --- | --- | --- | --- | --- |");
api.forEach((r) => {
  const body = r.multipart
    ? "`multipart/form-data`"
    : r.json
      ? r.json.type.arbitrary
        ? r.reqKeys.size
          ? `任意 JSON（§${r.no}）`
          : "任意 JSON"
        : "`" + r.json.type.display + "`"
      : "—";
  const query = r.query
    ? r.query.type && r.query.type.arbitrary
      ? r.queryKeys.size
        ? `任意键值对（§${r.no}）`
        : "任意键值对"
      : "`" + r.query.type.display + "`"
    : "—";
  let resp = r.resp.display;
  if (r.resp.type && r.resp.type.arbitrary) {
    resp = r.respKeys.keys.size ? `JSON（§${r.no}）` : "JSON（未抽到顶层键）";
  }
  const pp = r.pathParams.length ? r.pathParams.map((x) => "`" + x + "`").join(", ") : "—";
  const where = "`" + rel(r.site.file) + ":" + r.info.sigLine + "`";
  md.push(
    `| ${r.no} | ${r.methods.join(" / ")} | \`${r.path}\` | ${pp} | ${body} | ${query} | ${resp} | \`${r.handler}\` | ${where} |`
  );
});
md.push("");

md.push("## 请求 / 查询类型字段");
md.push("");
if (!usedStructs.size) {
  md.push("（无）");
  md.push("");
}
for (const [name, entry] of [...usedStructs.entries()].sort((a, b) => a[0].localeCompare(b[0]))) {
  const def = structIndex.get(name);
  if (!def) continue;
  const usedAs = [...entry.which]
    .map((w) => (w === "body" ? "请求体" : w === "query" ? "查询" : "响应"))
    .join(" / ");
  md.push(`### \`${name}\`（${usedAs}；定义于 \`${rel(def.file)}:${def.line}\`）`);
  md.push("");
  if (!def.fields.length) {
    md.push("> 该结构体没有可解析的命名字段（可能是 tuple struct 或解析失败），以源码定义为准。");
    md.push("");
    continue;
  }
  md.push("| 字段（JSON 名） | 类型 | 可省略 |");
  md.push("| --- | --- | --- |");
  for (const f of def.fields) {
    const jsonName = f.jsonName === f.name ? "`" + f.name + "`" : "`" + f.jsonName + "`（Rust: `" + f.name + "`）";
    md.push(`| ${jsonName} | \`${f.type.replace(/\|/g, "\\|")}\` | ${f.optional ? "是" : "否"} |`);
  }
  md.push("");
  md.push(`使用该类型的端点：${entry.routes.map((p) => "`" + p + "`").join("、")}`);
  md.push("");
}
if (structUnresolved.length) {
  md.push("### 未解析到定义的类型");
  md.push("");
  md.push(
    "以下类型名出现在处理函数签名里，但 `src/` 内没有以该名字定义的 `struct`（可能是别名、外部类型或解析遗漏）——**字段没有核对**："
  );
  md.push("");
  for (const n of structUnresolved) md.push(`- \`${n}\``);
  md.push("");
}

md.push("## 嵌套类型（只展开一层）");
md.push("");
md.push("上面的类型字段里直接引用的结构体与枚举。再深一层不展开，避免把整棵类型树都倒进来。");
md.push("");
for (const [name, parents] of [...nestedStructs.entries()].sort((a, b) => a[0].localeCompare(b[0]))) {
  const def = structIndex.get(name);
  if (!def) continue;
  md.push(
    `### \`${name}\`（struct；被 ${[...parents].map((p) => "`" + p + "`").join("、")} 引用；定义于 \`${rel(def.file)}:${def.line}\`）`
  );
  md.push("");
  md.push("| 字段（JSON 名） | 类型 | 可省略 |");
  md.push("| --- | --- | --- |");
  for (const f of def.fields) {
    const jsonName = f.jsonName === f.name ? "`" + f.name + "`" : "`" + f.jsonName + "`（Rust: `" + f.name + "`）";
    md.push(`| ${jsonName} | \`${f.type.replace(/\|/g, "\\|")}\` | ${f.optional ? "是" : "否"} |`);
  }
  md.push("");
}
for (const [name, parents] of [...usedEnums.entries()].sort((a, b) => a[0].localeCompare(b[0]))) {
  const def = enumIndex.get(name);
  if (!def) continue;
  const values = def.variants.map((v) =>
    v.jsonName === v.name ? "`" + v.name + "`" : "`" + v.jsonName + "`（Rust: `" + v.name + "`）"
  );
  md.push(
    `### \`${name}\`（enum；被 ${[...parents].map((p) => "`" + p + "`").join("、")} 引用；定义于 \`${rel(def.file)}:${def.line}\`）`
  );
  md.push("");
  md.push(`取值：${values.length ? values.join("、") : "未解析到变体"}`);
  md.push("");
}

md.push("## 字段抽取明细");
md.push("");
md.push(
  "每条路由只列**能从源码里直接看到**的东西：函数体内读取的键（`req[\"x\"]` / `req.get(\"x\")` / `get_*(&req, \"x\")`）"
);
md.push("与 `json!({...})` 返回的**顶层键**。嵌套结构、经共享 helper 读取的键、运行时拼接的键不在此列——抽不到就写「未抽取到」。");
md.push("");
for (const r of api) {
  const hasKeys = r.reqKeys.size || r.queryKeys.size || r.respKeys.keys.size;
  if (!hasKeys) continue;
  md.push(`### §${r.no} ${r.methods.join(" / ")} \`${r.path}\``);
  md.push("");
  const where = `\`${rel(r.site.file)}:${r.info.sigLine}\``;
  const req = r.reqKeys.size
    ? [...r.reqKeys].sort().map((k) => "`" + k + "`").join("、")
    : r.json
      ? "未抽取到（可能经共享 helper 读取）"
      : "（无请求体）";
  const q = r.queryKeys.size ? [...r.queryKeys].sort().map((k) => "`" + k + "`").join("、") : "";
  const resp = r.respKeys.keys.size
    ? [...r.respKeys.keys].sort().map((k) => "`" + k + "`").join("、")
    : "未抽取到（动态构造或非 JSON 响应）";
  const st = r.respKeys.statuses.size ? [...r.respKeys.statuses].sort().join(" / ") : "";
  md.push(`- 源码：${where}`);
  if (r.json && !r.json.type.arbitrary) {
    md.push(`- 请求体：\`${r.json.type.display}\`（字段见上文类型表）`);
  } else if (r.json) {
    md.push(`- 读取的请求键：${req}`);
  }
  if (q) md.push(`- 读取的查询键：${q}`);
  md.push(`- 响应顶层键：${resp}`);
  if (st) md.push(`- \`status\` 取值：${st}`);
  md.push("");
}

if (unresolved.length) {
  md.push("## 未找到处理函数的声明");
  md.push("");
  md.push("| 方法 | 路径 | 处理函数 |");
  md.push("| --- | --- | --- |");
  for (const r of unresolved) md.push(`| ${r.methods.join(" / ")} | \`${r.path}\` | \`${r.handler}\` |`);
  md.push("");
}

md.push("## 已知缺口");
md.push("");
md.push(
  `- ${arbitraryNoReq.length} 条任意 JSON 端点**未抽取到请求键**（处理器可能完全不用请求体字段，或经共享 helper/嵌套结构读取）。`
);
md.push(
  "- 响应只列顶层键：嵌套对象的内部键、数组元素结构、以及动态拼接的键未展开。"
);
md.push("- 不覆盖：鉴权要求、速率限制、错误码语义。这些属于运行时行为，见 `docs/API.md` 与对应源码。");
md.push("");

const content = md.join("\n");
const isCheck = process.argv.includes("--check");
if (isCheck) {
  const disk = fs.existsSync(OUT) ? fs.readFileSync(OUT, "utf8") : "";
  const norm = (s) => s.replace(/\r\n/g, "\n").trimEnd();
  if (norm(disk) !== norm(content)) {
    // 给出第一处差异，别让人对着整份文件找
    const a = norm(disk).split("\n");
    const b = norm(content).split("\n");
    let i = 0;
    while (i < a.length && i < b.length && a[i] === b[i]) i++;
    if (i >= a.length || i >= b.length) {
      // 长度不同：多出的那几行才是差异，别只报两个 <EOF>
      const docLonger = a.length > b.length;
      const extra = docLonger ? a.slice(i) : b.slice(i);
      const firstMeaningful = extra.find((l) => l.trim() !== "") || extra[0] || "";
      console.error(
        `docs/API-CONTRACTS.md 与代码不一致（第 ${i + 1} 行起）：文档${docLonger ? "多出" : "缺少"} ${extra.length} 行。\n` +
          `  ${docLonger ? "文档多出" : "代码多出"}的第一行：${JSON.stringify(firstMeaningful.slice(0, 160))}\n` +
          `  重跑：node scripts/gen_api_contracts.js`
      );
      process.exit(1);
    }
    console.error(
      `docs/API-CONTRACTS.md 与代码不一致（第 ${i + 1} 行起）。\n` +
        `  文档：${JSON.stringify(a[i].slice(0, 160))}\n` +
        `  代码：${JSON.stringify(b[i].slice(0, 160))}\n` +
        `  重跑：node scripts/gen_api_contracts.js`
    );
    process.exit(1);
  }
  console.log(`ok: ${api.length} api routes, contracts in sync`);
} else {
  fs.writeFileSync(OUT, content);
  console.log(
    `api=${api.length} page=${page.length} typed_body=${typedBodies} arbitrary_body=${arbitraryBodies} ` +
      `with_req_keys=${withReqKeys} structs=${structResolved}/${usedStructs.size} unresolved_handler=${unresolved.length} -> ${rel(OUT)}`
  );
}
