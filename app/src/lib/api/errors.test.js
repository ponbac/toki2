import { describe, expect, test } from "bun:test";
import ky, { HTTPError } from "ky";
import { apiErrorMessage } from "./errors.ts";

async function requestFailure(body) {
  try {
    await ky.get("https://toki.test/entries", {
      retry: 0,
      fetch: async () => Response.json(body, { status: 400 }),
    });
  } catch (error) {
    expect(error).toBeInstanceOf(HTTPError);
    return error;
  }
  throw new Error("The failing response unexpectedly succeeded");
}

describe("API error messages", () => {
  test("shows a server error after Ky consumes the response body", async () => {
    const error = await requestFailure({ error: "  This entry is locked  " });

    expect(await apiErrorMessage(error, "Could not save entry")).toBe(
      "This entry is locked",
    );
  });

  test.each([
    { body: { error: "  " } },
    { body: { error: 123 } },
    { body: { message: "No error field" } },
    { body: null },
  ])("uses the fallback for an unusable error body: %j", async ({ body }) => {
    const error = await requestFailure(body);

    expect(await apiErrorMessage(error, "Could not save entry")).toBe(
      "Could not save entry",
    );
  });

  test("uses the fallback for a failure without an HTTP response", async () => {
    expect(
      await apiErrorMessage(new TypeError("Failed to fetch"), "Try again"),
    ).toBe("Try again");
  });
});
