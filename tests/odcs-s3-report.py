"""Exact S3 scenario/count assertions; incomplete evidence never passes."""
import re
import sys
from pathlib import Path

report = re.sub(r"\x1b\[[0-9;]*m", "", Path(sys.argv[1]).read_text())
names = [
    "Colliding table names do not select the first catalog",
    "Declared and observed fields remain distinguishable",
    "Existing team grants cover the whole contract",
    "Contract access is denied without a granted team membership",
    "Query preparation exposes selected meaning and unresolved bindings",
]
assert re.findall(r"^\s*Scenario: (.+)$", report, re.M) == names, report
assert re.search(r"^5 scenarios \(5 passed\)$", report, re.M), report
assert re.search(r"^5 steps \(5 passed\)$", report, re.M), report
assert not re.search(r"\b(?:failed|skipped|undefined)\b", report, re.I), report
