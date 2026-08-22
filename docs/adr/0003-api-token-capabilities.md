# API tokens persist a closed capability set

Existing tokens migrate to `timer:read` so widening the Automation API cannot silently raise their authority. Capabilities belong to the token grant, not the user principal: session callers keep full access, while a bearer token may use only the agent operations it was issued for. Capabilities constrain catalog operations rather than acting as a general route allowlist; undocumented routes remain available to authenticated tokens. Issuance rejects empty or unknown values; PostgreSQL CHECKs mirror that invariant.
