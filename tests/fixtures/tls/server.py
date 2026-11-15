#!/usr/bin/env python3
"""ローカル HTTPS サーバー（プライベート CA の証明書を使用）。

`tests/tls.rs` から起動される。外部ネットワークは使わない。
証明書は `tests/fixtures/tls/` に置いた自己署名のテスト用である。

使い方: python3 server.py <cert> <key>
標準出力へ `port <番号>` を 1 行出力してから待ち受ける。
"""
import http.server
import socketserver
import ssl
import sys

BODY = (
    "<html lang=\"ja\"><head><title>TLS fixture</title></head><body>"
    "<main><article><h1>ルート証明書の検証</h1>"
    "<p>この文書はプライベート CA で署名したローカルサーバーから取得したものである。"
    "本文は十分な長さを持つように書かれている。</p>"
    "<h2>1. 検証</h2><p>CA が信頼されていれば取得できる。</p>"
    "</article></main></body></html>"
).encode("utf-8")


class Handler(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *args):
        pass

    def do_GET(self):
        self.send_response(200)
        self.send_header("Content-Type", "text/html; charset=utf-8")
        self.send_header("Content-Length", str(len(BODY)))
        self.end_headers()
        self.wfile.write(BODY)


class Server(socketserver.ThreadingTCPServer):
    allow_reuse_address = True
    daemon_threads = True


def main() -> None:
    cert, key = sys.argv[1], sys.argv[2]
    server = Server(("127.0.0.1", 0), Handler)
    context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    context.load_cert_chain(cert, key)
    server.socket = context.wrap_socket(server.socket, server_side=True)
    print("port %d" % server.server_address[1], flush=True)
    server.serve_forever()


if __name__ == "__main__":
    main()
