#!/usr/bin/env python3
import http.server
import socketserver
import sys

PORT = int(sys.argv[1]) if len(sys.argv) > 1 else 18081
SERVICE_NAME = sys.argv[2] if len(sys.argv) > 2 else "test service"


class ReadyHandler(http.server.SimpleHTTPRequestHandler):
    def do_GET(self):
        if self.path == "/ready":
            self.send_error(404)
            return

        self.send_response(200)
        self.send_header("Content-type", "text/plain")
        self.end_headers()
        self.wfile.write(f"Hello from {SERVICE_NAME}".encode())

    def log_message(self, format, *args):
        sys.stderr.write(f"[test-service] {format % args}\n")


with socketserver.TCPServer(("", PORT), ReadyHandler) as httpd:
    print(f"{SERVICE_NAME} listening on port {PORT}")
    sys.stderr.flush()
    httpd.serve_forever()
