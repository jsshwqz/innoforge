#!/usr/bin/env python3
"""
跨网盘通用专利订阅源增量打包工具（InnoForge Route B Publisher）

将 `headers_v2/`（或任意包含 `.jsonl` / `.jsonl.gz` 专利著录的目录）增量压缩为 `.jsonl.gz`
（体积通常压缩 75%~80%，自动跳过 `work_*` 临时目录与重复 `.csv`），并生成标准
`innoforge-feed.json` 清单文件。

生成的输出目录可直接上传至任意一家网盘/对象存储，接收方零注册、免客户端即可在 InnoForge 中增量订阅：
  - 天翼云盘 / 阿里云盘 / 夸克 / 115 + OpenList (AList) 只读 WebDAV
  - Cloudflare R2（10GB 免费 + 零出口流量费）
  - 阿里云 OSS / 腾讯云 COS / 七牛云 / MinIO
  - 坚果云 / 群晖 NAS / Nextcloud WebDAV

用法示例：
  python3 tools/build_patent_feed.py --src /mnt/f/down/专利数据 --out /mnt/f/down/innoforge-feed
  python3 tools/build_patent_feed.py --src "F:\\down\\专利数据" --out "F:\\down\\innoforge-feed" --quarters 2026Q2,2026Q3
"""

from __future__ import annotations

import argparse
import gzip
import hashlib
import json
import os
import shutil
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Dict, List, Optional, Set, Tuple

SKIP_DIRS = {"work_v2", "__pycache__", "__macosx", "tmp", "_tmp", "temp", "node_modules"}


def should_skip_dir(name: str) -> bool:
    low = name.lower()
    return name.startswith(".") or low.startswith("work_") or low in SKIP_DIRS


def resolve_source_root(raw_src: str) -> Path:
    src = Path(raw_src).expanduser()
    headers = src / "headers_v2"
    if headers.is_dir():
        return headers
    return src


def parse_quarters_filter(raw: str) -> Set[str]:
    if not raw.strip():
        return set()
    return {q.strip().upper() for q in raw.replace("，", ",").split(",") if q.strip()}


def sha256_and_size(path: Path) -> Tuple[str, int]:
    h = hashlib.sha256()
    total = 0
    with path.open("rb") as f:
        while True:
            chunk = f.read(1024 * 1024)
            if not chunk:
                break
            h.update(chunk)
            total += len(chunk)
    return h.hexdigest(), total


def compress_jsonl_incremental(
    src_file: Path,
    dst_gz: Path,
    prev_item: Optional[Dict[str, object]] = None,
) -> Tuple[bool, int]:
    """若目标 .jsonl.gz 已存在且整秒 mtime 与源文件匹配（兼容 WSL2/NTFS 纳秒截断）则跳过重压。返回 (是否重新压缩, 行数估算/0)。"""
    dst_gz.parent.mkdir(parents=True, exist_ok=True)
    src_stat = src_file.stat()
    src_mtime_sec = int(src_stat.st_mtime)
    if dst_gz.is_file():
        dst_stat = dst_gz.stat()
        dst_mtime_sec = int(dst_stat.st_mtime)
        prev_src_size = prev_item.get("src_size") if isinstance(prev_item, dict) else None
        src_size_unchanged = prev_src_size is None or int(prev_src_size) == int(src_stat.st_size)
        if dst_stat.st_size > 0 and dst_mtime_sec + 1 >= src_mtime_sec and src_size_unchanged:
            return False, 0

    tmp_path = dst_gz.with_suffix(dst_gz.suffix + ".tmp")
    rows = 0
    try:
        with src_file.open("rb") as fin, gzip.open(tmp_path, "wb", compresslevel=6) as fout:
            for line in fin:
                if line.strip():
                    rows += 1
                fout.write(line)
        os.utime(tmp_path, (src_mtime_sec, src_mtime_sec))
        os.replace(tmp_path, dst_gz)
    finally:
        if tmp_path.exists():
            tmp_path.unlink(missing_ok=True)
    return True, rows


def build_feed(
    src_root: Path,
    out_root: Path,
    title: str,
    quarters: Set[str],
    base_url: str,
) -> Dict[str, object]:
    if not src_root.is_dir():
        raise FileNotFoundError(f"源目录不存在: {src_root}")
    out_root.mkdir(parents=True, exist_ok=True)

    # 读取旧清单以便对未变更文件复用 sha256 与行数，实现毫秒级增量重建
    prev_index: Dict[str, Dict[str, object]] = {}
    manifest_path = out_root / "innoforge-feed.json"
    if manifest_path.is_file():
        try:
            old = json.loads(manifest_path.read_text(encoding="utf-8"))
            for item in old.get("files", []):
                if isinstance(item, dict) and item.get("path"):
                    prev_index[str(item["path"])] = item
        except Exception:
            pass

    discovered: List[Path] = []
    for cur, dirs, files in os.walk(src_root):
        dirs[:] = sorted(d for d in dirs if not should_skip_dir(d))
        cur_p = Path(cur)
        for fname in sorted(files):
            low = fname.lower()
            if fname.startswith(".") or fname.startswith("~$"):
                continue
            if not (low.endswith(".jsonl") or low.endswith(".jsonl.gz")):
                continue
            full_p = cur_p / fname
            rel_parts = full_p.relative_to(src_root).parts
            if quarters:
                quarter_hit = any(p.upper() in quarters for p in rel_parts[:-1])
                if not quarter_hit:
                    continue
            discovered.append(full_p)

    # 若同目录同时存在 .jsonl 与 .jsonl.gz，优先采用已有 .jsonl.gz
    by_stem: Dict[str, Path] = {}
    for p in discovered:
        rel = p.relative_to(src_root).as_posix()
        stem = rel[:-9] if rel.lower().endswith(".jsonl.gz") else rel[:-6]
        if stem not in by_stem or p.name.lower().endswith(".jsonl.gz"):
            by_stem[stem] = p

    entries: List[Dict[str, object]] = []
    recompressed = 0
    reused = 0
    total_raw_bytes = 0
    total_gz_bytes = 0

    for stem in sorted(by_stem.keys()):
        src_file = by_stem[stem]
        rel_out = f"{stem}.jsonl.gz"
        dst_gz = out_root / rel_out
        src_size = src_file.stat().st_size
        total_raw_bytes += src_size

        old_item = prev_index.get(rel_out)
        if src_file.name.lower().endswith(".jsonl.gz"):
            dst_gz.parent.mkdir(parents=True, exist_ok=True)
            if not dst_gz.is_file() or int(dst_gz.stat().st_mtime) + 1 < int(src_file.stat().st_mtime):
                shutil.copy2(src_file, dst_gz)
                changed = True
            else:
                changed = False
            rows = 0
        else:
            changed, rows = compress_jsonl_incremental(src_file, dst_gz, old_item)

        dst_stat = dst_gz.stat()
        mtime = int(dst_stat.st_mtime)
        size = int(dst_stat.st_size)
        total_gz_bytes += size

        if (
            not changed
            and old_item
            and old_item.get("size") == size
            and abs(int(old_item.get("mtime", 0) or 0) - mtime) <= 2
            and old_item.get("sha256")
        ):
            sha = str(old_item["sha256"])
            rows = int(old_item.get("rows", 0) or 0)
            reused += 1
        else:
            sha, _ = sha256_and_size(dst_gz)
            recompressed += 1

        entry: Dict[str, object] = {
            "path": rel_out,
            "size": size,
            "src_size": int(src_size),
            "mtime": mtime,
            "sha256": sha,
        }
        if rows > 0:
            entry["rows"] = rows
        if base_url:
            entry["url"] = f"{base_url.rstrip('/')}/{rel_out}"
        entries.append(entry)

    feed = {
        "version": 1,
        "title": title,
        "updated_at": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "file_count": len(entries),
        "total_bytes": total_gz_bytes,
        "files": entries,
    }
    tmp_manifest = manifest_path.with_suffix(".json.tmp")
    tmp_manifest.write_text(json.dumps(feed, ensure_ascii=False, indent=2), encoding="utf-8")
    os.replace(tmp_manifest, manifest_path)

    ratio = (1.0 - (total_gz_bytes / total_raw_bytes)) * 100.0 if total_raw_bytes > 0 else 0.0
    return {
        "files": len(entries),
        "recompressed": recompressed,
        "reused": reused,
        "raw_mb": round(total_raw_bytes / (1024 * 1024), 2),
        "gz_mb": round(total_gz_bytes / (1024 * 1024), 2),
        "saved_pct": round(ratio, 1),
        "manifest": str(manifest_path),
    }


def main(argv: Optional[List[str]] = None) -> int:
    parser = argparse.ArgumentParser(
        description="将专利 JSONL 目录增量压缩为 .jsonl.gz 并生成跨网盘通用 innoforge-feed.json 清单"
    )
    parser.add_argument("--src", required=True, help="源目录（如 F:\\down\\专利数据 或 headers_v2）")
    parser.add_argument("--out", required=True, help="输出发布目录（上传到天翼云 / R2 / OSS / 群晖）")
    parser.add_argument("--title", default="InnoForge Patent Feed", help="订阅源名称")
    parser.add_argument("--quarters", default="", help="可选：只打包指定季度，逗号分隔（如 2026Q2,2026Q3）")
    parser.add_argument("--base-url", default="", help="可选：在清单中写入绝对下载前缀 URL（留空默认相对路径，通用性最强）")
    args = parser.parse_args(argv)

    src_root = resolve_source_root(args.src)
    out_root = Path(args.out).expanduser()
    quarters = parse_quarters_filter(args.quarters)

    stats = build_feed(src_root, out_root, args.title, quarters, args.base_url)
    print(
        f"[OK] 打包完成: {stats['files']} 个 .jsonl.gz（新增/更新 {stats['recompressed']} · 复用 {stats['reused']}）\n"
        f"     原始体积: {stats['raw_mb']} MB -> 压缩后: {stats['gz_mb']} MB（节省 {stats['saved_pct']}%）\n"
        f"     清单文件: {stats['manifest']}\n"
        f"\n"
        f"下一步可将输出目录上传至任意网盘/对象存储：\n"
        f"  1) 天翼云盘 + OpenList WebDAV : 上传整个输出目录 -> 分享只读 WebDAV 地址 https://<域名>/dav/patents\n"
        f"  2) Cloudflare R2 (零流量费)   : rclone sync \"{out_root}\" r2:<bucket>/patents/\n"
        f"  3) 阿里云 OSS / 腾讯云 COS    : ossutil sync \"{out_root}\" oss://<bucket>/patents/\n"
        f"  4) 坚果云 / 群晖 / Nextcloud  : 同步或上传该目录并开启只读 WebDAV"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
