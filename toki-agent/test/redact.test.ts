import { describe, expect, it } from "vitest";
import { redactLog } from "../src/security/redact";

describe("log redaction", () => {
  it("redacts common token shapes", () => {
    const redacted = redactLog(
      "Authorization: Bearer abc123\nAuthorization: Basic abc456\nurl=https://user:pat-token@example.invalid/repo.git token=secret",
    );

    expect(redacted).not.toContain("abc123");
    expect(redacted).not.toContain("abc456");
    expect(redacted).not.toContain("pat-token");
    expect(redacted).not.toContain("secret");
    expect(redacted).toContain("[REDACTED]");
  });
});
