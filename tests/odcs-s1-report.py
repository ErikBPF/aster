"""Reject missing, skipped, duplicate or failed S1 scenarios before a gate marker."""
import re
import sys
from pathlib import Path

report = re.sub(r"\x1b\[[0-9;]*m", "", Path(sys.argv[1]).read_text())
names = [
    "Denied metadata stays off the wire",
    "Mixed unbound catalogs are never retrieved",
    "Notebook index belongs to the admitted workspace",
]
assert re.findall(r"^\s*Scenario: (.+)$", report, re.M) == names, report
assert re.search(r"^3 scenarios \(3 passed\)$", report, re.M), report
assert re.search(r"^3 steps \(3 passed\)$", report, re.M), report
assert not re.search(r"\b(?:failed|skipped|undefined)\b", report, re.I), report
