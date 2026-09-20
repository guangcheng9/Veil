import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "../api";
import { I18nProvider } from "../i18n";
import UpstreamPage from "./Upstream";

vi.mock("../api", () => ({
  api: {
    upstream: vi.fn(),
    clients: vi.fn(),
    status: vi.fn(),
    putUpstream: vi.fn(),
  },
}));

describe("Upstream page", () => {
  beforeEach(() => {
    vi.mocked(api.upstream).mockResolvedValue({
      anthropic_upstream: "https://api.anthropic.com",
      openai_completions_upstream: "https://api.openai.com",
      openai_responses_upstream: "https://api.openai.com",
      upstream_proxy: "",
      revision: 0,
      routes: [],
    });
    vi.mocked(api.clients).mockResolvedValue([]);
    vi.mocked(api.status).mockResolvedValue({ mapping_ttl_days: 90, request_body_limit_mib: 32 } as never);
    vi.mocked(api.putUpstream).mockResolvedValue(undefined);
  });

  it("normalizes an upstream on blur and tells the user what changed", async () => {
    render(<I18nProvider><UpstreamPage /></I18nProvider>);
    await screen.findAllByDisplayValue("https://api.openai.com");
    const input = screen.getAllByDisplayValue("https://api.openai.com")[0];
    fireEvent.change(input, { target: { value: "  https://relay.example/v1/  " } });
    fireEvent.blur(input);

    await waitFor(() => expect(input).toHaveValue("https://relay.example"));
    expect(screen.getByText(/normalized to https:\/\/relay\.example/i)).toBeInTheDocument();
  });
});
