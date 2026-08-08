#!/usr/bin/env python3
"""Diff flint's findings against Fleet's own parser.

Usage:
    ./gitops-oracle -repo REPO > oracle.json
    flint check REPO --format json > flint.json
    ./compare.py oracle.json flint.json

Every disagreement is one of two things:

  ORACLE-ONLY   Fleet complains, flint is silent  -> a missing flint rule.
  FLINT-ONLY    flint complains, Fleet is silent  -> candidate false positive.

"Candidate" matters. flint deliberately reports things Fleet's PARSER cannot
see, because the parser only ever judges one entry file at a time:
cross-file rules (orphaned-file, duplicate-content, case-collision), workspace
[[patterns]], and hygiene rules (trailing whitespace) are all legitimately
flint-only. The list below is a starting point for review, not a verdict.

Only entry files are compared. Fleet's parser cannot judge a fragment (a
profile, a standalone policy list) on its own — it validates those through the
parent that references them — so including them would manufacture noise.
"""

import json
import os
import sys
from collections import defaultdict

# flint rules that have no parser counterpart BY DESIGN. Listing them here
# keeps the diff focused on real disagreements. Add to this list only with a
# reason — an entry here is a claim that Fleet's parser structurally cannot
# see the issue, not that the rule is unimportant.
FLINT_ONLY_BY_DESIGN = {
    # Cross-file / workspace rules: the parser sees one entry file at a time.
    "orphaned-file",
    "duplicate-content",
    "case-collision",
    "unregistered-script",
    "duplicate-identifier",
    "broken-reference",
    "path-exists",
    # Repo-authored assertions, not Fleet semantics.
    "pattern/name-matches-filename",
    "pattern/filename",
    "pattern/content-must-match",
    "pattern/content-must-not-match",
    "pattern/token-consistency",
    "pattern/must-be-referenced",
    "pattern/unique-content-within",
    "pattern/required-structure",
    "pattern/forbid-file",
    # Style/hygiene: Fleet does not care, users do.
    "yaml-trailing-whitespace",
    "yaml-empty-values",
    "yaml-indentation",
    "query-syntax",
    "query-length",
    "interval-validation",
    "software-source",
}


def norm(path: str) -> str:
    """Normalize a path so ./fleets/x.yml and /abs/fleets/x.yml compare equal."""
    return os.path.normpath(os.path.abspath(path))


def subjects(msg: str) -> list:
    """Extract the concrete things a message is about — globs, keys, names.

    The two tools word findings differently and quote differently (Fleet uses
    "double", flint uses 'single'), so matching on whole messages manufactures
    disagreements. Matching on the quoted subject is what makes the diff
    trustworthy: both tools naming the same glob is agreement, however
    differently they phrase it.
    """
    out = []
    for quote in ("'", '"'):
        parts = msg.split(quote)
        # Quoted spans are the odd-indexed parts.
        out.extend(p for p in parts[1::2] if p.strip())
    if not out:
        out.append(msg[:30])
    return out


def agrees(msg: str, others: list) -> bool:
    """True when any other-tool message mentions the same subject."""
    for subj in subjects(msg):
        # A path/glob subject can be quoted with a prefix in one tool
        # ("paths: X" vs "X"), so compare on the tail token too.
        tail = subj.split(": ")[-1].strip()
        for om in others:
            if not om:
                continue
            if subj in om or (len(tail) > 3 and tail in om):
                return True
    return False


def load_oracle(p):
    d = json.load(open(p))
    entry, findings = set(), defaultdict(list)
    for f in d["files"]:
        if f.get("not_entry_file"):
            continue
        key = norm(f["path"])
        entry.add(key)
        for x in f["findings"]:
            findings[key].append((x["severity"], x["message"]))
    return entry, findings


def load_flint(p):
    d = json.load(open(p))
    findings = defaultdict(list)
    for f in d["files"]:
        key = norm(f["path"])
        for x in f.get("diagnostics") or []:
            findings[key].append((x.get("severity"), x.get("rule"), x.get("message")))
    return findings


def main():
    if len(sys.argv) != 3:
        print(__doc__)
        return 2

    entry, oracle = load_oracle(sys.argv[1])
    flint = load_flint(sys.argv[2])

    print(f"comparing {len(entry)} entry file(s)\n")

    oracle_only, flint_only = [], []
    for path in sorted(entry):
        flint_msgs = [fm for _, _, fm in flint.get(path, [])]
        oracle_msgs = [om for _, om in oracle.get(path, [])]

        for sev, msg in oracle.get(path, []):
            if not agrees(msg, flint_msgs):
                oracle_only.append((path, sev, msg))

        for sev, rule, msg in flint.get(path, []):
            if rule in FLINT_ONLY_BY_DESIGN:
                continue
            if not agrees(msg, oracle_msgs):
                flint_only.append((path, sev, rule, msg))

    print(f"ORACLE-ONLY — Fleet complains, flint silent ({len(oracle_only)}) "
          f"→ missing flint rules")
    for path, sev, msg in oracle_only:
        print(f"  [{sev}] {os.path.relpath(path)}: {msg[:130]}")

    print(f"\nFLINT-ONLY — flint complains, Fleet silent ({len(flint_only)}) "
          f"→ review for false positives")
    for path, sev, rule, msg in flint_only:
        print(f"  [{sev}] {rule} {os.path.relpath(path)}: {msg[:110]}")

    if not oracle_only and not flint_only:
        print("\nno disagreements on entry files.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
