#!/usr/bin/env bash
set -euo pipefail

SCRIPT_NAME=$(basename "$0")

YES=false
KEEP_DUMP=false
DUMP_ONLY=false
DUMP_PATH=""
USER_PROVIDED_DUMP_PATH=false
TEMP_DIR=""

SSH_TARGET="${DOKPLOY_SSH_TARGET:-root@toki-dokploy-01}"
CONTAINER_FILTER="${DOKPLOY_POSTGRES_CONTAINER_FILTER:-toki-postgres}"
REMOTE_DB="${DOKPLOY_POSTGRES_DB:-}"
REMOTE_USER="${DOKPLOY_POSTGRES_USER:-}"

LOCAL_HOST="${POSTGRES_HOST:-localhost}"
LOCAL_PORT="${POSTGRES_PORT:-5433}"
LOCAL_USER="${POSTGRES_USER:-postgres}"
LOCAL_PASSWORD="${POSTGRES_PASSWORD:-password}"
LOCAL_DB="${POSTGRES_DB:-toki}"

usage() {
    cat <<EOF
Usage: ${SCRIPT_NAME} [options]

Pull a PostgreSQL snapshot from the Dokploy production database over Tailscale SSH
and restore it into the local DB.

Options:
  --yes                           Skip destructive confirmation prompt.
  --keep-dump                     Keep the created dump file.
  --dump-only                     Create a production dump and skip local drop/recreate/restore.
  --dump-path <path>              Path for dump output file.
  --ssh-target <user@host>        Tailscale SSH target (default: root@toki-dokploy-01).
  --container-filter <name>       Docker container name filter (default: toki-postgres).
  --remote-db <name>              Production database name (default: POSTGRES_DB from container).
  --remote-user <user>            Production database user (default: POSTGRES_USER from container).
  --local-host <host>             Local Postgres host (default: localhost).
  --local-port <port>             Local Postgres port (default: 5433).
  --local-user <user>             Local Postgres user (default: postgres).
  --local-password <pass>         Local Postgres password (default: password).
  --local-db <name>               Local Postgres database (default: toki).
  -h, --help                      Show this help text.

Environment overrides:
  DOKPLOY_SSH_TARGET
  DOKPLOY_POSTGRES_CONTAINER_FILTER
  DOKPLOY_POSTGRES_DB
  DOKPLOY_POSTGRES_USER
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
            --dump-only)
                DUMP_ONLY=true
                KEEP_DUMP=true
                shift
                ;;
            --dump-path)
                [[ $# -ge 2 ]] || fail "--dump-path requires a value"
                DUMP_PATH="$2"
                USER_PROVIDED_DUMP_PATH=true
                shift 2
                ;;
            --ssh-target)
                [[ $# -ge 2 ]] || fail "--ssh-target requires a value"
                SSH_TARGET="$2"
                shift 2
                ;;
            --container-filter)
                [[ $# -ge 2 ]] || fail "--container-filter requires a value"
                CONTAINER_FILTER="$2"
                shift 2
                ;;
            --remote-db)
                [[ $# -ge 2 ]] || fail "--remote-db requires a value"
                REMOTE_DB="$2"
                shift 2
                ;;
            --remote-user)
                [[ $# -ge 2 ]] || fail "--remote-user requires a value"
                REMOTE_USER="$2"
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
    require_cmd tailscale
    require_cmd mktemp

    if [[ "${DUMP_ONLY}" == "false" ]]; then
        require_cmd psql
        require_cmd pg_restore
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
}

prepare_dump_path() {
    if [[ -z "${DUMP_PATH}" ]]; then
        if [[ "${DUMP_ONLY}" == "true" ]]; then
            DUMP_PATH="./prod.dump"
            USER_PROVIDED_DUMP_PATH=true
        else
            TEMP_DIR="$(mktemp -d)"
            DUMP_PATH="${TEMP_DIR}/prod.dump"
        fi
    fi
}

cleanup() {
    if [[ "${KEEP_DUMP}" == "false" ]] && [[ "${USER_PROVIDED_DUMP_PATH}" == "false" ]] && [[ -n "${DUMP_PATH}" ]] && [[ -f "${DUMP_PATH}" ]]; then
        rm -f "${DUMP_PATH}"
    fi

    if [[ -n "${TEMP_DIR}" ]] && [[ -d "${TEMP_DIR}" ]]; then
        rmdir "${TEMP_DIR}" 2>/dev/null || true
    fi
}

create_production_dump() {
    log "Creating production dump from Dokploy via tailscale ssh ${SSH_TARGET}"
    log "Looking for remote Docker container matching '${CONTAINER_FILTER}'"

    if ! tailscale ssh "${SSH_TARGET}" bash -s -- "${CONTAINER_FILTER}" "${REMOTE_DB}" "${REMOTE_USER}" >"${DUMP_PATH}" <<'REMOTE'
set -euo pipefail

container_filter="${1:-}"
remote_db="${2:-}"
remote_user="${3:-}"

if [[ -z "${container_filter}" ]]; then
    printf '[remote] Error: container filter was empty\n' >&2
    exit 1
fi

container_ref="$(docker ps --filter "name=${container_filter}" --format '{{.ID}} {{.Names}}' | head -n1)"
if [[ -z "${container_ref}" ]]; then
    printf '[remote] Error: no running Docker container matched name filter %q\n' "${container_filter}" >&2
    exit 1
fi

container_id="${container_ref%% *}"
container_name="${container_ref#* }"

if [[ -z "${remote_user}" ]]; then
    remote_user="$(docker exec "${container_id}" printenv POSTGRES_USER 2>/dev/null || true)"
fi

if [[ -z "${remote_db}" ]]; then
    remote_db="$(docker exec "${container_id}" printenv POSTGRES_DB 2>/dev/null || true)"
fi

remote_user="${remote_user:-postgres}"
remote_db="${remote_db:-${remote_user}}"
remote_password="$(docker exec "${container_id}" printenv POSTGRES_PASSWORD 2>/dev/null || true)"

printf '[remote] Dumping container=%s database=%s user=%s\n' "${container_name}" "${remote_db}" "${remote_user}" >&2

if [[ -n "${remote_password}" ]]; then
    docker exec -e PGPASSWORD="${remote_password}" "${container_id}" \
        pg_dump \
            --format=custom \
            --no-owner \
            --no-privileges \
            -U "${remote_user}" \
            -d "${remote_db}"
else
    docker exec "${container_id}" \
        pg_dump \
            --format=custom \
            --no-owner \
            --no-privileges \
            -U "${remote_user}" \
            -d "${remote_db}"
fi
REMOTE
    then
        fail "Failed to create production dump over Tailscale SSH"
    fi

    if [[ ! -s "${DUMP_PATH}" ]]; then
        fail "Production dump is empty: ${DUMP_PATH}"
    fi
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
    if [[ "${DUMP_ONLY}" == "false" ]]; then
        ensure_safe_local_target
    fi
    prepare_dump_path

    create_production_dump

    if [[ "${DUMP_ONLY}" == "true" ]]; then
        log "Done. Production dump saved at ${DUMP_PATH}"
        return
    fi

    confirm_destructive_restore
    recreate_local_database
    restore_dump

    log "Done. Local DB '${LOCAL_DB}' now contains a production snapshot."
    if [[ "${KEEP_DUMP}" == "true" ]] || [[ "${USER_PROVIDED_DUMP_PATH}" == "true" ]]; then
        log "Dump saved at ${DUMP_PATH}"
    fi
}

main "$@"
