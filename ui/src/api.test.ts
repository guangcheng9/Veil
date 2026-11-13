import { afterEach, describe, expect, it, vi } from "vitest";
import { api, type TrafficEvent } from "./api";

function response(body: string): Response {
  return {
    ok: true,
    status: 200,
    headers: new Headers({ "content-type": "text/event-stream" }),
    text: async () => body,
    json: async () => JSON.parse(body),
  } as Response;
}

describe("traffic API", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("parses a valid SSE snapshot and forwards the abort signal", async () => {
    const event: TrafficEvent = {
      ts: 1,
      method: "POST",
      path_template: "/v1/messages",
      status: 200,
      streaming: false,
      hit_types: [],
      hit_counts: [],
      latency_ms: 10,
      error_class: null,
      creator_prefix8: "",
    };
    const fetchMock = vi.fn().mockResolvedValue(response(`event: snapshot\ndata: ${JSON.stringify([event])}\n\n`));
    vi.stubGlobal("fetch", fetchMock);
    const controller = new AbortController();

    await expect(api.traffic(controller.signal)).resolves.toEqual([event]);
    expect(fetchMock).toHaveBeenCalledWith("/api/traffic", expect.objectContaining({ signal: controller.signal }));
  });

  it("rejects a response without a data line", async () => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(response("event: snapshot\n\n")));
    await expect(api.traffic()).rejects.toThrow("missing data line");
  });

  it("rejects a malformed data line", async () => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(response("data: not-json\n\n")));
    await expect(api.traffic()).rejects.toThrow("malformed JSON");
  });
});
