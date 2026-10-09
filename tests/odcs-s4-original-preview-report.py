"""Success-only narrow S4 original-source preview counts; Q4 remains undecided."""
import pathlib
import sys

rust, bdd = (pathlib.Path(path).read_text() for path in sys.argv[1:])
assert "test full_document_preview_preserves_all_content ... ok" in rust
assert "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out;" in rust
assert "1 scenario (1 passed)" in bdd
assert "1 step (1 passed)" in bdd
assert "skipped" not in bdd.lower()
assert "Original ODCS preview preserves all content without observation or execution" in bdd
print("odcs-s4-original-preview-report OK: 1 test; 1 scenario; 1 step; zero skips; whole S4 pending Q4")
