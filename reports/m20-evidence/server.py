#!/usr/bin/env python3
"""127.0.0.1 loopback HTTP stub for llmsearch M20 verification. Never leaves the host.

Endpoints:
  /search        HTML search results (2 hits: one absolute w/ sensitive-looking params,
                 one relative /doc2 without absolutize) -> #58 redaction + relative URL.
  /doc1          HTML with a <p>#hashtag</p>, an <img alt>, and a ~~~sh fenced block
                 -> #60 HTML conversion / ATX / fence.
  /doc2          small plain document.
  /e401          always 401 Unauthorized.
  /e403          always 403 Forbidden.
  /caching       returns 200 HTML; each hit logged (cache TTL test).
  /robots.txt    allow-all.
Every request is logged as "METHOD PATH" to /tmp/m20/requests.log (line buffered).
"""
import http.server
import socketserver

LOG = open("/tmp/m20/requests.log", "a", buffering=1)


def search_html(port):
    abs_url = "http://127.0.0.1:%d/doc1?author=alice&keyword=climate&design=modern" % port
    items = (
        '<li><a href="%s">First result</a></li>'
        '<li><a href="/doc2">Relative result</a></li>' % (abs_url,)
    )
    return "<html><body><ul>" + items + "</ul></body></html>"


def doc1_html(port):
    # Paragraph containing a line-leading '#' hashtag (no space) that must NOT become a heading.
    # An <img> with alt text that must NOT be preserved.
    # A ~~~sh fenced block, long enough that --max-chars truncates it mid-fence.
    code = "\n".join("echo line_%04d payload_padding_padding_padding" % i for i in range(40))
    return (
        "<html><body>"
        "<h1>Main title</h1>"
        "<p>Intro paragraph with some body text for the document section one.</p>"
        "<p>#hashtag_without_space is not a heading</p>"
        '<img src="http://127.0.0.1:%d/img.png" alt="preserved-alt-text" />'
        "<h2>Second section</h2>"
        "<p>Body of the second section with more text to fill the budget.</p>"
        "<pre>~~~sh\n" + code + "\n~~~</pre>"
        "</body></html>"
    ) % port


def doc2_html():
    return "<html><body><h1>Doc two</h1><p>Plain body.</p></body></html>"


def doc3_html():
    # Stable parent/child nesting for the includeChildren test (#60-4).
    return (
        "<html><body>"
        "<h1>Top section</h1>"
        "<p>top body text for the parent section.</p>"
        "<h2>Child A</h2><p>child a body text here.</p>"
        "<h2>Child B</h2><p>child b body text here.</p>"
        "</body></html>"
    )


def caching_html():
    return "<html><body><h1>Caching</h1><p>Body that is cached then re-fetched.</p></body></html>"


class Handler(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *a):
        pass

    def _send(self, status, body, ctype="text/html; charset=utf-8"):
        self.send_response(status)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        LOG.write("GET " + self.path + "\n")
        port = self.server.server_address[1]
        p = self.path.split("?")[0]
        if p == "/search":
            self._send(200, search_html(port).encode())
        elif p == "/doc1":
            self._send(200, doc1_html(port).encode())
        elif p == "/doc2":
            self._send(200, doc2_html().encode())
        elif p == "/doc3":
            self._send(200, doc3_html().encode())
        elif p == "/e401":
            self._send(401, b"Unauthorized")
        elif p == "/e403":
            self._send(403, b"Forbidden")
        elif p == "/caching":
            self._send(200, caching_html().encode())
        elif p == "/robots.txt":
            self._send(200, b"User-agent: *\nAllow: /\n", "text/plain")
        else:
            self._send(200, b"<html><body>ok</body></html>")


class Server(socketserver.ThreadingTCPServer):
    allow_reuse_address = True
    daemon_threads = True


with Server(("127.0.0.1", 0), Handler) as httpd:
    port = httpd.server_address[1]
    with open("/tmp/m20/port", "w") as f:
        f.write(str(port))
    print("READY", port, flush=True)
    httpd.serve_forever()
