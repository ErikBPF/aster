"""Deterministic local assistant; no inference, outbound calls, or tool execution."""
import json
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

SQL = 'SELECT order_id, customer, region, total, placed_at\nFROM "mock-local".aster_demo.orders\nLIMIT 10;'


def reply(body):
    if body.get('model') != 'aster-mock-assistant':
        raise ValueError('unknown model')
    messages = body.get('messages', [])
    guidance = '\n'.join(m.get('content', '') for m in messages if m.get('role') == 'system')
    if 'ASTER_DEMO_READ_ONLY_V1' in guidance and 'ASTER_DEMO_ORDERS_V1' in guidance:
        content = 'Mock assistant: received the read-only skill and orders reference (version 1). Review this synthetic demo query; it has not been executed.\n\n```sql\n' + SQL + '\n```'
    else:
        content = 'Mock assistant: enable the Demo orders SQL skill (version 1), including its orders reference, then ask again.'
    return {'choices': [{'message': {'role': 'assistant', 'content': content}, 'finish_reason': 'stop'}]}


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def do_POST(self):
        status = 200
        try:
            length = int(self.headers.get('Content-Length', '0'))
            if self.path != '/v1/chat/completions' or self.headers.get('Origin') or not 0 < length <= 262144:
                raise ValueError('invalid request')
            result = reply(json.loads(self.rfile.read(length)))
        except (ValueError, TypeError, KeyError, AttributeError):
            status, result = 400, {'error': 'Invalid mock assistant request'}
        data = json.dumps(result).encode()
        self.send_response(status)
        self.send_header('Content-Type', 'application/json')
        self.send_header('Content-Length', str(len(data)))
        self.end_headers()
        self.wfile.write(data)


if __name__ == '__main__':
    ThreadingHTTPServer(('127.0.0.1', 7778), Handler).serve_forever()
