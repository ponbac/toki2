# Kleer Integration Notes

Permanent notes for Toki's Kleer time-tracking integration. Read this before changing Kleer-specific code, API mapping, credentials, user mapping, or time-tracking behavior.

## Overview

- Kleer is the current time-tracking provider behind Toki's provider-agnostic time-tracking ports.
- Domain logic must stay provider-agnostic. Keep Kleer request/response details in the `kleer/` crate and `toki-api/src/adapters/outbound/kleer/`.
- Toki remains project-based for the first Kleer-backed release. Projectless or absence-only Kleer events are intentionally skipped.
- Timer history is still stored locally in Toki and merged with provider data for accurate start/end times.

## Important URLs

- Production API: `https://api.kleer.se/v1`
- Test API: `https://test-api.kleer.se/v1` (isolated API storage; entries written here do not appear in the normal `my.kleer.se` admin UI)
- Admin UI: `https://my.kleer.se`
- API docs: `https://api-doc.kleer.se/`
- Postman metadata: `https://api-doc.kleer.se/view/metadata/2s9YywgL6m`
- Postman collection: `https://api-doc.kleer.se/api/collections/11922723/2s9YywgL6m?environment=11922723-d718b016-0e8b-4900-85df-bbba28471ce1&segregateAuth=true&versionTag=latest`

## Authentication And Config

- Toki uses one server-side Kleer integration token. Regular users must never enter or store Kleer API tokens in Toki.
- Required backend settings:
  - `kleer.token` / `TOKI_KLEER__TOKEN`
  - `kleer.company_id` / `TOKI_KLEER__COMPANY_ID`
  - `kleer.base_url` / `TOKI_KLEER__BASE_URL` optional, defaults to production
- Use `TOKI_KLEER__BASE_URL=https://api.kleer.se/v1` when validating against the Kleer admin UI at `my.kleer.se`, including sandbox/test companies visible there.
- Do not use `https://test-api.kleer.se/v1` when expecting entries to show up in `my.kleer.se`; live validation on 2026-04-26 showed events written to `test-api` were readable from `test-api` but absent from the real API and admin UI for the same company/user ids.
- Kleer auth is per request through the `X-token` header.
- Send both `Accept: application/json` and `Content-Type: application/json`, including on GET requests. Kleer has returned XML from JSON endpoints when only `Accept` was sent.
- Live validation on 2026-04-17 showed the current Spinit test integration user belongs to company number `4875`, not `1`. Treat this as environment-specific configuration, not a hardcoded default.

## User Mapping

- Every Toki user must be mapped locally to a Kleer user before using time tracking.
- Normal time-tracking requests resolve the Kleer user id from:
  - authenticated Toki user
  - active local mapping
  - configured Kleer company id
  - server-side Kleer service account credentials
- Normal endpoints must never accept Kleer user ids from frontend request bodies or query strings.
- Missing mappings should return a clear not-connected response telling the user to contact an admin.
- Local storage:
  - `time_tracking_provider_users`: imported Kleer directory users
  - `time_tracking_user_links`: active Toki-user to Kleer-user mappings
- Partial unique indexes enforce one active mapping per `(user_id, provider)` and one active mapped local user per `(provider, provider_company_id, provider_user_id)`.
- Admin-only endpoints live under `/time-tracking/admin`:
  - `POST /kleer-users/import`
  - `POST /kleer-users/link-by-email`
  - `GET /kleer-users`
  - `PUT /user-links`
  - `DELETE /user-links/{userId}`
- Email matching only links active, imported Kleer users when the normalized email has exactly one unmapped Toki user and exactly one unmapped Kleer user. It does not overwrite existing manual mappings.

## Kleer Endpoints Used

- `GET /company/{companyId}/user/me`
- `GET /company/{companyId}/user`
- `GET /company/{companyId}/user/{userId}`
- `GET /company/{companyId}/user/foreign-id/{foreignId}`
- `GET /company/{companyId}/client-project`
- `GET /company/{companyId}/client-project?filter=active`
- `GET /company/{companyId}/activity`
- `GET /company/{companyId}/event`
- `GET /company/{companyId}/event/{eventId}`
- `PUT /company/{companyId}/event`
- `POST /company/{companyId}/event/{eventId}`
- `DELETE /company/{companyId}/event/{eventId}`
- `GET /company/{companyId}/event/statuses`
- `GET /company/{companyId}/payroll/user/{userId}/event/from/{fromDate}/to/{toDate}`
- `GET /company/{companyId}/payroll/user/{userId}/schedule/{startDate}/to/{endDate}`

## Events, Statuses, And Stats

- Toki-created Kleer events should always send a generated `foreign-id`. Live validation on 2026-04-21 showed `PUT /event` can return a generic `Tekniskt fel` 500 when `foreign-id` is omitted, even though the XSD marks it optional.
- Kleer statuses map directly to Toki `TimeEntryStatus`:
  - `Open` -> `open`
  - `Approved` -> `approved`
  - `Certified` -> `certified`
- Only `open` entries are editable. `approved` and `certified` entries are locked.
- `GET /event/statuses` exposes date-level statuses even when no event from the current Toki view is present. Use it to gate create/edit attempts before calling `PUT /event`.
  - Live validation on 2026-05-09: Pontus Backman (`131486`) on `2026-05-01` returned `APPROVED`; Martin Liljeberg (`129583`) returned `OPEN` after manually reopening `2026-05-09`.
- Weekly stats expose:
  - `workedHours`
  - `scheduledHours`
  - `remainingHours`
- Weekly stats also expose estimated Kleer period flex fields:
  - `absenceHours`
  - `coveredHours`
  - `periodFlexHours`
- Kleer support confirmed on 2026-04-27 that flex balance is not directly available through the API. Toki estimates period flex as `coveredHours - scheduledHours`, where `coveredHours = workedHours + absenceHours`.
- `periodFlexHours` is a selected-period estimate, not Kleer's stored historical flex balance and not Milltime's previous `FlexTimeCurrent` equivalent.
- Absence hours come from payroll events. Count leave/absence payroll event types as schedule-covering hours, but do not count `WorkHour` as absence to avoid double-counting normal project time.
- Weekly scheduled hours come from the payroll schedule endpoint. Use `actual-hours` from `payroll-user-schedule-metadatas`; it accounts for employment rate and bank holidays.

## Project And Activity Rules

- Kleer projects can restrict activities globally at the project level and per user.
- Activity pickers should respect project restrictions for the mapped Kleer user.
- Live validation on 2026-05-12 showed project-level `activities` can be empty while
  user-level `users[].activities` contains the allowed activities for that user. Example:
  `XYZ` had `all-activities: false`, `activities: []`,
  and Johnny Clayton (`123456`) had user activity `12345`
  (`Maintenance/Operations/Support – time bank`). Treat a non-empty user activity list as
  allowed when the project activity list is empty; intersect only when both lists are non-empty.
- Kleer JSON payloads use kebab-case field names and plain `YYYY-MM-DD` dates. Prefer explicit serde helpers over implicit date serialization.

## Main Repo Touchpoints

- Raw client: `kleer/src/client.rs`
- Kleer types: `kleer/src/types.rs`
- Backend adapter: `toki-api/src/adapters/outbound/kleer/`
- Service factory: `toki-api/src/factory.rs`
- Config: `toki-api/src/config.rs`, `toki-api/config/base.yaml`
- Mapping repository: `toki-api/src/repositories/time_tracking_user_link_repo.rs`
- Mapping migration: `toki-api/migrations/20260421120000_add_time_tracking_user_links.sql`
- Admin and connection routes: `toki-api/src/routes/time_tracking/admin.rs`, `toki-api/src/routes/time_tracking/connection.rs`
- Frontend API types: `app/src/lib/api/queries/time-tracking.ts`, `app/src/lib/api/mutations/time-tracking.ts`
- Mapping UI: `app/src/routes/_layout/time-tracking/-components/time-tracking-settings.tsx`
