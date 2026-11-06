#!/usr/bin/env python3
"""
verify-docs.py — ドキュメント管理規則・重複・REF 整合性の機械検証スクリプト。

検査項目:
1. docs/ 配下の必須構成が揃っていること。
2. docs/developers.md にユーザー指示のドキュメント管理規則が原文のまま含まれていること。
3. docs/specifications/**.md にコード情報・他ファイル参照が一切含まれないこと。
4. docs/designs/**.md のファイル集合および ## / ### 見出し構成が docs/specifications/**.md と完全一致し、
   すべての ### 節に実装状態（実装待ち / 実装中 / 実装完了 + REF:src/...）が記録されていること。
5. docs/（specifications 以外）内のすべての REF:<path>[#anchor] が実在するファイルを指していること。
6. docs/history.md の全 ### エントリが grep 検索用の固定ヘッダと [HIST-TAG: ...] を持つこと。
7. 複数ファイル間の重複検査:
   - <!-- SYNC:START <id> --> ... <!-- SYNC:END <id> --> ブロックが全出現箇所で一字一句一致する。
   - SYNC ブロック外で複数ドキュメント間に重複した段落が存在しないこと。
"""

from __future__ import annotations

import os
import re
import sys
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
DOCS = ROOT / "docs"

REQUIRED_DOCS = [
    DOCS / "developers.md",
    DOCS / "handoff.md",
    DOCS / "plans.md",
    DOCS / "history.md",
    DOCS / "script" / "verify-docs.py",
]

VERBATIM_RULES_MARKER = "1. ドキュメント整理"

# specifications/*.md で禁止されるパターン（コード情報・他ファイル参照を示す）
FORBIDDEN_SPEC_PATTERNS = [
    r"\bsrc/",
    r"\btests/",
    r"\bbenches/",
    r"\bcore::",
    r"\badapters::",
    r"\bui::",
    r"\bdocs/(?:specifications|designs|script|developers\.md|plans\.md|handoff\.md|history\.md)",
    r"\bREF:",
    r"\]\(",  # Markdown の相対リンク（外部リンク以外の参照を禁止）
]


def parse_sections_outside_fences(text: str) -> list[tuple[str, list[str]]]:
    """Return list of (heading_line, body_lines) for ## and ### headings outside code fences."""
    sections: list[tuple[str, list[str]]] = []
    in_fence = False
    current_head: str | None = None
    current_body: list[str] = []
    for line in text.splitlines():
        s = line.strip()
        if s.startswith("```"):
            in_fence = not in_fence
            if current_head is not None:
                current_body.append(line)
            continue
        if not in_fence and (s.startswith("## ") or s.startswith("### ")):
            if current_head is not None:
                sections.append((current_head, current_body))
            current_head = s
            current_body = []
        else:
            if current_head is not None:
                current_body.append(line)
    if current_head is not None:
        sections.append((current_head, current_body))
    return sections


def extract_sync_blocks(text: str) -> dict[str, str]:
    blocks: dict[str, str] = {}
    pattern = re.compile(
        r"<!--\s*SYNC:START\s+([A-Za-z0-9_-]+)\s*-->\n(.*?)\n<!--\s*SYNC:END\s+\1\s*-->",
        re.DOTALL,
    )
    for m in pattern.finditer(text):
        blocks[m.group(1)] = m.group(2).strip()
    return blocks


def strip_sync_and_fences(text: str) -> list[tuple[int, str]]:
    """Return (lineno, stripped_line) outside SYNC blocks and code fences."""
    out: list[tuple[int, str]] = []
    in_fence = False
    in_sync = False
    for idx, line in enumerate(text.splitlines(), 1):
        s = line.strip()
        if s.startswith("<!-- SYNC:START "):
            in_sync = True
            continue
        if s.startswith("<!-- SYNC:END "):
            in_sync = False
            continue
        if s.startswith("```"):
            in_fence = not in_fence
            continue
        if not in_fence and not in_sync:
            out.append((idx, s))
    return out


def main() -> int:
    errors: list[str] = []

    # 1. 必須ファイルの存在確認
    for path in REQUIRED_DOCS:
        if not path.is_file():
            errors.append(f"[rule-1] 必須ファイルが存在しません: {path.relative_to(ROOT)}")

    spec_dir = DOCS / "specifications"
    design_dir = DOCS / "designs"
    if not spec_dir.is_dir() or not design_dir.is_dir():
        errors.append("[rule-1] docs/specifications または docs/designs ディレクトリが存在しません")
        for err in errors:
            print(err, file=sys.stderr)
        return 1

    # 2. developers.md に管理規則の原文が含まれていること
    dev_path = DOCS / "developers.md"
    if dev_path.is_file():
        dev_text = dev_path.read_text(encoding="utf-8")
        if VERBATIM_RULES_MARKER not in dev_text:
            errors.append("[rule-2] docs/developers.md にドキュメント管理規則の原文が含まれていません")

    # 3. specifications/*.md の純粋性検査
    spec_files = sorted(p.name for p in spec_dir.glob("*.md"))
    design_files = sorted(p.name for p in design_dir.glob("*.md"))
    if not spec_files:
        errors.append("[rule-3] docs/specifications/*.md が空です")
    if spec_files != design_files:
        errors.append(
            f"[rule-4] specifications と designs のファイル一覧が一致しません: {spec_files} != {design_files}"
        )

    combined_forbidden = re.compile("|".join(f"(?:{p})" for p in FORBIDDEN_SPEC_PATTERNS))
    for name in spec_files:
        sp = spec_dir / name
        text = sp.read_text(encoding="utf-8")
        for lineno, line in enumerate(text.splitlines(), 1):
            # コードフェンス内は除外
            in_fence = False
            for prev_line in text.splitlines()[:lineno]:
                if prev_line.strip().startswith("```"):
                    in_fence = not in_fence
            if in_fence:
                continue
            m = combined_forbidden.search(line)
            if m:
                errors.append(
                    f"[rule-3] docs/specifications/{name}:{lineno} に禁止パターン '{m.group(0)}' が含まれています"
                )

    # 4. specifications と designs の見出し一致 & 実装状態の検査
    status_re = re.compile(
        r"^状態:\s*(実装完了|実装中|未実装・実装計画済み|未実装・未計画|実装待ち)（?(.+)?）?$"
    )

    for name in spec_files:
        sp = spec_dir / name
        dp = design_dir / name
        if not dp.is_file():
            continue
        s_secs = parse_sections_outside_fences(sp.read_text(encoding="utf-8"))
        d_secs = parse_sections_outside_fences(dp.read_text(encoding="utf-8"))
        s_heads = [h for h, _ in s_secs]
        d_heads = [h for h, _ in d_secs]
        if s_heads != d_heads:
            errors.append(
                f"[rule-4] docs/specifications/{name} と docs/designs/{name} の見出し構成が一致しません"
            )

        for head, body in d_secs:
            if not head.startswith("### "):
                continue
            non_empty = [l.strip() for l in body if l.strip()]
            if not non_empty:
                errors.append(f"[rule-4] docs/designs/{name} の '{head}' が空です")
                continue
            m = status_re.match(non_empty[0])
            if not m:
                errors.append(
                    f"[rule-4] docs/designs/{name} の '{head}' の冒頭に有効な '状態: ...' 行がありません: {non_empty[0]}"
                )

    # 5. REF: 参照の存在確認
    ref_re = re.compile(r"\bREF:([A-Za-z0-9_./-]+)(?:#([A-Za-z0-9_-]+))?")
    non_spec_mds = [
        DOCS / "developers.md",
        DOCS / "handoff.md",
        DOCS / "plans.md",
        DOCS / "history.md",
        *sorted(design_dir.glob("*.md")),
    ]
    for md_path in non_spec_mds:
        if not md_path.is_file():
            continue
        rel = md_path.relative_to(ROOT)
        text = md_path.read_text(encoding="utf-8")
        for m in ref_re.finditer(text):
            target_rel, anchor = m.group(1), m.group(2)
            if "..." in target_rel:
                continue
            target_path = ROOT / target_rel
            if not target_path.exists():
                errors.append(f"[rule-5] {rel} の REF:{target_rel} が存在しないパスを指しています")
            elif anchor and target_path.is_file():
                target_text = target_path.read_text(encoding="utf-8")
                if f'id="{anchor}"' not in target_text and f"# {anchor}" not in target_text:
                    errors.append(
                        f"[rule-5] {rel} の REF:{target_rel}#{anchor} のアンカーが対象ファイル内に見つかりません"
                    )

    # 6. docs/history.md の grep 検索構造検査
    hist_path = DOCS / "history.md"
    if hist_path.is_file():
        hist_text = hist_path.read_text(encoding="utf-8")
        hist_secs = parse_sections_outside_fences(hist_text)
        head_re = re.compile(r"^### \[HIST:([A-Z-]+):([A-Za-z0-9._-]+)\] \d{4}-\d{2}-\d{2} .+$")
        tag_re = re.compile(r"^\[HIST-TAG: category=([A-Z-]+) id=([A-Za-z0-9._-]+) topics=[^\]]+\]$")
        for head, body in hist_secs:
            if not head.startswith("### "):
                continue
            hm = head_re.match(head)
            if not hm:
                errors.append(f"[rule-6] docs/history.md の見出しが規約外です: {head}")
                continue
            non_empty = [l.strip() for l in body if l.strip()]
            if not non_empty:
                errors.append(f"[rule-6] docs/history.md の '{head}' の本文が空です")
                continue
            tm = tag_re.match(non_empty[0])
            if not tm:
                errors.append(
                    f"[rule-6] docs/history.md の '{head}' 直下に [HIST-TAG: ...] 行がありません"
                )

    # 7. 重複検査（SYNC ブロックの一致 + SYNC 外の段落重複禁止）
    sync_occurrences: dict[str, list[tuple[Path, str]]] = defaultdict(list)
    for md_path in non_spec_mds:
        if not md_path.is_file():
            continue
        for sync_id, content in extract_sync_blocks(md_path.read_text(encoding="utf-8")).items():
            sync_occurrences[sync_id].append((md_path, content))

    for sync_id, items in sync_occurrences.items():
        base_path, base_content = items[0]
        for other_path, other_content in items[1:]:
            if base_content != other_content:
                errors.append(
                    f"[rule-7] SYNC ブロック '{sync_id}' の内容が {base_path.relative_to(ROOT)} と {other_path.relative_to(ROOT)} で一致しません"
                )

    # SYNC 外の散文段落（40 文字以上）が複数のファイルに重複していないか検査
    prose_locations: dict[str, list[str]] = defaultdict(list)
    for md_path in non_spec_mds:
        if not md_path.is_file():
            continue
        rel = str(md_path.relative_to(ROOT))
        seen_in_file: set[str] = set()
        for lineno, line in strip_sync_and_fences(md_path.read_text(encoding="utf-8")):
            if len(line) < 40:
                continue
            if line.startswith(("状態:", "|---", "[HIST-TAG:", "#")):
                continue
            if line in seen_in_file:
                continue
            seen_in_file.add(line)
            prose_locations[line].append(f"{rel}:{lineno}")

    for line, locs in prose_locations.items():
        if len(locs) > 1:
            errors.append(
                f"[rule-7] SYNC ブロック外で複数ファイルに重複した記述があります ({', '.join(locs)}): {line[:60]}..."
            )

    if errors:
        for err in errors:
            print(err, file=sys.stderr)
        return 1

    print(
        f"OK: docs rules verified "
        f"({len(spec_files)} specifications, {len(design_files)} designs, "
        f"SYNC blocks & REF links valid)."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
