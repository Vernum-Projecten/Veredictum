# SPDX-FileCopyrightText: Veredictum contributors
# SPDX-License-Identifier: Apache-2.0
"""A fake server under test for the GitHub Action's CI job (#770).

It answers OPTIONS on any path with 200, an Allow header and an empty JSON
object, which passes every System API workflow, and every other method with
404, which fails the EHR workflows. That gives the job a run that exits 0 and
a run that exits 1 from one server. It is not an openEHR server and judges
nothing.

Usage: python3 stub-sut.py <port>
"""

import http.server
import sys


class Handler(http.server.BaseHTTPRequestHandler):
    def _send(self, code, body=b"", headers=()):
        self.send_response(code)
        for name, value in headers:
            self.send_header(name, value)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        if self.command != "HEAD":
            self.wfile.write(body)

    def do_OPTIONS(self):
        self._send(
            200,
            b"{}",
            [("Allow", "GET, POST, PUT, DELETE, OPTIONS"), ("Content-Type", "application/json")],
        )

    def _absent(self):
        self._send(404)

    do_GET = do_POST = do_PUT = do_DELETE = do_HEAD = do_PATCH = _absent


http.server.ThreadingHTTPServer(("127.0.0.1", int(sys.argv[1])), Handler).serve_forever()
