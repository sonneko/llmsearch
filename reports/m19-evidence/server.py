#!/usr/bin/env python3
"""127.0.0.1 loopback HTTP stub for llmsearch M19 verification. Never leaves the host."""
import http.server
import socketserver
import sys
import threading

LOG = open("/tmp/m19/requests.log", "a", buffering=1)

def search_html():
    items = []
    for i in range(1, 6):
        # Each result URL carries a secret-bearing query param (token) + a normal one.
        items.append(
            '<li><a href="/doc?n=%d&token=TOKVAL482%d&keep=normal%d">Result %d</a></li>'
            % (i, i, i, i)
        )
    return "<html><body><ul>" + "".join(items) + "</ul></body></html>"

def doc_html():
    return (
        "<html><body>"
        "<h1>Adaptation plan</h1>"
        "<p>Introductory paragraph about climate adaptation strategies in the region.</p>"
        "<h2>Water management</h2>"
        "<p>Details on water management, with several sentences of body text to give the "
        "section enough characters for a meaningful outline and read output.</p>"
        "<h2>Coastal defenses</h2>"
        "<p>More body text about coastal defenses and engineering measures for protection.</p>"
        "</body></html>"
    )

class Handler(http.server.BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def do_GET(self):
        LOG.write(self.path + "\n")
        if self.path.startswith("/search"):
            body = search_html().encode()
            ctype = "text/html; charset=utf-8"
        elif self.path.startswith("/doc"):
            body = doc_html().encode()
            ctype = "text/html; charset=utf-8"
        else:
            body = b"<html><body>ok</body></html>"
            ctype = "text/html; charset=utf-8"
        self.send_response(200)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

with socketserver.TCPServer(("127.0.0.1", 0), Handler) as httpd:
    port = httpd.server_address[1]
    with open("/tmp/m19/port", "w") as f:
        f.write(str(port))
    print("READY", port, flush=True)
    httpd.serve_forever()
