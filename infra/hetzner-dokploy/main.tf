locals {
  firewall_name = "${var.vm_name}-fw"
}

data "hcloud_ssh_key" "deploy" {
  name = var.ssh_key_name
}

resource "hcloud_firewall" "dokploy" {
  name = local.firewall_name

  rule {
    direction  = "in"
    protocol   = "tcp"
    port       = "22"
    source_ips = var.allowed_ssh_cidrs
  }

  rule {
    direction  = "in"
    protocol   = "tcp"
    port       = "80"
    source_ips = ["0.0.0.0/0", "::/0"]
  }

  rule {
    direction  = "in"
    protocol   = "tcp"
    port       = "443"
    source_ips = ["0.0.0.0/0", "::/0"]
  }

  rule {
    direction  = "in"
    protocol   = "udp"
    port       = "41641"
    source_ips = ["0.0.0.0/0", "::/0"]
  }
}

resource "hcloud_server" "dokploy" {
  name        = var.vm_name
  image       = var.image
  server_type = var.server_type
  location    = var.location

  ssh_keys     = [data.hcloud_ssh_key.deploy.id]
  firewall_ids = [hcloud_firewall.dokploy.id]

  user_data = templatefile("${path.module}/cloud-init.yaml.tftpl", {
    ssh_public_key       = data.hcloud_ssh_key.deploy.public_key
    tailscale_auth_key   = var.tailscale_auth_key
    hostname             = var.vm_name
    enable_tailscale_ssh = var.enable_tailscale_ssh
  })

  public_net {
    ipv4_enabled = true
    ipv6_enabled = true
  }
}
