#!/usr/bin/env python3
"""Fetch and project active Toki timer status for the Omarchy bar."""

from __future__ import annotations

import ipaddress
import json
import ssl
import stat
import sys
import urllib.error
import urllib.request
from pathlib import Path
from typing import Final
from urllib.parse import urlsplit

REQUEST_TIMEOUT_SECONDS: Final = 6
MAX_RESPONSE_BYTES: Final = 64 * 1024


class NoRedirectHandler(urllib.request.HTTPRedirectHandler):
    """Prevent Authorization from crossing an HTTP redirect boundary."""

    def redirect_request(self, *args: object, **kwargs: object) -> None:
        return None


def emit(payload: dict[str, object]) -> None:
    """Write one compact protocol response for the QML collector."""

    json.dump(payload, sys.stdout, separators=(",", ":"))
    sys.stdout.write("\n")


def credentials_path(arguments: list[str]) -> Path:
    """Resolve the configured credential file, allowing tests to override it."""

    if arguments and arguments[0].strip():
        return Path(arguments[0]).expanduser()
    return Path.home() / ".config" / "toki" / "credentials"


def load_credentials(path: Path) -> dict[str, str]:
    """Parse the deliberately small key=value credential format."""

    values: dict[str, str] = {}
    for raw_line in path.read_text(encoding="utf-8").splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        values[key.strip()] = value.strip()
    return values


def credentials_are_private(path: Path) -> bool:
    """Require that neither group nor other users can access the token file."""

    mode = stat.S_IMODE(path.stat().st_mode)
    return mode & (stat.S_IRWXG | stat.S_IRWXO) == 0


def validated_api_url(raw_url: str) -> str | None:
    """Accept HTTPS origins and loopback HTTP used during local development."""

    api_url = raw_url.rstrip("/")
    parsed = urlsplit(api_url)
    try:
        parsed.port
    except ValueError:
        return None
    if (
        not parsed.hostname
        or parsed.username is not None
        or parsed.password is not None
        or parsed.query
        or parsed.fragment
    ):
        return None
    if parsed.scheme == "https":
        return api_url
    if parsed.scheme != "http":
        return None

    if parsed.hostname == "localhost":
        return api_url
    try:
        return api_url if ipaddress.ip_address(parsed.hostname).is_loopback else None
    except ValueError:
        return None


def validated_app_url(raw_url: str) -> str:
    """Return a clickable web URL or an empty string for invalid input."""

    app_url = raw_url.rstrip("/")
    parsed = urlsplit(app_url)
    if parsed.scheme not in {"http", "https"} or not parsed.hostname:
        return ""
    if parsed.username is not None or parsed.password is not None:
        return ""
    return app_url


def parse_timer_response(value: object) -> dict[str, object]:
    """Parse unknown JSON into only the timer fields consumed by the widget."""

    if not isinstance(value, dict) or "timer" not in value:
        raise ValueError("timer response must be an object with a timer field")

    timer = value["timer"]
    if timer is None:
        return {"status": "ok", "timer": None}
    if not isinstance(timer, dict) or not isinstance(timer.get("startTime"), str):
        raise ValueError("active timer must contain startTime")

    projected: dict[str, object] = {"startTime": timer["startTime"]}
    for field in ("projectName", "activityName"):
        field_value = timer.get(field)
        if field_value is not None and not isinstance(field_value, str):
            raise ValueError(f"{field} must be a string or null")
        projected[field] = field_value

    note = timer.get("note", "")
    if not isinstance(note, str):
        raise ValueError("note must be a string")
    projected["note"] = note
    return {"status": "ok", "timer": projected}


def fetch_timer(api_url: str, token: str) -> dict[str, object]:
    """Fetch timer status without ever following a credential-bearing redirect."""

    request = urllib.request.Request(
        f"{api_url}/time-tracking/timer",
        headers={
            "Accept": "application/json",
            "Authorization": f"Bearer {token}",
        },
        method="GET",
    )
    opener = urllib.request.build_opener(
        NoRedirectHandler(),
        urllib.request.HTTPSHandler(context=ssl.create_default_context()),
    )

    try:
        with opener.open(request, timeout=REQUEST_TIMEOUT_SECONDS) as response:
            encoded = response.read(MAX_RESPONSE_BYTES + 1)
    except urllib.error.HTTPError as error:
        status = "unauthorized" if error.code in (401, 403) else "error"
        error.close()
        return {"status": status}
    except (urllib.error.URLError, TimeoutError, OSError):
        return {"status": "error"}

    if len(encoded) > MAX_RESPONSE_BYTES:
        return {"status": "error"}
    try:
        return parse_timer_response(json.loads(encoded.decode("utf-8")))
    except (UnicodeDecodeError, json.JSONDecodeError, ValueError):
        return {"status": "error"}


def status(path: Path) -> dict[str, object]:
    """Resolve credentials and return one widget protocol payload."""

    if not path.is_file():
        return {"status": "unconfigured"}
    try:
        if not credentials_are_private(path):
            return {"status": "insecure_credentials"}
        credentials = load_credentials(path)
    except OSError:
        return {"status": "error"}

    raw_api_url = credentials.get("api_url", "")
    token = credentials.get("token", "")
    if not raw_api_url or not token:
        return {"status": "unconfigured"}
    api_url = validated_api_url(raw_api_url)
    if not api_url:
        return {"status": "invalid_api_url"}

    payload = fetch_timer(api_url, token)
    app_url = validated_app_url(credentials.get("app_url", ""))
    if payload.get("status") == "ok" and app_url:
        payload["appUrl"] = app_url
    return payload


def main(arguments: list[str]) -> int:
    """Run the helper protocol once."""

    emit(status(credentials_path(arguments)))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main(sys.argv[1:]))
    except Exception as error:
        print(f"toki timer status failed: {type(error).__name__}", file=sys.stderr)
        emit({"status": "error"})
        raise SystemExit(0)
