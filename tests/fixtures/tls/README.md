# TLS テスト用のフィクスチャ

`REF:tests/tls.rs` が使う、テスト専用の自己署名 CA とサーバ証明書。

- `ca.pem` / `ca.key`: テスト用のプライベート CA（`SSL_CERT_FILE` に渡して信頼させる）
- `server.crt` / `server.key`: `ca.pem` で署名したサーバ証明書（SAN に `IP:127.0.0.1` を含む）
- `server-chain.pem`: サーバ証明書（`openssl s_server` 等へ渡す形）
- `other-ca.pem`: 無関係な CA。`ca.pem` を信頼していない状態を再現するために使う
- `server.py`: ポート 0 で待ち受け、実際のポートを `port <n>` の 1 行で標準出力に出す HTTPS サーバ

**鍵はテスト専用に生成したもので、実際の認証局・秘密情報ではない。**
外部の通信には使わず、このディレクトリの外へコピーしない。

再生成する場合（有効期間 100 年・SAN は `IP:127.0.0.1`）:

```bash
cd tests/fixtures/tls
openssl req -x509 -newkey rsa:2048 -nodes -keyout ca.key -out ca.pem \
  -days 36500 -subj "/CN=llmsearch-test-ca"
openssl req -newkey rsa:2048 -nodes -keyout server.key -out server.csr \
  -subj "/CN=127.0.0.1"
openssl x509 -req -in server.csr -CA ca.pem -CAkey ca.key -out server.crt \
  -days 36500 -extfile <(printf "subjectAltName=IP:127.0.0.1\n")
```
