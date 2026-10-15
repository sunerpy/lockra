#!/usr/bin/env python3
"""The browser smoke test's sites (scripts/smoke-browser-linux.sh): the pages of scripts/smoke/site/
over HTTPS, with a certificate for every *.test name the run made with openssl, and over plain HTTP;
the browsers resolve every *.test name to 127.0.0.1. Quiet, uncached, and ready once it says so.

Usage: site.py <root> <cert> <key> <https-port> <http-port>
"""

import functools
import http.server
import ssl
import sys
import threading


def many_fields(count=2000):
    """A sign-in form among `count` other fields: the content script's budget on a large page."""
    rows = "\n".join(f'<label>Field {i} <input name="f{i}" /></label>' for i in range(count))
    return (
        '<!doctype html><html lang="en"><head><meta charset="utf-8" />'
        "<title>Two thousand fields · login.test</title></head><body><h1>A long form</h1>"
        '<form id="login" action="/welcome.html" method="get">'
        '<label>User name <input id="username" name="username" autocomplete="username" /></label>'
        '<label>Password <input id="password" name="password" type="password" autocomplete="current-password" /></label>'
        f"{rows}</form></body></html>"
    ).encode()


class Handler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, format, *args):  # noqa: A002 - the base class's name
        pass

    def do_GET(self):  # noqa: N802 - the base class's name
        if self.path.split("?")[0] == "/many.html":
            body = many_fields()
            self.send_response(200)
            self.send_header("Content-Type", "text/html; charset=utf-8")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            return
        super().do_GET()

    def end_headers(self):
        self.send_header("Cache-Control", "no-store")
        super().end_headers()


def serve(server):
    threading.Thread(target=server.serve_forever, daemon=True).start()


def main():
    root, cert, key, https_port, http_port = sys.argv[1:6]
    handler = functools.partial(Handler, directory=root)
    secure = http.server.ThreadingHTTPServer(("127.0.0.1", int(https_port)), handler)
    context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    context.load_cert_chain(cert, key)
    secure.socket = context.wrap_socket(secure.socket, server_side=True)
    plain = http.server.ThreadingHTTPServer(("127.0.0.1", int(http_port)), handler)
    serve(secure)
    serve(plain)
    print("site: ready", flush=True)
    threading.Event().wait()


if __name__ == "__main__":
    main()
