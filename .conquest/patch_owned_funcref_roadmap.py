from pathlib import Path

path = Path("docs/roadmap.md")
text = path.read_text()
old = "- [x] nullable `funcref` defined-function params/results/locals and direct calls, with reference-valued host functions/globals and unowned non-null embedding arguments explicitly fail-closed pending instance-ownership semantics\n"
new = (
    "- [x] nullable `funcref` defined-function params/results/locals and direct calls, with reference-valued host functions/globals\n"
    "- [x] ownership-checked non-null `funcref` embedding handles with same-instance round-trips, foreign/expired-handle fail-closed checks, and legacy raw-index non-null arguments remaining rejected\n"
)
if text.count(old) != 1:
    raise SystemExit(f"roadmap ownership anchor: expected one, found {text.count(old)}")
path.write_text(text.replace(old, new, 1))
