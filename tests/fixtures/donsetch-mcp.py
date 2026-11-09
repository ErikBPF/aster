"""Loopback-only Donsetch MCP fixture. Every action is synthetic; no external I/O."""
import argparse
import json
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

TOOLS = [
    {"name": "inspect_catalog", "description": "Read a synthetic catalog summary. No real data is accessed.",
     "inputSchema": {"type": "object", "properties": {}, "additionalProperties": False}},
    {"name": "draft_note", "description": "Simulate creating a note; returns a mock receipt only.",
     "inputSchema": {"type": "object", "properties": {"text": {"type": "string", "maxLength": 500}}, "required": ["text"], "additionalProperties": False}},
]

class Handler(BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def respond(self, status, body=None):
        data = json.dumps(body).encode() if body is not None else b''
        self.send_response(status)
        self.send_header('Content-Type', 'application/json')
        self.send_header('Content-Length', str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_GET(self):
        self.respond(405)

    def do_DELETE(self):
        self.respond(200)

    def do_POST(self):
        if self.path != '/mcp' or self.headers.get('Origin'):
            self.respond(403)
            return
        try:
            length = int(self.headers.get('Content-Length', '0'))
            if not 0 < length <= 65536:
                raise ValueError()
            body = json.loads(self.rfile.read(length))
            method = body['method']
            if method.startswith('notifications/'):
                self.respond(202)
                return
            if method == 'initialize':
                result = {'protocolVersion': '2025-03-26', 'capabilities': {'tools': {}}, 'serverInfo': {'name': 'Donsetch mock', 'version': '1'}}
            elif method == 'tools/list':
                result = {'tools': TOOLS}
            elif method == 'tools/call':
                params = body['params']
                arguments = params.get('arguments', {})
                if params['name'] == 'inspect_catalog' and arguments == {}:
                    text = 'Donsetch mock: 3 synthetic tables, 12 columns. No production catalog queried.'
                elif params['name'] == 'draft_note' and set(arguments) == {'text'} and isinstance(arguments['text'], str) and len(arguments['text']) <= 500:
                    text = 'Donsetch mock receipt: note simulated. No external changes made. Text: ' + arguments['text']
                else:
                    raise ValueError()
                result = {'content': [{'type': 'text', 'text': text}]}
            else:
                raise ValueError()
            self.respond(200, {'jsonrpc': '2.0', 'id': body['id'], 'result': result})
        except (ValueError, KeyError, TypeError):
            self.respond(400, {'error': 'Invalid mock request'})

if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--port', type=int, default=7777)
    args = parser.parse_args()
    print(f'Donsetch mock MCP: http://127.0.0.1:{args.port}/mcp', flush=True)
    ThreadingHTTPServer(('127.0.0.1', args.port), Handler).serve_forever()
