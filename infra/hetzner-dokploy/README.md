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
- Branch: `master`

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
  - Domain: `toki-api.spinit.se`
  - Enable HTTPS/TLS with Let's Encrypt.

Create the frontend app:

- `toki-web`
  - Build type: Dockerfile
  - Dockerfile path: `app/Dockerfile`
  - Build context/path: repository root
  - Internal port: `80`
  - Domain: `toki.spinit.se`
  - Enable HTTPS/TLS with Let's Encrypt.

Finally, configure Dokploy database backups.

## Aspire Observability

Deploy the standalone Aspire Dashboard as a normal Dokploy service. Prefer a
single-container **Application** using the Docker provider because this keeps
deployments, logs, monitoring, restarts, and shared environment variables inside
Dokploy.

Create the service from **Create Service -> Application**:

- Name: `aspire-dashboard`
- App name: `toki-aspire-dashboard`
- Provider: Docker
- Image: `mcr.microsoft.com/dotnet/aspire-dashboard:latest`
- UI port: publish container port `18888` only for Tailscale access, for example `http://<vm-tailscale-ip>:18888`
- OTLP gRPC port: keep container port `18889` private on the internal Dokploy/Docker network.
- Do not attach a public domain to the Aspire service.

Use the stable app name `toki-aspire-dashboard` so Docker DNS resolves
`http://toki-aspire-dashboard:18889` from the API container.

Add shared values under the Dokploy production **Project Environment**:

```bash
ASPIRE_OTLP_API_KEY=<shared-long-random-key>
ASPIRE_OTLP_ENDPOINT=http://toki-aspire-dashboard:18889
```

Aspire service environment:

```bash
DASHBOARD__OTLP__AUTHMODE=ApiKey
DASHBOARD__OTLP__PRIMARYAPIKEY=${{environment.ASPIRE_OTLP_API_KEY}}
DASHBOARD__TELEMETRYLIMITS__MAXLOGCOUNT=50000
DASHBOARD__TELEMETRYLIMITS__MAXTRACECOUNT=50000
DASHBOARD__TELEMETRYLIMITS__MAXMETRICSCOUNT=50000
DASHBOARD__TELEMETRYLIMITS__MAXATTRIBUTECOUNT=256
DASHBOARD__TELEMETRYLIMITS__MAXATTRIBUTELENGTH=16384
```

The dashboard UI also has a browser login token. Retrieve it from the container logs:

```bash
tailscale ssh root@toki-dokploy-01 'docker ps --format "{{.Names}}" | grep aspire'
tailscale ssh root@toki-dokploy-01 'docker service logs toki-aspire-dashboard 2>&1 | grep -i token'
```

Configure `toki-api` to export directly to Aspire over the internal Dokploy network:

```bash
RUST_LOG=info,toki_api=debug,az_devops=info,kleer=info,tower_http=info,hyper=warn,h2=warn,tonic=warn,opentelemetry=warn
OTEL_SERVICE_NAME=toki-api
OTEL_RESOURCE_ATTRIBUTES=deployment.environment=production
OTEL_EXPORTER_OTLP_PROTOCOL=grpc
OTEL_EXPORTER_OTLP_ENDPOINT=${{environment.ASPIRE_OTLP_ENDPOINT}}
OTEL_EXPORTER_OTLP_HEADERS=x-otlp-api-key=${{environment.ASPIRE_OTLP_API_KEY}}
# Optional. Defaults to the SDK interval unless configured.
# OTEL_METRIC_EXPORT_INTERVAL=5000
TOKI_OBSERVABILITY__CAPTURE_REQUEST_BODIES=true
TOKI_OBSERVABILITY__REQUEST_BODY_MAX_LOGGED_BYTES=16384
TOKI_OBSERVABILITY__REQUEST_BODY_MAX_BUFFERED_BYTES=65536
```

After adding these variables in Dokploy, redeploy or rebuild `toki-api`. A live
Docker service update is useful for immediate verification, but Dokploy will
overwrite live-only changes on the next deployment.

OTEL trace, log, and metric export use the same OTLP endpoint. Export is enabled only when `OTEL_EXPORTER_OTLP_ENDPOINT` is present and `OTEL_SDK_DISABLED` is not `true`. If deploying the API before Aspire is ready, set:

```bash
OTEL_SDK_DISABLED=true
```

Aspire is intentionally in-memory only for this deployment. If VM memory pressure is visible, lower `DASHBOARD__TELEMETRYLIMITS__MAXLOGCOUNT`, `DASHBOARD__TELEMETRYLIMITS__MAXTRACECOUNT`, or `TOKI_OBSERVABILITY__REQUEST_BODY_MAX_BUFFERED_BYTES`.

If creating Aspire as a Dokploy Docker Compose/Stack service instead, use a
prebuilt `image:`. Docker Stack mode does not support `build:`. No Traefik labels
or Dokploy domain are needed for Aspire because the UI is Tailscale-only.

Avoid managing Aspire only with `docker service create` except for temporary
debugging. A service created outside Dokploy will not appear in the project, will
not use Dokploy shared variables, and can drift from the documented application
state.

Observability smoke checks:

```bash
curl -I http://<vm-tailscale-ip>:18888/
curl -I https://toki-api.spinit.se/
tailscale ssh root@toki-dokploy-01 'docker service ps toki-aspire-dashboard'
tailscale ssh root@toki-dokploy-01 'docker service ps toki-api-8gdssr'
tailscale ssh root@toki-dokploy-01 'docker service logs --tail 100 toki-api-8gdssr'
```

The API root returning `401` is normal. In Aspire, confirm the `toki-api`
resource appears under **Structured logs** and that **Traces** includes SQL,
HTTP, or `repo_differ.poll` spans.

## App Environment

Frontend build-time arguments:

```bash
VITE_API_URL=https://toki-api.spinit.se
VITE_TIME_TRACKING_PROVIDER_URL=<Kleer test or production web URL>
```

Set these on the web application under **Environment -> Build-time Arguments**.
The runtime **Environment Settings** are available to the nginx container after
the Vite bundle has already been built, so they do not change `import.meta.env`.

Backend production environment:

```bash
APP_ENVIRONMENT=production
TOKI_APPLICATION__APP_URL=https://toki.spinit.se
TOKI_APPLICATION__API_URL=https://toki-api.spinit.se
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
TOKI_AUTH__REDIRECT_URL=https://toki-api.spinit.se/oauth/callback
TOKI_KLEER__TOKEN=<Kleer service token>
TOKI_KLEER__COMPANY_ID=<Kleer company id>
TOKI_KLEER__BASE_URL=https://api.kleer.se/v1
TOKI_OBSERVABILITY__CAPTURE_REQUEST_BODIES=true
TOKI_OBSERVABILITY__REQUEST_BODY_MAX_LOGGED_BYTES=16384
TOKI_OBSERVABILITY__REQUEST_BODY_MAX_BUFFERED_BYTES=65536
```

## DNS Cutover

Before cutover, lower TTL for:

- `toki.spinit.se`
- `toki-api.spinit.se`

Point A and AAAA records to the `ipv4_address` and `ipv6_address` outputs, then confirm Dokploy issues certificates and smoke-test login, API calls, Kleer time tracking, PR polling, and web push.

When using Cloudflare, keep DNS simple while certificates are issued:

- `toki.spinit.se` -> Hetzner IPv4/IPv6
- `toki-api.spinit.se` -> Hetzner IPv4/IPv6
- If Let's Encrypt issuance fails behind the Cloudflare proxy, temporarily switch the records to DNS-only until Dokploy has issued certificates.

## Smoke Checks

```bash
curl -I https://toki.spinit.se/prs
curl -I https://toki-api.spinit.se/
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
