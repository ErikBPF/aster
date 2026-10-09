"""Disposable HTTPS current-membership authority; never used outside S3 smoke."""
import http.server
import json
from pathlib import Path
import ssl

GROUP = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"
USER = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb"


class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path.startswith("/api/v3/core/users/"):
            value = {"pagination": {"count": 1}, "results": [{
                "uuid": USER, "is_active": True,
                "groups": [GROUP] if Path("/fixture/member").exists() else []}]}
        elif self.path.startswith(f"/api/v3/core/groups/{GROUP}/"):
            value = {"pk": GROUP, "parents": []}
        else:
            self.send_error(404)
            return
        self.send_response(200)
        body = json.dumps(value).encode()
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        self.wfile.write(body)


server = http.server.HTTPServer(("0.0.0.0", 8443), Handler)
context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
context.load_cert_chain("/fixture/cert.pem", "/fixture/key.pem")
server.socket = context.wrap_socket(server.socket, server_side=True)
server.serve_forever()
