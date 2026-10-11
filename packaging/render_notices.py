"""Render cargo-about JSON (stdin) as the crate section of THIRD_PARTY_LICENSES.md (stdout).

The project's own crate is skipped; only third-party crates are listed.
"""
import collections
import json
import sys

OWN_CRATE = "linewright"

data = json.load(sys.stdin)
sections = []
counts = collections.Counter()
for lic in data["licenses"]:
    users = [u["crate"] for u in lic["used_by"] if u["crate"]["name"] != OWN_CRATE]
    if not users:
        continue
    sections.append((lic["name"], users, lic["text"].rstrip()))
    counts[lic["name"]] += len(users)

out = ["## Rust crates", ""]
for name, n in counts.most_common():
    out.append(f"- {name}: {n} crate(s)")
out.append("")
for name, users, text in sections:
    used = ", ".join(f"{c['name']} {c['version']}" for c in users)
    out += [f"### {name}", "", f"Used by: {used}", "", "```text", text, "```", ""]
sys.stdout.write("\n".join(out) + "\n")
