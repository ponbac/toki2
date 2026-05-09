# Toki2 Hetzner Dokploy Infrastructure

OpenTofu module for the first Toki2 Dokploy host on Hetzner Cloud.

OpenTofu owns only the VM, public firewall, SSH key attachment, cloud-init, Tailscale bootstrap, and Dokploy installation. Dokploy owns applications, PostgreSQL, routing, TLS, logs, restarts, and database backups.

## Defaults

- Image: `ubuntu-24.04`
- Location: `hel1`
- Server type: `cx33`
- VM name: `toki-dokploy-01`
- Public ports: `22`, `80`, `443`, and UDP `41641`
- Dokploy panel: installed on `:3000`, but not opened in the Hetzner firewall

## Required Inputs

- `hcloud_token` - Hetzner Cloud API token
- `ssh_key_name` - existing Hetzner SSH key name
- `tailscale_auth_key` - one-time or short-lived Tailscale auth key

Create an ignored `secret.auto.tfvars`:

```hcl
hcloud_token       = "..."
ssh_key_name       = "Toki"
tailscale_auth_key = "tskey-auth-..."
```

Or export variables:

```bash
export TF_VAR_hcloud_token="..."
export TF_VAR_ssh_key_name="your-hetzner-key-name"
export TF_VAR_tailscale_auth_key="tskey-auth-..."
```

## Usage

```bash
cd infra/hetzner-dokploy
tofu init
tofu fmt
tofu validate
tofu plan
tofu apply
```

After apply, find the VM Tailscale IP in the Tailscale admin console or with `tailscale status`, then open:

```text
http://<tailscale-ip>:3000
```

## Tailscale Bootstrap

The VM joins Tailscale during first boot through cloud-init:

1. `curl -fsSL https://tailscale.com/install.sh | sh`
2. `tailscale up --auth-key=<tailscale_auth_key> --hostname=toki-dokploy-01`
3. `tailscale set --ssh` when `enable_tailscale_ssh = true`

The auth key is rendered into cloud-init and therefore into local OpenTofu state. Use a one-time or short-lived reusable key, then expire/revoke it in the Tailscale admin console after the VM has joined.

Current intended access model:

- Public app traffic enters on `80` and `443`.
- Public SSH is temporarily allowed by `allowed_ssh_cidrs`.
- Dokploy listens on `:3000`, but public `3000` is blocked by Hetzner firewall and UFW.
- Dokploy is accessed through the VM's Tailscale IP, for example `http://100.x.y.z:3000`.

After Tailscale SSH is verified, narrow or remove public SSH access by changing `allowed_ssh_cidrs`.

## Dokploy Setup

Create the Dokploy admin account through the Tailscale URL, then create one project, one PostgreSQL service, and two apps.

Use plain Git source if the GitHub integration is unavailable:

- Repository: `https://github.com/ponbac/toki2.git`
- Branch: `kleer`

Create a Dokploy PostgreSQL service:

- Name: `postgres`
- Database: `toki`
- User: `toki`
- Save the generated password for the API environment.
- Deploy/start the service before starting the API.
- Use the generated internal service host, for example `toki-postgres-u6q5wr`.

Create the backend app:

- `toki-api`
  - Build type: Dockerfile
  - Dockerfile path: `Dockerfile`
  - Build context/path: repository root
  - Internal port: `8080`
  - Domain: `toki-api.bkmn.xyz`
  - Enable HTTPS/TLS with Let's Encrypt.

Create the frontend app:

- `toki-web`
  - Build type: Dockerfile
  - Dockerfile path: `app/Dockerfile`
  - Build context/path: repository root
  - Internal port: `80`
  - Domain: `toki.bkmn.xyz`
  - Enable HTTPS/TLS with Let's Encrypt.

Finally, configure Dokploy database backups.

## App Environment

Frontend build environment:

```bash
VITE_API_URL=https://toki-api.bkmn.xyz
VITE_TIME_TRACKING_PROVIDER_URL=<Kleer test or production web URL>
```

Backend production environment:

```bash
APP_ENVIRONMENT=production
TOKI_APPLICATION__APP_URL=https://toki.bkmn.xyz
TOKI_APPLICATION__API_URL=https://toki-api.bkmn.xyz
TOKI_APPLICATION__HOST=0.0.0.0
TOKI_APPLICATION__PORT=8080
TOKI_DATABASE__HOST=<Dokploy Postgres service host>
TOKI_DATABASE__PORT=5432
TOKI_DATABASE__USERNAME=<Dokploy Postgres user>
TOKI_DATABASE__PASSWORD=<Dokploy Postgres password>
TOKI_DATABASE__DATABASE_NAME=<Dokploy Postgres database>
TOKI_DATABASE__REQUIRE_SSL=false
DATABASE_URL=postgres://<user>:<password>@<Dokploy Postgres service host>:5432/<database>
TOKI_AUTH__CLIENT_ID=<Azure AD app client id>
TOKI_AUTH__CLIENT_SECRET=<Azure AD app secret>
TOKI_AUTH__REDIRECT_URL=https://toki-api.bkmn.xyz/oauth/callback
TOKI_KLEER__TOKEN=<Kleer service token>
TOKI_KLEER__COMPANY_ID=<Kleer company id>
TOKI_KLEER__BASE_URL=https://api.kleer.se/v1
```

## DNS Cutover

Before cutover, lower TTL for:

- `toki.bkmn.xyz`
- `toki-api.bkmn.xyz`

Point A and AAAA records to the `ipv4_address` and `ipv6_address` outputs, then confirm Dokploy issues certificates and smoke-test login, API calls, Kleer time tracking, PR polling, and web push.

When using Cloudflare, keep DNS simple while certificates are issued:

- `toki.bkmn.xyz` -> Hetzner IPv4/IPv6
- `toki-api.bkmn.xyz` -> Hetzner IPv4/IPv6
- If Let's Encrypt issuance fails behind the Cloudflare proxy, temporarily switch the records to DNS-only until Dokploy has issued certificates.

## Smoke Checks

```bash
curl -I https://toki.bkmn.xyz/prs
curl -I https://toki-api.bkmn.xyz/
tailscale ssh root@toki-dokploy-01 'docker service ls'
```

The API root returning `401` is normal because it is authenticated.

## Security Notes

- `terraform.tfstate`, `*.tfvars`, `.env`, plans, and crash logs are ignored here.
- `tailscale_auth_key` is still present in local OpenTofu state because it is rendered into cloud-init. Use a one-time or short-lived key and store state securely.
- Narrow `allowed_ssh_cidrs` after Tailscale SSH is confirmed.
- Dokploy publishes its panel on `3000`; the Hetzner firewall deliberately does not allow public TCP `3000`, because Docker-published ports can bypass UFW.
- Application secrets should be stored in Dokploy for this first version, not in OpenTofu.
- The frontend uses `app/Dockerfile`, which builds with Bun and serves the built Vite app with `nginx:alpine`. Dokploy still owns TLS and routing.

## Debugging

```bash
ssh deploy@<server-ip>
sudo tail -f /var/log/cloud-init-output.log
```

Useful VM checks:

```bash
tailscale ssh root@toki-dokploy-01 'docker service ls'
tailscale ssh root@toki-dokploy-01 'docker service logs --tail 100 toki-api-8gdssr'
tailscale ssh root@toki-dokploy-01 'ufw status verbose'
tailscale ssh root@toki-dokploy-01 'tailscale status'
```
