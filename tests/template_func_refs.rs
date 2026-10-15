//! 模板函数引用完整性检查 / Template function reference integrity check
//!
//! 防止 switchOaPanelTab 空壳事件再次出现：扫描 templates/*.html 中所有
//! on* 属性里调用的函数名，断言同文件内存在对应声明。

use std::fs;
use std::path::PathBuf;

/// JavaScript 关键字与 DOM 原生方法，不检查这些
const BUILTINS: &[&str] = &[
    "if",
    "for",
    "while",
    "return",
    "switch",
    "function",
    "catch",
    "typeof",
    "document",
    "window",
    "this",
    "new",
    "var",
    "let",
    "const",
    "true",
    "false",
    "null",
    "undefined",
    "void",
    "delete",
    "in",
    "of",
    "instanceof",
    "else",
    "do",
    "break",
    "continue",
    "throw",
    "try",
    "finally",
    "await",
    "yield",
    "class",
    "extends",
    "super",
    "import",
    "export",
    "default",
    "from",
    "as",
    "static",
    "get",
    "set",
    "async",
    "with",
    "debugger",
    "eval",
    "parseInt",
    "parseFloat",
    "isNaN",
    "JSON",
    "Math",
    "console",
    "alert",
    "confirm",
    "prompt",
    "String",
    "Number",
    "Array",
    "Object",
    "Boolean",
    "Date",
    "RegExp",
    "Error",
    "Promise",
    "Map",
    "Set",
    "Symbol",
    "encodeURIComponent",
    "decodeURIComponent",
    "encodeURI",
    "decodeURI",
    "setTimeout",
    "setInterval",
    "clearTimeout",
    "clearInterval",
    "requestAnimationFrame",
    "fetch",
    "navigator",
    "location",
    "history",
    "localStorage",
    "sessionStorage",
    "performance",
    "event",
    "arguments",
    "self",
    "globalThis",
    "NaN",
    "Infinity",
    // DOM methods
    "preventDefault",
    "stopPropagation",
    "stopImmediatePropagation",
    "getElementById",
    "getElementsByClassName",
    "getElementsByTagName",
    "querySelector",
    "querySelectorAll",
    "createElement",
    "createTextNode",
    "appendChild",
    "removeChild",
    "insertBefore",
    "replaceChild",
    "setAttribute",
    "getAttribute",
    "removeAttribute",
    "hasAttribute",
    "click",
    "focus",
    "blur",
    "select",
    "submit",
    "reset",
    "scrollIntoView",
    "scrollTo",
    "scroll",
    "open",
    "close",
    "print",
    "stop",
    "back",
    "forward",
    "go",
    "reload",
    "replace",
    "assign",
    "toString",
    "valueOf",
    "toFixed",
    "split",
    "join",
    "slice",
    "splice",
    "concat",
    "push",
    "pop",
    "shift",
    "unshift",
    "sort",
    "reverse",
    "indexOf",
    "lastIndexOf",
    "find",
    "findIndex",
    "filter",
    "map",
    "reduce",
    "forEach",
    "some",
    "every",
    "includes",
    "startsWith",
    "endsWith",
    "contains",
    "trim",
    "padStart",
    "padEnd",
    "repeat",
    "substring",
    "substr",
    "charAt",
    "charCodeAt",
    "fromCharCode",
    "toLowerCase",
    "toUpperCase",
    "match",
    "search",
    "replaceAll",
    "keys",
    "values",
    "entries",
    "freeze",
    "seal",
    "create",
    "parse",
    "stringify",
    "round",
    "floor",
    "ceil",
    "abs",
    "min",
    "max",
    "pow",
    "sqrt",
    "random",
    "log",
    "info",
    "warn",
    "error",
    "debug",
    "trace",
    "time",
    "timeEnd",
    "now",
    "getTime",
    "getFullYear",
    "getMonth",
    "getDate",
    "getHours",
    "getMinutes",
    "getSeconds",
    "toISOString",
    "toLocaleString",
    "resolve",
    "reject",
    "all",
    "race",
    "then",
    "catch",
    "bind",
    "call",
    "apply",
    "from",
    "isArray",
    "of",
    "add",
    "remove",
    "toggle",
    "has",
    "size",
    "next",
    "done",
    "iterator",
    "length",
    "prototype",
    "constructor",
    "name",
    "message",
    "stack",
    "code",
    "closest",
    "matches",
    "insertAdjacentHTML",
    "insertAdjacentText",
    "requestFullscreen",
    "exitFullscreen",
    "createObjectURL",
    "revokeObjectURL",
    "readAsText",
    "readAsArrayBuffer",
    "readAsDataURL",
    "abort",
    "result",
    "send",
    "setRequestHeader",
    "getAllResponseHeaders",
    "getResponseHeader",
    "overrideMimeType",
    "URL",
    "URLSearchParams",
    "Headers",
    "FormData",
    "Blob",
    "File",
    "FileReader",
    "CustomEvent",
    "Event",
    "EventTarget",
    "structuredClone",
    "queueMicrotask",
    "atob",
    "btoa",
    "crypto",
    "subtle",
    "digest",
    "encrypt",
    "decrypt",
    "sign",
    "verify",
    "generateKey",
    "getRandomValues",
    "randomUUID",
    "observe",
    "unobserve",
    "disconnect",
    "takeRecords",
    "isIntersecting",
    "intersectionRatio",
    "target",
    "boundingClientRect",
    "relatedTarget",
    "deltaX",
    "deltaY",
    "deltaMode",
    "key",
    "code",
    "location",
    "repeat",
    "composed",
    "detail",
    "oldValue",
    "newValue",
    "propertyName",
    "elapsedTime",
    "pseudoElement",
    "animationName",
    "loaded",
    "total",
    "lengthComputable",
];

/// 跨文件白名單：在模板 A 中引用、在模板 B 中聲明的全局函數名。
/// 目前為空——所有 on* 引用的函數都在同文件內聲明。
const CROSS_FILE_WHITELIST: &[&str] = &[];

#[test]
fn template_onclick_functions_must_be_declared() {
    let templates_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("templates");

    let html_files: Vec<_> = fs::read_dir(&templates_dir)
        .expect("templates/ directory not found")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "html"))
        .collect();

    assert!(!html_files.is_empty(), "No .html files found in templates/");

    let builtins: std::collections::HashSet<&str> = BUILTINS.iter().copied().collect();
    let whitelist: std::collections::HashSet<&str> = CROSS_FILE_WHITELIST.iter().copied().collect();

    let mut failures: Vec<String> = Vec::new();

    // Pre-compile regexes outside the loop (clippy::regex_creation_in_loops)
    let re_function = regex::Regex::new(r"\bfunction\s+(\w+)\s*\(").unwrap();
    let re_const = regex::Regex::new(r"\b(?:const|let|var)\s+(\w+)\s*=").unwrap();
    let re_window = regex::Regex::new(r"\bwindow\.(\w+)\s*=").unwrap();
    let on_attr_re = regex::Regex::new(
        r#"on(?:click|change|input|keyup|keydown|submit|load|focus|blur)="([^"]*)""#,
    )
    .unwrap();
    let call_re = regex::Regex::new(r"(\w+)\s*\(").unwrap();
    let inline_re = regex::Regex::new(r#"onclick=['"]?(\w+)\s*\("#).unwrap();

    for entry in &html_files {
        let path = entry.path();
        let filename = path.file_name().unwrap().to_string_lossy().to_string();
        let content = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("Failed to read {}: {}", filename, e));

        // Collect declarations in this file
        let mut declared: std::collections::HashSet<String> = std::collections::HashSet::new();

        // function foo(
        for cap in re_function.captures_iter(&content) {
            declared.insert(cap[1].to_string());
        }
        // const/let/var foo =
        for cap in re_const.captures_iter(&content) {
            declared.insert(cap[1].to_string());
        }
        // window.foo =
        for cap in re_window.captures_iter(&content) {
            declared.insert(cap[1].to_string());
        }

        // Collect function calls from on* attributes
        let mut referenced: std::collections::HashSet<String> = std::collections::HashSet::new();

        for cap in on_attr_re.captures_iter(&content) {
            let attr_value = &cap[1];
            for call_cap in call_re.captures_iter(attr_value) {
                referenced.insert(call_cap[1].to_string());
            }
        }

        // Also check onclick=foo( patterns in template strings (without quotes)
        for cap in inline_re.captures_iter(&content) {
            referenced.insert(cap[1].to_string());
        }

        // Check each referenced function is declared
        for func_name in &referenced {
            if builtins.contains(func_name.as_str()) {
                continue;
            }
            if whitelist.contains(func_name.as_str()) {
                continue;
            }
            if declared.contains(func_name) {
                continue;
            }
            failures.push(format!(
                "  {} : on* attribute calls '{}' but no function/const/window. declaration found in same file",
                filename, func_name
            ));
        }
    }

    if !failures.is_empty() {
        panic!(
            "Template function reference check failed ({} issues):\n{}",
            failures.len(),
            failures.join("\n")
        );
    }
}
