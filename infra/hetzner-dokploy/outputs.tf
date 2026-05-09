output "server_name" {
  description = "Hetzner server name."
  value       = hcloud_server.dokploy.name
}

output "server_id" {
  description = "Hetzner server ID."
  value       = hcloud_server.dokploy.id
}

output "ipv4_address" {
  description = "Public IPv4 address for DNS A records."
  value       = hcloud_server.dokploy.ipv4_address
}

output "ipv6_address" {
  description = "Public IPv6 address for DNS AAAA records."
  value       = hcloud_server.dokploy.ipv6_address
}

output "dokploy_tailscale_url" {
  description = "Dokploy URL once you resolve the VM Tailscale IP from the Tailscale admin console or `tailscale status`."
  value       = "http://<tailscale-ip>:3000"
}
