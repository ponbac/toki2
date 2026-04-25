variable "hcloud_token" {
  description = "Hetzner Cloud API token."
  type        = string
  sensitive   = true
}

variable "ssh_key_name" {
  description = "Name of the existing SSH key in Hetzner Cloud."
  type        = string
}

variable "tailscale_auth_key" {
  description = "One-time or short-lived Tailscale auth key used by cloud-init to join the VM to the tailnet."
  type        = string
  sensitive   = true
}

variable "server_type" {
  description = "Hetzner server type for the Dokploy VM."
  type        = string
  default     = "cx33"
}

variable "location" {
  description = "Hetzner location for the Dokploy VM."
  type        = string
  default     = "hel1"
}

variable "image" {
  description = "Hetzner image name."
  type        = string
  default     = "ubuntu-24.04"
}

variable "vm_name" {
  description = "Name and Tailscale hostname for the Dokploy VM."
  type        = string
  default     = "toki-dokploy-01"
}

variable "allowed_ssh_cidrs" {
  description = "CIDR ranges allowed to reach public SSH. Narrow this after Tailscale SSH is verified."
  type        = list(string)
  default     = ["0.0.0.0/0", "::/0"]
}

variable "enable_tailscale_ssh" {
  description = "Enable Tailscale SSH on first boot."
  type        = bool
  default     = true
}
