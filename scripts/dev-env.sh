# Shared local development environment for just recipes.
#
# This file is meant to be sourced, not executed:
#   source scripts/dev-env.sh
#
# Explicit environment variables set by the caller take precedence over the
# defaults below.

set -a
[[ -f .env ]] && source .env
[[ -f toki-api/.env.local ]] && source toki-api/.env.local
set +a

if [[ -n "${OPENAI_API_KEY:-}" && "${OPENCODE_ALLOW_OPENAI_API_KEY:-}" != "1" ]]; then
    unset OPENAI_API_KEY
    echo "warning: OPENAI_API_KEY was unset for local Toki dev; OpenCode must use auth.json unless OPENCODE_ALLOW_OPENAI_API_KEY=1" >&2
fi

TOKI_AGENT__BASE_URL="${TOKI_AGENT__BASE_URL:-https://toki-agent.pontus98.workers.dev}"
export TOKI_AGENT__BASE_URL

if [[ -z "${TOKI_AGENT__INTERNAL_TOKEN:-}" ]]; then
    toki_agent_token_file="toki-agent/.alchemy/toki-agent-internal-token"
    if [[ -f "$toki_agent_token_file" ]]; then
        TOKI_AGENT__INTERNAL_TOKEN="$(cat "$toki_agent_token_file")"
        export TOKI_AGENT__INTERNAL_TOKEN
    else
        echo "warning: $toki_agent_token_file is missing; real agent routes will not authenticate" >&2
    fi
fi
