// 生成 docs/API-ROUTES.md：从 src/common.rs 的 .route(...) 声明里抽出全部路由。
// 目的是让"路由很多、文档很少"这件事**在仓库里可见且不会继续漂移**（#62/#67）。
// 由 tests/route_inventory_up_to_date.rs 守卫：清单与代码不一致就红。
//
// 解析方式：按括号配对扫描（跳过字符串字面量），因此单行/多行两种写法都能吃到。
const fs = require("fs");
const path = require("path");

const ROOT = path.resolve(__dirname, "..");
const SRC = path.join(ROOT, "src", "common.rs");
const OUT = path.join(ROOT, "docs", "API-ROUTES.md");
const METHODS = ["get", "post", "put", "delete", "patch", "head", "options"];

const HANDLER_RE = new RegExp(
  "\\b(?:get|post|put|delete|patch|head|options)\\s*\\(\\s*([A-Za-z_][A-Za-z0-9_:]*)"
);
const PATH_RE = /\.route\(\s*"([^"]+)"/;

const src = fs.readFileSync(SRC, "utf8");

function lineOf(offset) {
  let line = 1;
  for (let i = 0; i < offset; i++) if (src[i] === "\n") line++;
  return line;
}

function extractRoutes(text) {
  const out = [];
  let from = 0;
  for (;;) {
    const start = text.indexOf(".route(", from);
    if (start === -1) break;
    from = start + 1;
    // 找到与 .route( 配对的右括号（跳过字符串）
    let depth = 0;
    let inStr = false;
    let esc = false;
    let end = start;
    for (let j = start + 6; j < text.length; j++) {
      const c = text[j];
      if (inStr) {
        if (esc) esc = false;
        else if (c === "\\") esc = true;
        else if (c === '"') inStr = false;
        continue;
      }
      if (c === '"') {
        inStr = true;
        continue;
      }
      if (c === "(") depth++;
      else if (c === ")") {
        depth--;
        if (depth === 0) {
          end = j;
          break;
        }
      }
    }
    const seg = text.slice(start, end + 1);
    const pm = seg.match(PATH_RE);
    if (!pm) continue;
    const routePath = pm[1];
    const tail = seg.slice(pm.index + pm[0].length);
    const methods = [];
    for (const m of METHODS) {
      if (new RegExp("\\b" + m + "\\s*\\(").test(tail)) methods.push(m.toUpperCase());
    }
    // 处理函数不一定是 `routes::x`：也可能是 `crate::a::b` 或裸函数名。
    const handler = (seg.match(HANDLER_RE) || [])[1] || "";
    out.push({
      path: routePath,
      methods: methods.length ? methods : ["?"],
      handler,
      line: lineOf(start),
    });
  }
  return out;
}

const routes = extractRoutes(src);
const isApi = (p) => p.startsWith("/api");
const apiCount = routes.filter((r) => isApi(r.path)).length;
const pageCount = routes.length - apiCount;

const md = [];
md.push("# 路由清单（自动生成）");
md.push("");
md.push("> 本文件由 `scripts/gen_route_inventory.js` 从 `src/common.rs` 的 `.route(...)` 声明生成，**不要手改**。");
md.push("> `tests/route_inventory_up_to_date.rs` 会在清单与代码不一致时失败——想改路由就重跑生成脚本：");
md.push("> `node scripts/gen_route_inventory.js`");
md.push("");
md.push(`共 **${routes.length}** 条路由：页面/静态 ${pageCount} 条，\`/api\` 接口 ${apiCount} 条。`);
md.push("");
md.push("## 说明：清单 ≠ 逐条文档");
md.push("");
md.push("清单只保证「有哪些端点、处理函数是哪个、在 common.rs 第几行」。逐条的参数/请求体/返回结构仍写在 `docs/API.md`；");
md.push("两者的差距是**已知且可见**的（#62/#67），不再靠「没人统计」维持体面。要补哪一条，按下面的表认领即可。");
md.push("");
md.push("| # | 方法 | 路径 | 处理函数 | 源码位置 |");
md.push("| --- | --- | --- | --- | --- |");
routes.forEach((r, i) => {
  md.push(
    `| ${i + 1} | ${r.methods.join(" / ")} | \`${r.path}\` | \`${r.handler}\` | src/common.rs:${r.line} |`
  );
});
md.push("");

fs.writeFileSync(OUT, md.join("\n"));
console.log(`routes=${routes.length} api=${apiCount} page=${pageCount} -> ${path.relative(ROOT, OUT)}`);
