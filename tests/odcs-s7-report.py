"""Reject empty, skipped or partial S7 scenario runs."""
from pathlib import Path
import re
import sys

text = re.sub(r"\x1b\[[0-9;]*m", "", Path(sys.argv[1]).read_text())
for name in ["Authorized contract browsing survives denied observations",
             "Manual query building retains semantic context for review"]:
    assert name in text, name
assert re.search(r"2 scenarios? \(2 passed\)", text), text
assert re.search(r"2 steps? \(2 passed\)", text), text
assert "odcs-catalog-browser OK" in text
print("odcs-s7-report OK: 2 scenarios, 2 steps, zero skips")
