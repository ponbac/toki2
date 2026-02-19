#!/usr/bin/env bash
set -euo pipefail

SCRIPT_NAME=$(basename "$0")

YES=false
KEEP_DUMP=false
DUMP_PATH=""
USER_PROVIDED_DUMP_PATH=false
TEMP_DIR=""
PROXY_PID=""
PROXY_LOG_FILE=""

FLY_APP="${FLY_APP:-toki2}"
FLY_DB_APP="${FLY_DB_APP:-}"
PROXY_PORT="${PROXY_PORT:-15432}"
LOCAL_HOST="${POSTGRES_HOST:-localhost}"
LOCAL_PORT="${POSTGRES_PORT:-5433}"
LOCAL_USER="${POSTGRES_USER:-postgres}"
LOCAL_PASSWORD="${POSTGRES_PASSWORD:-password}"
LOCAL_DB="${POSTGRES_DB:-toki}"

PROD_HOST=""
PROD_PORT=""
PROD_USER=""
PROD_PASSWORD=""
PROD_DB=""

usage() {
    cat <<EOF
Usage: ${SCRIPT_NAME} [options]

Pull a PostgreSQL snapshot from Fly production and restore it into local DB.

Options:
  --yes                      Skip destructive confirmation prompt.
  --keep-dump                Keep the created dump file.
  --dump-path <path>         Path for dump output file.
  --fly-app <name>           Fly app to read DB env from (default: toki2).
  --fly-db-app <name>        Fly Postgres app for proxy (default: derived from *.flycast host).
  --proxy-port <port>        Local port for Fly DB proxy (default: 15432).
  --local-host <host>        Local Postgres host (default: localhost).
  --local-port <port>        Local Postgres port (default: 5433).
  --local-user <user>        Local Postgres user (default: postgres).
  --local-password <pass>    Local Postgres password (default: password).
  --local-db <name>          Local Postgres database (default: toki).
  -h, --help                 Show this help text.
EOF
}

log() {
    printf '[%s] %s\n' "${SCRIPT_NAME}" "$*"
}

fail() {
    printf '[%s] Error: %s\n' "${SCRIPT_NAME}" "$*" >&2
    exit 1
}

require_cmd() {
    local cmd="$1"
    if ! command -v "${cmd}" >/dev/null 2>&1; then
        fail "Required command not found: ${cmd}"
    fi
}

parse_args() {
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --yes)
                YES=true
                shift
                ;;
            --keep-dump)
                KEEP_DUMP=true
                shift
                ;;
            --dump-path)
                [[ $# -ge 2 ]] || fail "--dump-path requires a value"
                DUMP_PATH="$2"
                USER_PROVIDED_DUMP_PATH=true
                shift 2
                ;;
            --fly-app)
                [[ $# -ge 2 ]] || fail "--fly-app requires a value"
                FLY_APP="$2"
                shift 2
                ;;
            --fly-db-app)
                [[ $# -ge 2 ]] || fail "--fly-db-app requires a value"
                FLY_DB_APP="$2"
                shift 2
                ;;
            --proxy-port)
                [[ $# -ge 2 ]] || fail "--proxy-port requires a value"
                PROXY_PORT="$2"
                shift 2
                ;;
            --local-host)
                [[ $# -ge 2 ]] || fail "--local-host requires a value"
                LOCAL_HOST="$2"
                shift 2
                ;;
            --local-port)
                [[ $# -ge 2 ]] || fail "--local-port requires a value"
                LOCAL_PORT="$2"
                shift 2
                ;;
            --local-user)
                [[ $# -ge 2 ]] || fail "--local-user requires a value"
                LOCAL_USER="$2"
                shift 2
                ;;
            --local-password)
                [[ $# -ge 2 ]] || fail "--local-password requires a value"
                LOCAL_PASSWORD="$2"
                shift 2
                ;;
            --local-db)
                [[ $# -ge 2 ]] || fail "--local-db requires a value"
                LOCAL_DB="$2"
                shift 2
                ;;
            -h|--help)
                usage
                exit 0
                ;;
            *)
                fail "Unknown argument: $1. Use --help for usage."
                ;;
        esac
    done
}

ensure_prerequisites() {
    require_cmd flyctl
    require_cmd pg_dump
    require_cmd pg_restore
    require_cmd psql
    require_cmd mktemp
}

ensure_fly_auth() {
    if ! flyctl auth whoami >/dev/null 2>&1; then
        fail "Not authenticated with Fly. Run: flyctl auth login"
    fi
}

ensure_safe_local_target() {
    case "${LOCAL_HOST}" in
        localhost|127.0.0.1|::1)
            ;;
        *)
            fail "Refusing restore: local host must be localhost/127.0.0.1/::1 (got '${LOCAL_HOST}')"
            ;;
    esac

    if [[ "${LOCAL_HOST}" == *".flycast" ]] || [[ "${LOCAL_HOST}" == *"fly.dev"* ]]; then
        fail "Refusing restore: local host looks like Fly infrastructure (${LOCAL_HOST})"
    fi
}

prepare_dump_path() {
    if [[ -z "${DUMP_PATH}" ]]; then
        TEMP_DIR="$(mktemp -d)"
        DUMP_PATH="${TEMP_DIR}/prod.dump"
    fi
}

cleanup() {
    if [[ -n "${PROXY_PID}" ]]; then
        kill "${PROXY_PID}" >/dev/null 2>&1 || true
    fi

    if [[ "${KEEP_DUMP}" == "false" ]] && [[ "${USER_PROVIDED_DUMP_PATH}" == "false" ]] && [[ -n "${DUMP_PATH}" ]] && [[ -f "${DUMP_PATH}" ]]; then
        rm -f "${DUMP_PATH}"
    fi

    if [[ -n "${PROXY_LOG_FILE}" ]] && [[ -f "${PROXY_LOG_FILE}" ]]; then
        rm -f "${PROXY_LOG_FILE}"
    fi

    if [[ -n "${TEMP_DIR}" ]] && [[ -d "${TEMP_DIR}" ]]; then
        rmdir "${TEMP_DIR}" 2>/dev/null || true
    fi
}

read_fly_environment() {
    local env_output
    if ! env_output="$(flyctl ssh console --app "${FLY_APP}" --quiet --command "env" 2>/dev/null)"; then
        fail "Failed to read environment from Fly app '${FLY_APP}'. Ensure it has a running machine and you have access."
    fi
    printf '%s' "${env_output}"
}

extract_env_value() {
    local env_blob="$1"
    local key="$2"
    local line
    line="$(printf '%s\n' "${env_blob}" | grep -m1 "^${key}=" || true)"
    if [[ -z "${line}" ]]; then
        fail "Missing '${key}' in Fly app '${FLY_APP}' environment."
    fi
    printf '%s' "${line#*=}"
}

fetch_production_connection() {
    local fly_env
    fly_env="$(read_fly_environment)"

    PROD_HOST="$(extract_env_value "${fly_env}" "TOKI_DATABASE__HOST")"
    PROD_PORT="$(extract_env_value "${fly_env}" "TOKI_DATABASE__PORT")"
    PROD_USER="$(extract_env_value "${fly_env}" "TOKI_DATABASE__USERNAME")"
    PROD_PASSWORD="$(extract_env_value "${fly_env}" "TOKI_DATABASE__PASSWORD")"
    PROD_DB="$(extract_env_value "${fly_env}" "TOKI_DATABASE__DATABASE_NAME")"
}

start_fly_proxy_if_needed() {
    if [[ "${PROD_HOST}" != *.flycast ]]; then
        return
    fi

    if [[ -z "${FLY_DB_APP}" ]]; then
        FLY_DB_APP="${PROD_HOST%%.flycast}"
    fi

    PROXY_LOG_FILE="$(mktemp)"
    log "Starting Fly DB proxy via app '${FLY_DB_APP}' on 127.0.0.1:${PROXY_PORT}"
    flyctl proxy "${PROXY_PORT}:${PROD_PORT}" --app "${FLY_DB_APP}" --bind-addr 127.0.0.1 --quiet >"${PROXY_LOG_FILE}" 2>&1 &
    PROXY_PID=$!

    local attempt
    for attempt in {1..20}; do
        if ! kill -0 "${PROXY_PID}" >/dev/null 2>&1; then
            local proxy_output
            proxy_output="$(cat "${PROXY_LOG_FILE}" 2>/dev/null || true)"
            fail "Fly proxy exited early. ${proxy_output}"
        fi

        if PGPASSWORD="${PROD_PASSWORD}" \
           psql -h 127.0.0.1 -p "${PROXY_PORT}" -U "${PROD_USER}" -d "${PROD_DB}" -c '\q' >/dev/null 2>&1; then
            PROD_HOST="127.0.0.1"
            PROD_PORT="${PROXY_PORT}"
            return
        fi

        sleep 1
    done

    local proxy_output
    proxy_output="$(cat "${PROXY_LOG_FILE}" 2>/dev/null || true)"
    fail "Timed out waiting for Fly DB proxy to become ready. ${proxy_output}"
}

create_production_dump() {
    log "Creating production dump from ${PROD_HOST}:${PROD_PORT}/${PROD_DB}"
    PGHOST="${PROD_HOST}" \
    PGPORT="${PROD_PORT}" \
    PGUSER="${PROD_USER}" \
    PGPASSWORD="${PROD_PASSWORD}" \
    PGDATABASE="${PROD_DB}" \
        pg_dump \
            --format=custom \
            --no-owner \
            --no-privileges \
            --file "${DUMP_PATH}"
}

confirm_destructive_restore() {
    if [[ "${YES}" == "true" ]]; then
        return
    fi

    if [[ ! -t 0 ]]; then
        fail "Confirmation required but no TTY available. Re-run with --yes."
    fi

    log "About to DROP and recreate local DB:"
    log "  host=${LOCAL_HOST} port=${LOCAL_PORT} user=${LOCAL_USER} database=${LOCAL_DB}"
    printf "Type '%s' to continue: " "${LOCAL_DB}"
    local confirmation
    read -r confirmation
    if [[ "${confirmation}" != "${LOCAL_DB}" ]]; then
        fail "Confirmation did not match '${LOCAL_DB}'. Aborting."
    fi
}

recreate_local_database() {
    log "Recreating local database '${LOCAL_DB}'"
    PGPASSWORD="${LOCAL_PASSWORD}" \
    PGHOST="${LOCAL_HOST}" \
    PGPORT="${LOCAL_PORT}" \
    PGUSER="${LOCAL_USER}" \
    psql --dbname "postgres" --set ON_ERROR_STOP=1 --set target_db="${LOCAL_DB}" <<'SQL'
SELECT pg_terminate_backend(pid)
FROM pg_stat_activity
WHERE datname = :'target_db'
  AND pid <> pg_backend_pid();

SELECT format('DROP DATABASE IF EXISTS %I', :'target_db') \gexec
SELECT format('CREATE DATABASE %I', :'target_db') \gexec
SQL
}

restore_dump() {
    log "Restoring dump into local database '${LOCAL_DB}'"
    PGPASSWORD="${LOCAL_PASSWORD}" \
    PGHOST="${LOCAL_HOST}" \
    PGPORT="${LOCAL_PORT}" \
    PGUSER="${LOCAL_USER}" \
        pg_restore \
            --no-owner \
            --no-privileges \
            --clean \
            --if-exists \
            --dbname "${LOCAL_DB}" \
            "${DUMP_PATH}"
}

main() {
    parse_args "$@"
    trap cleanup EXIT

    ensure_prerequisites
    ensure_safe_local_target
    ensure_fly_auth
    prepare_dump_path

    fetch_production_connection
    start_fly_proxy_if_needed
    create_production_dump
    confirm_destructive_restore
    recreate_local_database
    restore_dump

    log "Done. Local DB '${LOCAL_DB}' now contains a production snapshot."
    if [[ "${KEEP_DUMP}" == "true" ]] || [[ "${USER_PROVIDED_DUMP_PATH}" == "true" ]]; then
        log "Dump saved at ${DUMP_PATH}"
    fi
}

main "$@"
