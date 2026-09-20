import { describe, expect, it } from "vitest";
import { normalizeUpstreamBase } from "./upstream";

describe("normalizeUpstreamBase", () => {
  it("trims whitespace, trailing slashes, and a terminal v1 path", () => {
    expect(normalizeUpstreamBase("  https://relay.example/api/V1///  ")).toBe(
      "https://relay.example/api",
    );
  });

  it("removes repeated v1 suffixes without changing similar path names", () => {
    expect(normalizeUpstreamBase("https://relay.example/v1/v1")).toBe(
      "https://relay.example",
    );
    expect(normalizeUpstreamBase("https://relay.example/v1beta/")).toBe(
      "https://relay.example/v1beta",
    );
  });
});
