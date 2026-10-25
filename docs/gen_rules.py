"""Render docs/docs/rules-list.md from `flint list-rules --format json`, and
docs/docs/schemas/fleetlint.json from `flint config-schema`.

Run before `zensical build`. The page is never committed (it's gitignored):
it is always built from the flint it describes, so it cannot drift.

    uv run python gen_rules.py            # uses `cargo run -p flint`
    FLINT=/path/to/flint uv run python gen_rules.py
"""

import json
import os
import subprocess
from collections import defaultdict
from pathlib import Path

HERE = Path(__file__).resolve().parent
OUT = HERE / "docs" / "rules-list.md"

flint = os.environ.get("FLINT")
cmd = [str(Path(flint).resolve())] if flint else ["cargo", "run", "-q", "-p", "flint", "--"]
data = json.loads(
    subprocess.run(cmd + ["list-rules", "--format", "json"], cwd=HERE.parent,
                   check=True, capture_output=True, text=True).stdout
)

by_category = defaultdict(list)
for rule in data["rules"]:
    by_category[rule["category"]].append(rule)

lines = [
    "---",
    "icon: lucide/list",
    "---",
    "",
    "# All rules",
    "",
    f"Generated from `flint list-rules` at docs build time: {data['total']} rule codes.",
    "Configure them in `.fleetlint.toml` (see [Rules](rules.md)).",
    "",
]
for category in sorted(by_category):
    lines += [f"## {category}", "", "| Rule | Fixable | Description |", "|---|---|---|"]
    for r in sorted(by_category[category], key=lambda r: r["name"]):
        name = f"[`{r['name']}`]({r['docs_url']})" if r.get("docs_url") else f"`{r['name']}`"
        desc = r["description"].replace("|", "\\|")
        lines.append(f"| {name} | {'yes' if r['fixable'] else 'no'} | {desc} |")
    lines.append("")

OUT.write_text("\n".join(lines))
print(f"wrote {OUT.relative_to(HERE.parent)} ({data['total']} rules)")

# The fleetlint.toml JSON Schema, served at /schemas/fleetlint.json (the URL
# a SchemaStore catalog entry points editors at).
schema = HERE / "docs" / "schemas" / "fleetlint.json"
schema.parent.mkdir(exist_ok=True)
schema.write_text(subprocess.run(cmd + ["config-schema"], cwd=HERE.parent, check=True,
                                 capture_output=True, text=True).stdout)
print(f"wrote {schema.relative_to(HERE.parent)}")
