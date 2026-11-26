#!/usr/bin/env python3
"""
InnoForge 轻量质量回归评测脚手架（Eval Harness）

定位：
- 用公开/脱敏固定考题量化评估 InnoForge 的检索召回相关度、响应耗时、CRAAP 评分覆盖、
  HTML 实体零泄漏以及（可选）AI 对比的结构完整度与防幻觉表现；
- 支持 `--save-baseline` 保存基线，以及 `--compare` 自动输出前后对比分并在发生回归时
  返回非零退出码，作为后续自进化迭代的量化硬标尺。

用法示例：
  python3 tools/eval_suite.py --self-test
  python3 tools/eval_suite.py --base-url http://127.0.0.1:3001 --mode fast
  python3 tools/eval_suite.py --base-url http://127.0.0.1:3001 --mode fast --save-baseline /mnt/d/TEMP/eval-base.json
  python3 tools/eval_suite.py --base-url http://127.0.0.1:3001 --mode fast --compare /mnt/d/TEMP/eval-base.json
"""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
import time
from pathlib import Path
from typing import Any, Dict, List, Optional, Tuple
from urllib import error, request

RAW_HTML_ENTITY_RE = re.compile(r"&(?:amp|lt|gt|quot|#39|#x27);")
PATENT_NUM_RE = re.compile(r"\b(?:CN|US|EP|WO|JP|KR)\s*\d{6,13}\s*[A-Z]\d?\b", re.I)


def http_post_json(
    url: str,
    payload: Dict[str, Any],
    timeout: float = 60.0,
    access_token: str = "",
) -> Tuple[int, Dict[str, Any], int]:
    data = json.dumps(payload, ensure_ascii=False).encode("utf-8")
    headers = {"Content-Type": "application/json"}
    if access_token:
        headers["X-InnoForge-Token"] = access_token
    req = request.Request(
        url,
        data=data,
        headers=headers,
        method="POST",
    )
    opener = request.build_opener(request.ProxyHandler({}))
    t0 = time.perf_counter()
    try:
        with opener.open(req, timeout=timeout) as resp:
            elapsed_ms = int((time.perf_counter() - t0) * 1000)
            body = resp.read().decode("utf-8", errors="replace")
            return resp.status, json.loads(body), elapsed_ms
    except error.HTTPError as e:
        elapsed_ms = int((time.perf_counter() - t0) * 1000)
        body = e.read().decode("utf-8", errors="replace")
        try:
            parsed = json.loads(body)
        except Exception:
            parsed = {"error": body[:300]}
        return e.code, parsed, elapsed_ms
    except Exception as e:
        elapsed_ms = int((time.perf_counter() - t0) * 1000)
        return 0, {"error": str(e)}, elapsed_ms


def grade_search_case(
    case: Dict[str, Any],
    http_status: int,
    resp: Dict[str, Any],
    latency_ms: int,
) -> Dict[str, Any]:
    patents = resp.get("patents") or []
    hits = len(patents)
    min_hits = int(case.get("min_hits", 1))
    max_latency_ms = int(case.get("max_latency_ms", 20000))
    expected_terms = [t.lower() for t in (case.get("expected_terms") or [])]
    expected_ipcs = [p.upper() for p in (case.get("expected_ipc_prefixes") or [])]
    require_craap = bool(case.get("require_craap", False))

    if http_status != 200:
        return {
            "id": case["id"],
            "name": case["name"],
            "passed": False,
            "score": 0.0,
            "http_status": http_status,
            "latency_ms": latency_ms,
            "hits": 0,
            "relevance_rate": 0.0,
            "html_entity_leaks": 0,
            "notes": [f"HTTP {http_status}: {resp.get('error', 'request failed')}"],
        }

    relevant_hits = 0
    entity_leaks = 0
    for item in patents[:10]:
        title = str(item.get("title") or "")
        abstract_text = str(item.get("abstract_text") or "")
        blob = f"{title} {abstract_text}".lower()
        if RAW_HTML_ENTITY_RE.search(title) or RAW_HTML_ENTITY_RE.search(abstract_text):
            entity_leaks += 1
        if not expected_terms or any(term in blob for term in expected_terms):
            relevant_hits += 1

    relevance_rate = (relevant_hits / min(hits, 10)) if hits > 0 else (1.0 if min_hits == 0 else 0.0)

    # 子项打分（满分 100）：命中数(35) + 领域相关度(35) + 延迟预算(15) + 附带质量(CRAAP/IPC/无HTML实体)(15)
    score = 0.0
    notes: List[str] = []

    if hits >= min_hits:
        score += 35.0
    else:
        notes.append(f"命中不足: {hits} < {min_hits}")

    score += round(35.0 * relevance_rate, 1)
    if hits > 0 and relevance_rate < 0.5:
        notes.append(f"领域相关度偏低: {relevance_rate:.0%}")

    if latency_ms <= max_latency_ms:
        score += 15.0
    else:
        ratio = max(0.0, 1.0 - (latency_ms - max_latency_ms) / max(max_latency_ms, 1))
        score += round(15.0 * ratio, 1)
        notes.append(f"耗时超预算: {latency_ms}ms > {max_latency_ms}ms")

    quality_pts = 15.0
    if entity_leaks > 0:
        quality_pts -= 5.0
        notes.append(f"检测到 {entity_leaks} 条未解码 HTML 实体")

    craap_scores = resp.get("craap_scores") or {}
    if require_craap and hits > 0 and not craap_scores:
        quality_pts -= 5.0
        notes.append("缺失 craap_scores 质量互证评分")

    if expected_ipcs:
        anchors = resp.get("ipc_anchors") or []
        anchor_codes = [str(a.get("code") or "").upper() for a in anchors]
        if not any(any(code.startswith(pref) for pref in expected_ipcs) for code in anchor_codes):
            quality_pts -= 5.0
            notes.append(f"IPC 锚点未命中预期家族 {expected_ipcs} (实得 {anchor_codes})")

    score += max(0.0, quality_pts)
    score = round(min(100.0, score), 1)
    passed = (hits >= min_hits) and (entity_leaks == 0) and (score >= 70.0)

    return {
        "id": case["id"],
        "name": case["name"],
        "passed": passed,
        "score": score,
        "http_status": http_status,
        "latency_ms": latency_ms,
        "hits": hits,
        "relevance_rate": round(relevance_rate, 3),
        "html_entity_leaks": entity_leaks,
        "source": resp.get("source") or resp.get("scope") or "",
        "notes": notes,
    }


def grade_ai_compare_case(
    case: Dict[str, Any],
    http_status: int,
    resp: Dict[str, Any],
    latency_ms: int,
) -> Dict[str, Any]:
    field = case.get("response_field")
    if field:
        text = str(resp.get(field) or "")
    else:
        text = str(resp.get("content") or resp.get("analysis") or resp.get("preview") or "")
    if http_status != 200 or not text.strip():
        return {
            "id": case["id"],
            "name": case["name"],
            "passed": False,
            "score": 0.0,
            "http_status": http_status,
            "latency_ms": latency_ms,
            "notes": [f"HTTP {http_status}: {resp.get('error', 'empty AI response')}"],
        }

    signals = case.get("required_signals") or []
    matched_signals = [s for s in signals if s.lower() in text.lower()]
    signal_rate = len(matched_signals) / len(signals) if signals else 1.0

    # 防幻觉核验：合成输入中没有任何专利号，若 AI 凭空捏造 CN/US/EP 专利号则扣分
    fabricated: List[str] = []
    if case.get("forbid_fabricated_patents"):
        input_blob = json.dumps(case.get("payload", {}), ensure_ascii=False)
        input_pns = {m.replace(" ", "").upper() for m in PATENT_NUM_RE.findall(input_blob)}
        output_pns = {m.replace(" ", "").upper() for m in PATENT_NUM_RE.findall(text)}
        fabricated = sorted(output_pns - input_pns)

    score = round(60.0 * signal_rate, 1)
    notes: List[str] = []
    if len(text) >= 200:
        score += 15.0
    else:
        notes.append(f"输出过短 ({len(text)} 字)")
    if not fabricated:
        score += 25.0
    else:
        notes.append(f"疑似编造未输入专利号: {fabricated[:5]}")

    passed = (signal_rate >= 0.6) and (not fabricated) and (score >= 75.0)
    return {
        "id": case["id"],
        "name": case["name"],
        "passed": passed,
        "score": round(min(100.0, score), 1),
        "http_status": http_status,
        "latency_ms": latency_ms,
        "signal_rate": round(signal_rate, 3),
        "fabricated_patents": fabricated,
        "output_chars": len(text),
        "notes": notes,
    }


def grade_idea_intent_case(
    case: Dict[str, Any],
    http_status: int,
    resp: Dict[str, Any],
    latency_ms: int,
) -> Dict[str, Any]:
    idea = resp.get("idea") or {}
    actual_intent = str(idea.get("query_intent") or "")
    expected_intent = str(case.get("expected_intent") or "")
    analysis = str(idea.get("analysis") or "")
    expected_signals = case.get("expected_analysis_signals") or []
    max_latency_ms = int(case.get("max_latency_ms", 30000))

    if http_status != 200 or resp.get("status") != "ok":
        return {
            "id": case["id"],
            "name": case["name"],
            "passed": False,
            "score": 0.0,
            "http_status": http_status,
            "latency_ms": latency_ms,
            "output_chars": len(analysis),
            "notes": [f"HTTP {http_status}: {resp.get('message') or resp.get('error') or 'analyze failed'}"],
        }

    score = 0.0
    notes: List[str] = []
    if actual_intent == expected_intent:
        score += 50.0
    else:
        notes.append(f"意图不匹配: 期望 {expected_intent}, 实得 {actual_intent}")

    matched = [s for s in expected_signals if s in analysis]
    sig_rate = len(matched) / len(expected_signals) if expected_signals else 1.0
    score += round(35.0 * sig_rate, 1)
    if sig_rate < 1.0:
        notes.append(f"报告信号缺失: 命中 {matched} / {expected_signals}")

    if latency_ms <= max_latency_ms:
        score += 15.0
    else:
        notes.append(f"耗时超预算: {latency_ms}ms > {max_latency_ms}ms")

    score = round(min(100.0, score), 1)
    passed = (actual_intent == expected_intent) and (sig_rate >= 0.5) and (score >= 80.0)
    return {
        "id": case["id"],
        "name": case["name"],
        "passed": passed,
        "score": score,
        "http_status": http_status,
        "latency_ms": latency_ms,
        "query_intent": actual_intent,
        "output_chars": len(analysis),
        "notes": notes,
    }


def compare_with_baseline(
    current: Dict[str, Any],
    baseline: Dict[str, Any],
    max_score_drop: float = 5.0,
) -> Tuple[bool, List[str]]:
    lines: List[str] = []
    regressed = False
    cur_avg = float(current.get("avg_score", 0.0))
    base_avg = float(baseline.get("avg_score", 0.0))
    delta_avg = round(cur_avg - base_avg, 1)
    lines.append(
        f"总分对比: {base_avg:.1f} -> {cur_avg:.1f} ({delta_avg:+.1f}) | "
        f"平均耗时: {baseline.get('avg_latency_ms', 0)}ms -> {current.get('avg_latency_ms', 0)}ms"
    )
    if cur_avg + max_score_drop < base_avg:
        regressed = True
        lines.append(f"[REGRESSION] 平均分下降超过阈值 {max_score_drop} 分")

    base_map = {r["id"]: r for r in baseline.get("results", [])}
    for r in current.get("results", []):
        b = base_map.get(r["id"])
        if not b:
            continue
        ds = round(float(r["score"]) - float(b["score"]), 1)
        dl = int(r["latency_ms"]) - int(b["latency_ms"])
        lines.append(
            f"  - {r['id']}: score {b['score']} -> {r['score']} ({ds:+.1f}), "
            f"latency {b['latency_ms']}ms -> {r['latency_ms']}ms ({dl:+d}ms)"
        )
        if b.get("passed") and not r.get("passed"):
            regressed = True
            lines.append(f"    [REGRESSION] 用例 {r['id']} 从通过退化为失败")
    return not regressed, lines


def format_markdown_report(report: Dict[str, Any]) -> str:
    lines = [
        f"# InnoForge Quality Eval Report ({report['mode']})",
        "",
        f"- **Target**: `{report['base_url']}`",
        f"- **Timestamp**: `{report['timestamp']}`",
        f"- **Summary**: `{report['passed_cases']}/{report['total_cases']}` passed | "
        f"Avg Score **{report['avg_score']:.1f}/100** | Avg Latency **{report['avg_latency_ms']}ms**",
        "",
        "| Case | Score | Latency | Hits / Chars | Status | Notes |",
        "| :--- | :---: | :---: | :---: | :---: | :--- |",
    ]
    for r in report["results"]:
        metric = (
            f"{r.get('hits', 0)} hits ({r.get('relevance_rate', 0):.0%})"
            if "hits" in r
            else f"{r.get('output_chars', 0)} chars"
        )
        status = "✅ PASS" if r["passed"] else "❌ FAIL"
        note_str = "; ".join(r.get("notes") or []) or "—"
        lines.append(
            f"| `{r['id']}` {r['name']} | {r['score']:.1f} | {r['latency_ms']}ms | {metric} | {status} | {note_str} |"
        )
    return "\n".join(lines) + "\n"


def run_self_test() -> int:
    # 1. 检索高分用例
    case = {
        "id": "t1",
        "name": "test search",
        "min_hits": 2,
        "max_latency_ms": 5000,
        "expected_terms": ["sic", "cooling"],
        "expected_ipc_prefixes": ["H01L"],
        "require_craap": True,
    }
    resp_ok = {
        "source": "serpapi",
        "patents": [
            {"title": "SiC power module cooling", "abstract_text": "microchannel"},
            {"title": "Double-sided SiC cooling", "abstract_text": "thermal"},
        ],
        "craap_scores": {"1": {"total": 88}},
        "ipc_anchors": [{"code": "H01L23"}],
    }
    g1 = grade_search_case(case, 200, resp_ok, 1200)
    assert g1["passed"] and g1["score"] == 100.0, f"expected 100, got {g1}"

    # 2. 检出 HTML 实体泄漏与延迟超标
    resp_bad = {
        "source": "serpapi",
        "patents": [{"title": "SiC &amp; Cooling", "abstract_text": "ok"}],
        "craap_scores": {},
        "ipc_anchors": [],
    }
    g2 = grade_search_case(case, 200, resp_bad, 12000)
    assert not g2["passed"] and g2["html_entity_leaks"] == 1, f"expected fail, got {g2}"

    # 3. AI 幻觉专利号拦截
    ai_case = {
        "id": "ai1",
        "name": "ai test",
        "payload": {"items": [{"content": "双面微针翅散热与单面液冷对比，热阻差异明显"}]},
        "required_signals": ["双面", "单面", "热阻"],
        "forbid_fabricated_patents": True,
    }
    ai_hallucinated = grade_ai_compare_case(
        ai_case,
        200,
        {"content": "双面与单面方案的热阻差异显著，参考对比文件 CN119999999A 所记载结构..." + "补充说明" * 60},
        3000,
    )
    assert not ai_hallucinated["passed"] and "CN119999999A" in ai_hallucinated["fabricated_patents"]

    # 4. 基线退步检测
    ok, _ = compare_with_baseline(
        {"avg_score": 80.0, "avg_latency_ms": 2000, "results": [g2]},
        {"avg_score": 95.0, "avg_latency_ms": 1500, "results": [g1]},
    )
    assert not ok, "regression should fail baseline gate"

    # 5. 开放式调研意图路由评分自测
    intent_case = {
        "id": "intent1",
        "name": "survey intent",
        "expected_intent": "market_landscape_survey",
        "expected_analysis_signals": ["行业与产品谱系调研模式"],
        "max_latency_ms": 10000,
    }
    gi = grade_idea_intent_case(
        intent_case,
        200,
        {
            "status": "ok",
            "idea": {
                "query_intent": "market_landscape_survey",
                "analysis": "## 快速调研报告（行业与产品谱系调研模式）",
            },
        },
        1500,
    )
    assert gi["passed"] and gi["score"] == 100.0, f"expected 100, got {gi}"
    print("eval_suite.py self-test: ALL PASSED")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description="InnoForge 轻量质量回归评测脚手架（Eval Harness）")
    ap.add_argument("--base-url", default="http://127.0.0.1:3001", help="InnoForge 服务地址")
    ap.add_argument(
        "--token",
        default=os.environ.get("INNOFORGE_ACCESS_TOKEN", ""),
        help="非回环服务的 X-InnoForge-Token 访问令牌（可选，默认读环境变量 INNOFORGE_ACCESS_TOKEN）",
    )
    ap.add_argument(
        "--cases",
        default=str(Path(__file__).with_name("eval_cases.json")),
        help="评测考题 JSON 路径",
    )
    ap.add_argument("--mode", choices=["fast", "full"], default="fast", help="fast=检索质量; full=检索+AI对比+OA预判+调研意图")
    ap.add_argument("--save-baseline", default="", help="将本次评测结果保存为 JSON 基线文件")
    ap.add_argument("--compare", default="", help="与历史基线 JSON 对比，退步时返回非零退出码")
    ap.add_argument("--output-md", default="", help="导出 Markdown 评测报告路径")
    ap.add_argument("--self-test", action="store_true", help="离线自测评分器与回归对比逻辑")
    args = ap.parse_args()

    if args.self_test:
        return run_self_test()

    cases_data = json.loads(Path(args.cases).read_text(encoding="utf-8"))
    base = args.base_url.rstrip("/")
    tok = args.token.strip()
    results: List[Dict[str, Any]] = []

    for case in cases_data.get("search_cases", []):
        status, resp, ms = http_post_json(f"{base}{case['endpoint']}", case["payload"], timeout=45.0, access_token=tok)
        res = grade_search_case(case, status, resp, ms)
        results.append(res)
        flag = "PASS" if res["passed"] else "FAIL"
        print(
            f"[{flag}] {res['id']:32s} score={res['score']:5.1f} latency={res['latency_ms']:5d}ms "
            f"hits={res['hits']:2d} rel={res['relevance_rate']:.0%} {('; '.join(res['notes']) if res['notes'] else '')}"
        )

    if args.mode == "full":
        for group_key in ("ai_compare_cases", "ai_oa_cases"):
            for case in cases_data.get(group_key, []):
                status, resp, ms = http_post_json(
                    f"{base}{case['endpoint']}", case["payload"], timeout=90.0, access_token=tok
                )
                res = grade_ai_compare_case(case, status, resp, ms)
                results.append(res)
                flag = "PASS" if res["passed"] else "FAIL"
                print(
                    f"[{flag}] {res['id']:32s} score={res['score']:5.1f} latency={res['latency_ms']:5d}ms "
                    f"chars={res.get('output_chars', 0)} {('; '.join(res['notes']) if res['notes'] else '')}"
                )

        for case in cases_data.get("idea_intent_cases", []):
            sub_status, sub_resp, _ = http_post_json(
                f"{base}/api/idea/submit",
                {"title": case["title"], "description": case["description"]},
                timeout=20.0,
                access_token=tok,
            )
            idea_id = str(sub_resp.get("id") or (sub_resp.get("idea") or {}).get("id") or "")
            if sub_status != 200 or not idea_id:
                res = grade_idea_intent_case(case, sub_status, sub_resp, 0)
            else:
                try:
                    st, resp, ms = http_post_json(
                        f"{base}/api/idea/analyze",
                        {"id": idea_id},
                        timeout=60.0,
                        access_token=tok,
                    )
                    res = grade_idea_intent_case(case, st, resp, ms)
                finally:
                    # 评测用临时脱敏记录立即清理，严禁污染用户真实项目库
                    http_post_json(f"{base}/api/idea/{idea_id}/delete", {}, timeout=15.0, access_token=tok)
            results.append(res)
            flag = "PASS" if res["passed"] else "FAIL"
            print(
                f"[{flag}] {res['id']:32s} score={res['score']:5.1f} latency={res['latency_ms']:5d}ms "
                f"intent={res.get('query_intent', '')} {('; '.join(res['notes']) if res['notes'] else '')}"
            )

    total = len(results)
    passed_count = sum(1 for r in results if r["passed"])
    avg_score = round(sum(float(r["score"]) for r in results) / total, 1) if total else 0.0
    avg_latency = int(sum(int(r["latency_ms"]) for r in results) / total) if total else 0

    report = {
        "timestamp": time.strftime("%Y-%m-%d %H:%M:%S"),
        "base_url": base,
        "mode": args.mode,
        "total_cases": total,
        "passed_cases": passed_count,
        "avg_score": avg_score,
        "avg_latency_ms": avg_latency,
        "results": results,
    }
    print(f"\n=== EVAL SUMMARY: {passed_count}/{total} passed | avg_score={avg_score:.1f}/100 | avg_latency={avg_latency}ms ===")

    if args.save_baseline:
        out_path = Path(args.save_baseline)
        out_path.parent.mkdir(parents=True, exist_ok=True)
        out_path.write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8")
        print(f"Baseline saved -> {out_path}")

    if args.output_md:
        md_path = Path(args.output_md)
        md_path.parent.mkdir(parents=True, exist_ok=True)
        md_path.write_text(format_markdown_report(report), encoding="utf-8")
        print(f"Markdown report -> {md_path}")

    if args.compare:
        base_report = json.loads(Path(args.compare).read_text(encoding="utf-8"))
        ok, diff_lines = compare_with_baseline(report, base_report)
        print("\n=== BASELINE COMPARISON ===")
        for line in diff_lines:
            print(line)
        if not ok:
            return 1

    return 0 if passed_count == total else 1


if __name__ == "__main__":
    sys.exit(main())
