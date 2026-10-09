"""Exact S2 scenario/count assertions; no marker on incomplete evidence."""
import re
import sys
from pathlib import Path

report = re.sub(r"\x1b\[[0-9;]*m", "", Path(sys.argv[1]).read_text())
names = [
    "Compiled intake needs no author files",
    "A multi-object contract retains its identity",
    "Invalid v3.2 does not become legacy",
    "Schema-valid v3.1 is rejected by the support gate",
    "Namespace segments survive adapter and API boundaries",
]
assert re.findall(r"^\s*Scenario: (.+)$", report, re.M) == names, report
assert re.search(r"^5 scenarios \(5 passed\)$", report, re.M), report
assert re.search(r"^5 steps \(5 passed\)$", report, re.M), report
assert not re.search(r"\b(?:failed|skipped|undefined)\b", report, re.I), report
