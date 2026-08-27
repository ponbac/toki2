#!/usr/bin/env python3
"""Talk to Toki's time-tracking API for the Omarchy bar widget."""

from __future__ import annotations

import argparse
import ipaddress
import json
import ssl
import stat
import sys
import urllib.error
import urllib.parse
import urllib.request
from datetime import date, timedelta
from pathlib import Path
from typing import Final
from urllib.parse import urlsplit

REQUEST_TIMEOUT_SECONDS: Final = 6
MAX_RESPONSE_BYTES: Final = 256 * 1024
MAX_NOTE_CHARS: Final = 4000
MAX_FIELD_CHARS: Final = 200
RECENT_LIMIT: Final = 8
WEEKDAY_LABELS: Final = ("M", "T", "W", "T", "F", "S", "S")
TIMER_FIELDS: Final = (
    "userNote",
    "projectId",
    "projectName",
    "activityId",
    "activityName",
)


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

    if arguments and arguments[0].strip() and not arguments[0].startswith("-"):
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
    for field in ("projectId", "projectName", "activityId", "activityName"):
        field_value = timer.get(field)
        if field_value is not None and not isinstance(field_value, str):
            raise ValueError(f"{field} must be a string or null")
        projected[field] = field_value

    note = timer.get("note", "")
    if not isinstance(note, str):
        raise ValueError("note must be a string")
    projected["note"] = note
    return {"status": "ok", "timer": projected}


def iso_week_bounds(today: date | None = None) -> tuple[date, date]:
    """Return the Monday–Sunday span containing `today`."""

    today = today or date.today()
    monday = today - timedelta(days=today.weekday())
    return monday, monday + timedelta(days=6)


def aggregate_day_hours(
    entries: list[dict[str, object]], monday: date, today: date
) -> list[dict[str, object]]:
    """Sum entry hours onto the seven days of the week starting `monday`."""

    hours_by_date = {
        (monday + timedelta(days=offset)).isoformat(): 0.0 for offset in range(7)
    }
    for entry in entries:
        day_key = entry.get("date")
        raw_hours = entry.get("hours")
        if not isinstance(day_key, str) or day_key not in hours_by_date:
            continue
        try:
            hours_by_date[day_key] += float(raw_hours)
        except (TypeError, ValueError):
            continue

    days: list[dict[str, object]] = []
    for offset in range(7):
        day = monday + timedelta(days=offset)
        key = day.isoformat()
        days.append(
            {
                "date": key,
                "weekday": offset,
                "label": WEEKDAY_LABELS[offset],
                "hours": round(hours_by_date[key], 4),
                "today": day == today,
            }
        )
    return days


def project_recents(entries: object) -> list[dict[str, object]]:
    """Keep a short unique list of recent project/activity/note triples."""

    if not isinstance(entries, list):
        return []

    recents: list[dict[str, object]] = []
    seen: set[tuple[str, str, str]] = set()
    for entry in entries:
        if not isinstance(entry, dict):
            continue
        project_id = entry.get("projectId")
        project_name = entry.get("projectName")
        activity_id = entry.get("activityId")
        activity_name = entry.get("activityName")
        if not isinstance(project_id, str) or not isinstance(project_name, str):
            continue
        if not isinstance(activity_id, str) or not isinstance(activity_name, str):
            continue
        note = entry.get("note")
        note_text = note if isinstance(note, str) else ""
        fingerprint = (project_id, activity_id, note_text)
        if fingerprint in seen:
            continue
        seen.add(fingerprint)
        recents.append(
            {
                "projectId": project_id,
                "projectName": project_name,
                "activityId": activity_id,
                "activityName": activity_name,
                "note": note_text,
            }
        )
        if len(recents) >= RECENT_LIMIT:
            break
    return recents


def parse_week_stats(value: object) -> dict[str, float]:
    """Pick the week totals the meter needs."""

    if not isinstance(value, dict):
        raise ValueError("time-info must be an object")
    stats: dict[str, float] = {}
    for field in (
        "workedHours",
        "scheduledHours",
        "remainingHours",
        "periodFlexHours",
    ):
        try:
            stats[field] = float(value.get(field, 0))
        except (TypeError, ValueError) as error:
            raise ValueError(f"{field} must be a number") from error
    return stats


def parse_time_entries(value: object) -> list[dict[str, object]]:
    """Accept a JSON array of time entries, ignoring malformed rows."""

    if not isinstance(value, list):
        raise ValueError("time entries must be an array")
    entries: list[dict[str, object]] = []
    for row in value:
        if isinstance(row, dict):
            entries.append(row)
    return entries


def parse_projects(value: object) -> list[dict[str, str]]:
    """Project list responses down to id + name."""

    if not isinstance(value, list):
        raise ValueError("projects must be an array")
    projects: list[dict[str, str]] = []
    for row in value:
        if not isinstance(row, dict):
            continue
        project_id = row.get("projectId")
        project_name = row.get("projectName")
        if isinstance(project_id, str) and isinstance(project_name, str):
            projects.append({"projectId": project_id, "projectName": project_name})
    return projects


def parse_activities(value: object) -> list[dict[str, str]]:
    """Normalize activity id from the API's `activity` field."""

    if not isinstance(value, list):
        raise ValueError("activities must be an array")
    activities: list[dict[str, str]] = []
    for row in value:
        if not isinstance(row, dict):
            continue
        activity_id = row.get("activity")
        activity_name = row.get("activityName")
        if isinstance(activity_id, str) and isinstance(activity_name, str):
            activities.append(
                {"activityId": activity_id, "activityName": activity_name}
            )
    return activities


def sanitized_timer_fields(payload: object) -> dict[str, str]:
    """Allow only the timer fields the API accepts, with length caps."""

    if not isinstance(payload, dict):
        return {}
    fields: dict[str, str] = {}
    for key in TIMER_FIELDS:
        value = payload.get(key)
        if not isinstance(value, str):
            continue
        limit = MAX_NOTE_CHARS if key == "userNote" else MAX_FIELD_CHARS
        trimmed = value[:limit]
        if trimmed or key == "userNote":
            fields[key] = trimmed
    return fields


def _opener() -> urllib.request.OpenerDirector:
    return urllib.request.build_opener(
        NoRedirectHandler(),
        urllib.request.HTTPSHandler(context=ssl.create_default_context()),
    )


def api_call(
    api_url: str,
    token: str,
    method: str,
    path: str,
    body: dict[str, object] | None = None,
) -> tuple[str, object | None]:
    """Perform one API call. Never follows redirects. Returns (kind, json)."""

    encoded_body = None if body is None else json.dumps(body).encode("utf-8")
    headers = {
        "Accept": "application/json",
        "Authorization": f"Bearer {token}",
    }
    if encoded_body is not None:
        headers["Content-Type"] = "application/json"
    request = urllib.request.Request(
        f"{api_url}{path}",
        data=encoded_body,
        headers=headers,
        method=method,
    )
    try:
        with _opener().open(request, timeout=REQUEST_TIMEOUT_SECONDS) as response:
            encoded = response.read(MAX_RESPONSE_BYTES + 1)
    except urllib.error.HTTPError as error:
        status = "unauthorized" if error.code in (401, 403) else "error"
        error.close()
        return status, None
    except (urllib.error.URLError, TimeoutError, OSError):
        return "error", None

    if len(encoded) > MAX_RESPONSE_BYTES:
        return "error", None
    if not encoded:
        return "ok", None
    try:
        return "ok", json.loads(encoded.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError):
        return "error", None


def fetch_timer(api_url: str, token: str) -> dict[str, object]:
    """Fetch timer status without ever following a credential-bearing redirect."""

    kind, parsed = api_call(api_url, token, "GET", "/time-tracking/timer")
    if kind != "ok":
        return {"status": kind}
    try:
        return parse_timer_response(parsed)
    except ValueError:
        return {"status": "error"}


def resolve_session(path: Path) -> dict[str, object]:
    """Load credentials or return a protocol error payload."""

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
    return {
        "status": "ok",
        "api_url": api_url,
        "token": token,
        "app_url": validated_app_url(credentials.get("app_url", "")),
    }


def snapshot(api_url: str, token: str, app_url: str) -> dict[str, object]:
    """Timer + this week's hours + recents, in one protocol payload."""

    payload = fetch_timer(api_url, token)
    if payload.get("status") != "ok":
        return payload
    if app_url:
        payload["appUrl"] = app_url

    today = date.today()
    monday, sunday = iso_week_bounds(today)
    query = urllib.parse.urlencode(
        {"from": monday.isoformat(), "to": sunday.isoformat()}
    )
    unique_from = (today - timedelta(days=21)).isoformat()
    unique_query = urllib.parse.urlencode(
        {"from": unique_from, "to": sunday.isoformat(), "unique": "true"}
    )

    info_kind, info_body = api_call(
        api_url, token, "GET", f"/time-tracking/time-info?{query}"
    )
    entries_kind, entries_body = api_call(
        api_url, token, "GET", f"/time-tracking/time-entries?{query}"
    )
    recents_kind, recents_body = api_call(
        api_url, token, "GET", f"/time-tracking/time-entries?{unique_query}"
    )

    week: dict[str, object] = {
        "workedHours": 0.0,
        "scheduledHours": 40.0,
        "remainingHours": 40.0,
        "periodFlexHours": 0.0,
        "days": aggregate_day_hours([], monday, today),
    }
    if info_kind == "ok":
        try:
            week.update(parse_week_stats(info_body))
        except ValueError:
            pass
    if entries_kind == "ok":
        try:
            week["days"] = aggregate_day_hours(
                parse_time_entries(entries_body), monday, today
            )
        except ValueError:
            pass
    payload["week"] = week
    payload["recents"] = (
        project_recents(recents_body) if recents_kind == "ok" else []
    )
    return payload


def list_projects_payload(api_url: str, token: str) -> dict[str, object]:
    kind, body = api_call(api_url, token, "GET", "/time-tracking/projects")
    if kind != "ok":
        return {"status": kind}
    try:
        return {"status": "ok", "projects": parse_projects(body)}
    except ValueError:
        return {"status": "error"}


def list_activities_payload(
    api_url: str, token: str, project_id: str
) -> dict[str, object]:
    if not project_id or len(project_id) > MAX_FIELD_CHARS:
        return {"status": "error"}
    encoded = urllib.parse.quote(project_id, safe="")
    kind, body = api_call(
        api_url,
        token,
        "GET",
        f"/time-tracking/projects/{encoded}/activities",
    )
    if kind != "ok":
        return {"status": kind}
    try:
        return {"status": "ok", "activities": parse_activities(body)}
    except ValueError:
        return {"status": "error"}


def mutate(
    api_url: str,
    token: str,
    method: str,
    path: str,
    body: dict[str, object] | None,
    app_url: str,
) -> dict[str, object]:
    kind, _parsed = api_call(api_url, token, method, path, body)
    if kind != "ok":
        return {"status": kind}
    return snapshot(api_url, token, app_url)


def status(path: Path) -> dict[str, object]:
    """Resolve credentials and return a snapshot for the widget."""

    session = resolve_session(path)
    if session.get("status") != "ok":
        return {"status": session["status"]}
    return snapshot(str(session["api_url"]), str(session["token"]), str(session["app_url"]))


def parse_cli(arguments: list[str]) -> argparse.Namespace:
    """Parse helper flags. A bare positional is still the credentials path."""

    parser = argparse.ArgumentParser(add_help=False)
    parser.add_argument("credentials", nargs="?", default="")
    parser.add_argument("--action", default="snapshot")
    parser.add_argument("--payload", default="")
    parser.add_argument("--project-id", default="")
    return parser.parse_args(arguments)


def dispatch(arguments: list[str]) -> dict[str, object]:
    """Run one helper action and return the protocol payload."""

    parsed = parse_cli(arguments)
    path = credentials_path([parsed.credentials] if parsed.credentials else [])
    action = parsed.action.strip() or "snapshot"
    session = resolve_session(path)
    if session.get("status") != "ok":
        return {"status": session["status"]}

    api_url = str(session["api_url"])
    token = str(session["token"])
    app_url = str(session["app_url"])

    if action == "snapshot":
        return snapshot(api_url, token, app_url)
    if action == "projects":
        return list_projects_payload(api_url, token)
    if action == "activities":
        return list_activities_payload(api_url, token, parsed.project_id)
    if action == "start":
        payload = json.loads(parsed.payload) if parsed.payload else {}
        return mutate(
            api_url,
            token,
            "POST",
            "/time-tracking/timer",
            sanitized_timer_fields(payload),
            app_url,
        )
    if action == "save":
        payload = json.loads(parsed.payload) if parsed.payload else {}
        body: dict[str, object] = {}
        fields = sanitized_timer_fields(payload)
        if "userNote" in fields:
            body["userNote"] = fields["userNote"]
        return mutate(api_url, token, "PUT", "/time-tracking/timer", body, app_url)
    if action == "stop":
        return mutate(api_url, token, "DELETE", "/time-tracking/timer", None, app_url)
    if action == "update":
        payload = json.loads(parsed.payload) if parsed.payload else {}
        return mutate(
            api_url,
            token,
            "PUT",
            "/time-tracking/update-timer",
            sanitized_timer_fields(payload),
            app_url,
        )
    return {"status": "error"}


def main(arguments: list[str]) -> int:
    """Run the helper protocol once."""

    try:
        payload = dispatch(arguments)
    except (json.JSONDecodeError, ValueError, OSError):
        payload = {"status": "error"}
    emit(payload)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main(sys.argv[1:]))
    except Exception as error:
        print(f"toki timer status failed: {type(error).__name__}", file=sys.stderr)
        emit({"status": "error"})
        raise SystemExit(0)
