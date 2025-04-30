#!/usr/bin/env bash
set -eo pipefail

if [ "$#" -ne 1 ]; then
  echo "Usage: $0 <email>"
  exit 1
fi

EMAIL=$1

# Check if psql is installed
if ! [ -x "$(command -v psql)" ]; then
  echo >&2 "Error: psql is not installed."
  echo >&2 "Install it with: sudo apt install postgresql-client"
  exit 1
fi

# Database connection settings (use defaults if not provided)
DB_USER=${POSTGRES_USER:=postgres}
DB_PASSWORD="${POSTGRES_PASSWORD:=password}"
DB_NAME="${POSTGRES_DB:=toki}"
DB_HOST="${POSTGRES_HOST:=localhost}"
DB_PORT="${POSTGRES_PORT:=5432}"

export PGPASSWORD="${DB_PASSWORD}"

# Update user roles to Admin for the given email
psql -h "${DB_HOST}" -U "${DB_USER}" -d "${DB_NAME}" -p "${DB_PORT}" \
  -c "UPDATE users SET roles = ARRAY['Admin'] WHERE email = '${EMAIL}';"

if [ $? -eq 0 ]; then
  echo "Successfully set roles to Admin for user ${EMAIL}"
else
  echo >&2 "Error updating roles for user ${EMAIL}"
  exit 1
fi
