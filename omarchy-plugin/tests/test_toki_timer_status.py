from __future__ import annotations

import sys
import tempfile
import threading
import unittest
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path
from unittest.mock import patch

PLUGIN_DIR = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(PLUGIN_DIR))

import toki_timer_status as timer_status


class TimerStatusTest(unittest.TestCase):
    def test_missing_credentials_are_unconfigured(self) -> None:
        self.assertEqual(
            timer_status.status(Path("/tmp/definitely-missing-toki-credentials")),
            {"status": "unconfigured"},
        )

    def test_group_readable_credentials_are_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "credentials"
            path.write_text("api_url=https://api.example\ntoken=toki_secret\n")
            path.chmod(0o640)

            self.assertEqual(
                timer_status.status(path),
                {"status": "insecure_credentials"},
            )

    def test_remote_plain_http_api_is_rejected_before_fetch(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "credentials"
            path.write_text("api_url=http://api.example\ntoken=toki_secret\n")
            path.chmod(0o600)

            with patch.object(timer_status, "fetch_timer") as fetch:
                self.assertEqual(
                    timer_status.status(path),
                    {"status": "invalid_api_url"},
                )
                fetch.assert_not_called()

    def test_api_url_rejects_query_fragment_and_malformed_port(self) -> None:
        for value in (
            "https://api.example?tenant=other",
            "https://api.example#other",
            "https://api.example:not-a-port",
        ):
            with self.subTest(value=value):
                self.assertIsNone(timer_status.validated_api_url(value))

    def test_active_timer_response_is_projected(self) -> None:
        self.assertEqual(
            timer_status.parse_timer_response(
                {
                    "timer": {
                        "startTime": "2026-08-22T08:30:00Z",
                        "projectName": "Toki",
                        "activityName": None,
                        "note": "Review",
                        "ignored": "value",
                    }
                }
            ),
            {
                "status": "ok",
                "timer": {
                    "startTime": "2026-08-22T08:30:00Z",
                    "projectName": "Toki",
                    "activityName": None,
                    "note": "Review",
                },
            },
        )

    def test_fetch_does_not_forward_authorization_across_redirect(self) -> None:
        sink_called = threading.Event()
        source_authorization: list[str | None] = []

        class QuietHandler(BaseHTTPRequestHandler):
            def log_message(self, format: str, *args: object) -> None:
                pass

        class SinkHandler(QuietHandler):
            def do_GET(self) -> None:
                sink_called.set()
                self.send_response(200)
                self.end_headers()
                self.wfile.write(b'{"timer":null}')

        sink = HTTPServer(("127.0.0.1", 0), SinkHandler)
        sink_url = f"http://127.0.0.1:{sink.server_port}/captured"

        class RedirectHandler(QuietHandler):
            def do_GET(self) -> None:
                source_authorization.append(self.headers.get("Authorization"))
                self.send_response(302)
                self.send_header("Location", sink_url)
                self.end_headers()

        source = HTTPServer(("127.0.0.1", 0), RedirectHandler)
        threads = [
            threading.Thread(target=server.serve_forever, daemon=True)
            for server in (source, sink)
        ]
        for thread in threads:
            thread.start()

        try:
            payload = timer_status.fetch_timer(
                f"http://127.0.0.1:{source.server_port}", "toki_secret"
            )
        finally:
            source.shutdown()
            sink.shutdown()
            source.server_close()
            sink.server_close()

        self.assertEqual(payload, {"status": "error"})
        self.assertEqual(source_authorization, ["Bearer toki_secret"])
        self.assertFalse(sink_called.is_set())


if __name__ == "__main__":
    unittest.main()
