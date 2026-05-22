import { Sandbox as CloudflareSandbox } from "@cloudflare/sandbox";

export class SandboxV2<Env = unknown> extends CloudflareSandbox<Env> {
  override sleepAfter = "20m";
}
