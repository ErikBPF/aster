"""The canonical gate must own offline proof, not rely on a historical manual run."""
from pathlib import Path

recipe = (Path(__file__).resolve().parent.parent / "justfile").read_text().split("odcs-document-validation:\n", 1)[1].split("\nbackend-identity-validation:", 1)[0]
assert "python3 tests/odcs-offline-validation.py" in recipe, "offline proof missing from canonical S2 gate"
assert recipe.index("python3 tests/odcs-offline-validation.py") < recipe.index("'odcs-document-validation OK'")
