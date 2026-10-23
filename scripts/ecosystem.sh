#!/usr/bin/env bash
# ecosystem.sh — what does this change do to real GitOps repos?
#
# Builds flint at a base ref and from the working tree, lints a pinned
# real-world corpus with both, and reports the difference per rule:
# Fleet's own GitOps config (fleetdm/fleet it-and-security/, ~130 files).
# Modelled on ty's ecosystem analyzer: diagnostic changes are REPORTED, not
# failed on — only a crash (exit >= 101 or no JSON) fails the run.
#
# Usage:  scripts/ecosystem.sh [--base <git-ref>] [--out <dir>] [--corpus <dir>]...
#   --base    ref to compare against (default: main)
#   --out     where report.md and diff.json go (default: ./ecosystem-out)
#   --corpus  also lint this local GitOps repo, in place (repeatable). For
#             private repos: the report stays local; never commit it.
# Env:    FLEET_REPO   local fleetdm/fleet clone (default: ~/Code/core-work/fleet)
#         FLEET_CORPUS_SHA  pinned commit of that clone to lint
set -euo pipefail

FLEET_CORPUS_SHA="${FLEET_CORPUS_SHA:-350a9342cba539cee0e206fec4a09379ee12a074}"
FLEET_REPO="${FLEET_REPO:-$HOME/Code/core-work/fleet}"
BASE=main
OUT=ecosystem-out
EXTRA=()
while [ $# -gt 0 ]; do
  case "$1" in
    --base) BASE="$2"; shift 2 ;;
    --out) OUT="$2"; shift 2 ;;
    --corpus) EXTRA+=("$(cd "$2" && pwd)"); shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

ROOT="$(git rev-parse --show-toplevel)"
WORK="$(mktemp -d)"
trap 'git -C "$ROOT" worktree remove --force "$WORK/base-src" >/dev/null 2>&1 || true; rm -rf "$WORK"' EXIT
mkdir -p "$OUT"

# --- corpus: extracted at the pinned commit, so line numbers never move ---
git -C "$FLEET_REPO" cat-file -e "$FLEET_CORPUS_SHA^{commit}" 2>/dev/null \
  || git -C "$FLEET_REPO" fetch -q origin "$FLEET_CORPUS_SHA"
mkdir -p "$WORK/corpus"
git -C "$FLEET_REPO" archive "$FLEET_CORPUS_SHA" it-and-security | tar -x -C "$WORK/corpus"
# label <TAB> directory, one per corpus
CORPORA=("fleetdm/fleet it-and-security@${FLEET_CORPUS_SHA:0:10}"$'\t'"$WORK/corpus/it-and-security")
for dir in ${EXTRA[@]+"${EXTRA[@]}"}; do CORPORA+=("$(basename "$dir")"$'\t'"$dir"); done

# --- binaries: base from a throwaway worktree, PR from the working tree ---
echo "building base ($BASE)..." >&2
git -C "$ROOT" worktree add -q --detach "$WORK/base-src" "$BASE"
cargo build -q --locked --release -p flint --manifest-path "$WORK/base-src/Cargo.toml" \
  --target-dir "$WORK/base-target"
echo "building PR (working tree)..." >&2
cargo build -q --locked --release -p flint --manifest-path "$ROOT/Cargo.toml"
BASE_BIN="$WORK/base-target/release/flint"
PR_BIN="$ROOT/target/release/flint"

# One finding per line: corpus <TAB> rule <TAB> severity <TAB> path:line <TAB>
# message. Paths are relative to the corpus, so base and PR compare equal.
run() {
  local bin="$1" name="$2" entry label dir code
  : >"$WORK/$name.tsv"
  for entry in "${CORPORA[@]}"; do
    label="${entry%%$'\t'*}" dir="${entry#*$'\t'}" code=0
    (cd "$dir" && "$bin" check . --format json) >"$WORK/$name.json" 2>"$WORK/$name.err" || code=$?
    if [ "$code" -ge 101 ] || ! jq -e .summary "$WORK/$name.json" >/dev/null 2>&1; then
      echo "::error::$name flint crashed on $label (exit $code)" >&2
      cat "$WORK/$name.err" >&2
      return 1
    fi
    jq -r --arg c "$label" '.files[] | (.path | sub("^\\./"; "")) as $p | .diagnostics[]
           | [$c, .rule, .severity, "\($p):\(.line)", .message] | @tsv' "$WORK/$name.json" >>"$WORK/$name.tsv"
  done
  sort -o "$WORK/$name.tsv" "$WORK/$name.tsv"
}
crash=0
run "$BASE_BIN" base || crash=1
run "$PR_BIN" pr || crash=1
[ "$crash" -eq 0 ] || exit 1

comm -13 "$WORK/base.tsv" "$WORK/pr.tsv" >"$WORK/added.tsv"
comm -23 "$WORK/base.tsv" "$WORK/pr.tsv" >"$WORK/removed.tsv"

# --- report: one section per corpus ---
count() { awk -F'\t' -v c="$2" '$1 == c' "$1" | wc -l | tr -d ' '; }
{
  for entry in "${CORPORA[@]}"; do
    label="${entry%%$'\t'*}"
    awk -F'\t' -v c="$label" '$1 == c' "$WORK/added.tsv" >"$WORK/a.tsv"
    awk -F'\t' -v c="$label" '$1 == c' "$WORK/removed.tsv" >"$WORK/r.tsv"
    echo "### flint ecosystem: \`$label\`"
    echo
    echo "Base \`$BASE\`: $(count "$WORK/base.tsv" "$label") findings · this change: $(count "$WORK/pr.tsv" "$label") findings · +$(wc -l <"$WORK/a.tsv" | tr -d ' ') / −$(wc -l <"$WORK/r.tsv" | tr -d ' ')"
    echo
    if [ ! -s "$WORK/a.tsv" ] && [ ! -s "$WORK/r.tsv" ]; then
      echo "No diagnostic changes."
    else
      echo "| rule | added | removed |"
      echo "|---|---|---|"
      join -t $'\t' -a1 -a2 -e0 -o 0,1.2,2.2 \
        <(cut -f2 "$WORK/a.tsv" | sort | uniq -c | awk '{print $2"\t"$1}' | sort) \
        <(cut -f2 "$WORK/r.tsv" | sort | uniq -c | awk '{print $2"\t"$1}' | sort) \
        | awk -F'\t' '{print "| `"$1"` | "$2" | "$3" |"}'
      for kind in a r; do
        [ -s "$WORK/$kind.tsv" ] || continue
        [ "$kind" = a ] && title=added || title=removed
        echo
        echo "<details><summary>$title (first 30)</summary>"
        echo
        echo '```'
        head -30 "$WORK/$kind.tsv" | awk -F'\t' '{print $3" "$4" ["$2"] "$5}'
        echo '```'
        echo "</details>"
      done
    fi
    echo
  done
} >"$OUT/report.md"

jq -n --rawfile a "$WORK/added.tsv" --rawfile r "$WORK/removed.tsv" \
  --arg sha "$FLEET_CORPUS_SHA" --arg base "$BASE" '
  def rows: split("\n") | map(select(length > 0) | split("\t")
            | {corpus: .[0], rule: .[1], severity: .[2], location: .[3], message: .[4]});
  {fleet_corpus_sha: $sha, base: $base,
   added: ($a | rows), removed: ($r | rows)}' >"$OUT/diff.json"

cat "$OUT/report.md"
