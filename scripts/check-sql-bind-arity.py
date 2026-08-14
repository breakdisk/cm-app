#!/usr/bin/env python3
"""Every positional INSERT must bind exactly as many values as it names.

On 2026-08-14 a column was added to the end of `catalog_items`' INSERT column
list and bound in the middle of the `.bind()` chain, next to its logical
neighbours. sqlx binds are positional, so every parameter after it shifted and
Postgres rejected the whole write:

    column "created_at" is of type timestamp with time zone
    but expression is of type text

That was not a bug in the new column. `save_item` is the only write path for
manual create, edit, CSV import and connector sync, so *every catalog write*
failed from the moment it deployed.

Nothing could have caught it. The unit tests use in-memory repository fakes and
the integration suites stand up no database, so a bind-order mistake is
invisible to both by construction. It surfaced from PATCHing a real row on the
live host.

It checks two things a reviewer would:

  * the counts agree — columns, highest `$N`, and `.bind(...)` calls
  * each bind sits at the position of the column it names

The second is the one that matters, and the first version of this script did
not have it. That version passed the very bug it was written for: appending a
column and moving a bind leaves 19 columns, $19 and 19 binds, all in agreement
and all wrong. It only failed once the *order* was compared.

Where a bind argument does not plainly name a column — a computed value, a
literal — the statement is skipped rather than guessed at.

Deliberately conservative: anything it cannot parse with confidence is skipped
and *named* at the end, so the report never reads as "all clear" when it merely
means "did not look".
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# `INSERT INTO schema.table ( ... ) VALUES ( ... )` — the column list and the
# value list, non-greedy so a statement with a trailing ON CONFLICT still parses.
INSERT_RE = re.compile(
    r"INSERT\s+INTO\s+[\w.\"]+\s*\((?P<cols>[^)]*?)\)\s*VALUES\s*\((?P<vals>[^)]*?)\)",
    re.IGNORECASE | re.DOTALL,
)
PLACEHOLDER_RE = re.compile(r"\$(\d+)")
# `.bind(` plus its argument, up to the matching close paren at depth 0.
BIND_RE = re.compile(r"\.bind\s*\(")


def bind_arguments(tail: str) -> list[str]:
    """Every `.bind(...)` argument in order, as written."""
    args: list[str] = []
    for m in BIND_RE.finditer(tail):
        depth, i = 1, m.end()
        while i < len(tail) and depth:
            if tail[i] == "(":
                depth += 1
            elif tail[i] == ")":
                depth -= 1
            i += 1
        args.append(tail[m.end() : i - 1].strip())
    return args


# Trailing method calls carry no column name: `i.source.as_str()` is the
# `source` column, `tenant_id.inner()` is `tenant_id`.
CAST_RE = re.compile(r"\s+as\s+[\w:<>, ]+$")


def column_of(expr: str) -> str | None:
    """The column an argument plainly refers to, or None when it is not plain."""
    e = expr.strip().lstrip("&").strip()
    # A width cast says nothing about which column this is: `p.total_shipments
    # as i32` is the `total_shipments` column. Without this the last segment is
    # `total_shipments as i32`, which matches no identifier, and the walk falls
    # back to the receiver `p` — so a whole statement gets skipped for a cast.
    e = CAST_RE.sub("", e).strip()
    segments = [seg for seg in e.split(".") if seg]
    for seg in reversed(segments):
        if re.fullmatch(r"[a-z_][a-z0-9_]*", seg):
            return seg
    return None
# Where a query chain stops being binds.
TERMINATOR_RE = re.compile(r"\.(execute|fetch_one|fetch_all|fetch_optional|fetch)\s*\(")


def columns(raw: str) -> list[str]:
    """Column names from a column list, comments and whitespace removed."""
    without_comments = re.sub(r"--[^\n]*", "", raw)
    return [c.strip() for c in without_comments.split(",") if c.strip()]


def analyse(path: Path) -> tuple[list[str], list[str], int]:
    """Return (failures, skips, judged) for one file."""
    text = path.read_text(encoding="utf-8", errors="replace")
    failures: list[str] = []
    skips: list[str] = []
    judged = 0

    for m in INSERT_RE.finditer(text):
        line = text.count("\n", 0, m.start()) + 1
        where = f"{path.relative_to(ROOT).as_posix()}:{line}"

        cols = columns(m.group("cols"))
        vals = m.group("vals")

        # Only statements written entirely with positional placeholders. A
        # literal like NOW() or 'available' is a value the caller never binds,
        # so the counts legitimately differ and this cannot judge them.
        val_parts = [v.strip() for v in vals.split(",") if v.strip()]
        if not val_parts or not all(re.fullmatch(r"\$\d+", v) for v in val_parts):
            skips.append(f"{where}  (values are not all positional)")
            continue

        # Placeholders must be exactly $1..$N with nothing missing or repeated.
        indices = [int(i) for i in PLACEHOLDER_RE.findall(vals)]
        if sorted(indices) != list(range(1, len(indices) + 1)):
            failures.append(
                f"{where}\n      placeholders are not a contiguous $1..$N: {sorted(indices)}"
            )
            continue

        # The rest of the statement may bind more (ON CONFLICT ... = $N, WHERE).
        tail_start = m.end()
        terminator = TERMINATOR_RE.search(text, tail_start)
        if terminator is None:
            skips.append(f"{where}  (no .execute/.fetch terminator found)")
            continue
        tail = text[tail_start : terminator.start()]

        highest = max(indices + [int(i) for i in PLACEHOLDER_RE.findall(tail)])
        bind_args = bind_arguments(tail)
        binds = len(bind_args)

        if binds == 0:
            # A prepared statement whose arguments are supplied elsewhere.
            skips.append(f"{where}  (no .bind() chain — arguments supplied elsewhere)")
            continue

        if len(cols) != highest or binds != highest:
            failures.append(
                f"{where}\n"
                f"      {len(cols)} columns, highest placeholder ${highest}, {binds} .bind() calls"
                f" — these must be equal."
            )
            continue

        # Counts agreeing is not enough, and assuming otherwise is what made the
        # first version of this script useless: the production bug had 19
        # columns, $19 and 19 binds and was still wrong, because a bind was
        # *moved* rather than added. Where the arguments plainly name their
        # columns, the two sequences have to match position for position.
        named = [column_of(a) for a in bind_args[: len(cols)]]
        if any(n is None for n in named) or sorted(n for n in named if n) != sorted(cols):
            skips.append(f"{where}  (bind arguments do not plainly name their columns)")
            continue

        for position, (col, got) in enumerate(zip(cols, named), start=1):
            if col != got:
                failures.append(
                    f"{where}\n"
                    f"      ${position} is column `{col}` but binds `{got}`.\n"
                    f"      Binds are positional, so every parameter after this one is\n"
                    f"      shifted too — the statement fails at runtime with a type\n"
                    f"      error naming a column you never touched."
                )
                break
        else:
            judged += 1

    return failures, skips, judged


def main() -> int:
    files = sorted(ROOT.glob("services/*/src/**/*.rs")) + sorted(ROOT.glob("libs/*/src/**/*.rs"))
    all_failures: list[str] = []
    all_skips: list[str] = []
    checked = 0
    all_judged = 0

    for path in files:
        if "INSERT INTO" not in path.read_text(encoding="utf-8", errors="replace"):
            continue
        failures, skips, judged = analyse(path)
        checked += 1
        all_judged += judged
        all_failures.extend(failures)
        all_skips.extend(skips)

    # Report both halves. "61 files checked" alone would read as full coverage
    # when the guard in fact verified the order of a little over half the
    # statements — the same overclaim this script exists to prevent.
    if all_skips:
        print(f"Not judged ({len(all_skips)}) — listed so this is never mistaken for a clean bill:")
        for s in all_skips:
            print(f"  - {s}")
        print()

    if all_failures:
        print(f"FAIL: {len(all_failures)} INSERT statement(s) whose arity does not add up:\n")
        for f in all_failures:
            print(f"  - {f}\n")
        return 1

    print(
        f"OK: {all_judged} of {all_judged + len(all_skips)} INSERT statement(s) across "
        f"{checked} file(s) verified to bind what they name, in order."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
