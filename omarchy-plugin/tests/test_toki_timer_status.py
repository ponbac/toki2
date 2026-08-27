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
                    "projectId": None,
                    "projectName": "Toki",
                    "activityId": None,
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

    def test_week_bounds_are_monday_through_sunday(self) -> None:
        from datetime import date

        monday, sunday = timer_status.iso_week_bounds(date(2026, 8, 27))
        self.assertEqual(monday.isoformat(), "2026-08-24")
        self.assertEqual(sunday.isoformat(), "2026-08-30")
        self.assertEqual(monday.weekday(), 0)
        self.assertEqual(sunday.weekday(), 6)

    def test_day_hours_include_weekend_when_registered(self) -> None:
        from datetime import date

        monday = date(2026, 8, 24)
        days = timer_status.aggregate_day_hours(
            [
                {"date": "2026-08-24", "hours": 8},
                {"date": "2026-08-29", "hours": 2.5},
            ],
            monday,
            date(2026, 8, 27),
        )
        self.assertEqual(len(days), 7)
        self.assertEqual(days[0]["label"], "M")
        self.assertEqual(days[0]["hours"], 8.0)
        self.assertFalse(days[0]["today"])
        self.assertEqual(days[3]["label"], "T")
        self.assertTrue(days[3]["today"])
        self.assertEqual(days[5]["label"], "S")
        self.assertEqual(days[5]["hours"], 2.5)
        self.assertEqual(days[6]["hours"], 0.0)

    def test_recents_are_unique_project_activity_notes(self) -> None:
        recents = timer_status.project_recents(
            [
                {
                    "projectId": "p1",
                    "projectName": "Toki",
                    "activityId": "a1",
                    "activityName": "Backend",
                    "note": "panel",
                },
                {
                    "projectId": "p1",
                    "projectName": "Toki",
                    "activityId": "a1",
                    "activityName": "Backend",
                    "note": "panel",
                },
                {
                    "projectId": "p2",
                    "projectName": "Kleer",
                    "activityId": "a2",
                    "activityName": "Mapping",
                    "note": None,
                },
            ]
        )
        self.assertEqual(
            recents,
            [
                {
                    "projectId": "p1",
                    "projectName": "Toki",
                    "activityId": "a1",
                    "activityName": "Backend",
                    "note": "panel",
                },
                {
                    "projectId": "p2",
                    "projectName": "Kleer",
                    "activityId": "a2",
                    "activityName": "Mapping",
                    "note": "",
                },
            ],
        )

    def test_timer_fields_are_allowlisted_and_capped(self) -> None:
        fields = timer_status.sanitized_timer_fields(
            {
                "userNote": "ok",
                "projectId": "p1",
                "extra": "nope",
                "activityId": 12,
            }
        )
        self.assertEqual(fields, {"userNote": "ok", "projectId": "p1"})
        self.assertEqual(
            timer_status.sanitized_timer_fields({"userNote": ""}),
            {"userNote": ""},
        )

    def test_cli_keeps_credentials_path_out_of_action_flags(self) -> None:
        parsed = timer_status.parse_cli(
            ["/tmp/creds", "--action", "start", "--payload", "{}"]
        )
        self.assertEqual(parsed.credentials, "/tmp/creds")
        self.assertEqual(parsed.action, "start")
        self.assertEqual(parsed.payload, "{}")


if __name__ == "__main__":
    unittest.main()
