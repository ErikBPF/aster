"""Small, bounded Trino client for synthetic-demo fixture and live checks."""
import json
import time
import urllib.parse
import urllib.request


def query(endpoint, sql):
    endpoint = endpoint.rstrip("/")
    origin = urllib.parse.urlsplit(endpoint)
    assert origin.scheme in {"http", "https"} and origin.netloc
    request = urllib.request.Request(endpoint + "/v1/statement", data=sql.encode(),
                                     headers={"X-Trino-User": "aster-benchmark", "Content-Type": "text/plain"})
    rows, size, deadline = [], 0, time.monotonic() + 60
    for _ in range(200):
        assert time.monotonic() < deadline, "Trino query deadline exceeded"
        with urllib.request.urlopen(request, timeout=min(15, max(1, deadline - time.monotonic()))) as response:
            body = response.read(2_000_001)
        size += len(body)
        assert len(body) <= 2_000_000 and size <= 8_000_000, "Trino response budget exceeded"
        page = json.loads(body)
        assert "error" not in page, "Trino rejected benchmark query"
        rows.extend(page.get("data", []))
        assert len(rows) <= 10_000, "Trino row budget exceeded"
        next_uri = page.get("nextUri")
        if not next_uri:
            return rows
        target = urllib.parse.urlsplit(next_uri)
        assert (target.scheme, target.netloc) == (origin.scheme, origin.netloc), "off-origin Trino continuation"
        request = urllib.request.Request(next_uri, headers={"X-Trino-User": "aster-benchmark"})
    raise AssertionError("Trino page budget exceeded")
