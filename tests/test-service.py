#!/usr/bin/env python3
import http.server
import socketserver
import sys

PORT = 18081


class ReadyHandler(http.server.SimpleHTTPRequestHandler):
    def do_GET(self):
        if self.path == "/ready":
            self.send_error(404)
            return

        self.send_response(200)
        self.send_header("Content-type", "text/plain")
        self.end_headers()
        self.wfile.write(b"Hello from test service")

    def log_message(self, format, *args):
        sys.stderr.write(f"[test-service] {format % args}\n")


with socketserver.TCPServer(("", PORT), ReadyHandler) as httpd:
    print(f"Test service listening on port {PORT}")
    sys.stderr.flush()
    httpd.serve_forever()
