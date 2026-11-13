import { render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { api, type LifecycleStatus, type TrafficEvent } from "../api";
import { I18nProvider } from "../i18n";
import DashboardPage from "./Dashboard";

vi.mock("../api", () => ({
  api: {
    traffic: vi.fn(),
    lifecycle: vi.fn(),
    status: vi.fn(),
    startProxy: vi.fn(),
    stopProxy: vi.fn(),
    restartProxy: vi.fn(),
    exitVeil: vi.fn(),
    checkUpdate: vi.fn(),
    applyUpdate: vi.fn(),
  },
  classifyUpdateError: vi.fn(() => "unknown"),
}));

const NOW = Date.now();

function event(i: number, overrides: Partial<TrafficEvent> = {}): TrafficEvent {
  return {
    ts: NOW - (i % 10) * 60_000,
    method: "POST",
    path_template: "/v1/messages",
    status: i % 20 === 0 ? 500 : 200,
    streaming: i % 3 === 0,
    hit_types: i % 4 === 0 ? ["PHONE"] : [],
    hit_counts: i % 4 === 0 ? [2] : [],
    latency_ms: 100 + i,
    error_class: i % 20 === 0 ? "upstream_http" : null,
    creator_prefix8: "abcd1234",
    ...overrides,
  };
}

function lifecycle(): LifecycleStatus {
  return {
    instance_id: "instance",
    pid: 1234,
    started_at: "2026-11-13T00:00:00Z",
    control_plane: "running",
    proxy_state: "running",
    listeners: [18787, 18791],
    last_lifecycle_error: null,
    pending_config_change: false,
  };
}

describe("Dashboard page", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.setItem("veil_lang", "zh");
    vi.mocked(api.lifecycle).mockResolvedValue(lifecycle());
    vi.mocked(api.status).mockResolvedValue({ version: "0.3.17" } as never);
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("labels every cache metric with its scope and unit", async () => {
    vi.mocked(api.traffic).mockResolvedValue(Array.from({ length: 512 }, (_, i) => event(i)));

    render(<I18nProvider><DashboardPage /></I18nProvider>);

    await screen.findByRole("heading", { name: "缓存指标（最近 512 条事件）" });
    expect(screen.getByText("缓存已满，旧事件会滚动淘汰")).toBeInTheDocument();
    expect(screen.getByText("缓存内请求数").closest(".card")).toHaveTextContent("512条");
    expect(screen.getByText("缓存内脱敏请求数").closest(".card")).toHaveTextContent("128个请求");
    expect(screen.getByText("缓存内错误请求数").closest(".card")).toHaveTextContent("26个请求");
    expect(screen.getByText("缓存内上游首包均值").closest(".card")).toHaveTextContent("ms");
    expect(screen.getByText("缓存内成功响应").closest(".card")).toHaveTextContent("486个响应");
    expect(screen.getByText("缓存内流式响应").closest(".card")).toHaveTextContent("171个响应");
    expect(screen.getByText("缓存内脱敏类型")).toBeInTheDocument();
    expect(screen.getByRole("img", { name: "近 20 分钟请求趋势" })).toBeInTheDocument();
    expect(screen.queryByText("平均延迟")).not.toBeInTheDocument();
    expect(screen.queryByText("打过码")).not.toBeInTheDocument();
  });

  it("uses the same explicit scopes in English", async () => {
    localStorage.setItem("veil_lang", "en");
    vi.mocked(api.traffic).mockResolvedValue([event(0)]);

    render(<I18nProvider><DashboardPage /></I18nProvider>);

    expect(await screen.findByRole("heading", { name: "Cached metrics (latest 512 events)" })).toBeInTheDocument();
    expect(screen.getByText("Cached requests")).toBeInTheDocument();
    expect(screen.getByText("Cached upstream TTFB average")).toBeInTheDocument();
    expect(screen.getByRole("img", { name: "Request trend, last 20 minutes" })).toBeInTheDocument();
    expect(screen.getByText("Data scope")).toBeInTheDocument();
  });
  it("shows an explicit loading state before the first traffic response", async () => {
    vi.mocked(api.traffic).mockReturnValue(new Promise<TrafficEvent[]>(() => undefined));

    render(<I18nProvider><DashboardPage /></I18nProvider>);

    expect(await screen.findByText("正在加载流量数据…")).toBeInTheDocument();
    expect(screen.getByText("缓存内请求数").closest(".card")).toHaveTextContent("—条");
  });
  it("shows cache usage and warns when the cache covers less than the chart window", async () => {
    vi.mocked(api.traffic).mockResolvedValue(Array.from({ length: 320 }, (_, i) => event(i)));

    render(<I18nProvider><DashboardPage /></I18nProvider>);

    expect(await screen.findByText("缓存 320 / 512 条事件")).toBeInTheDocument();
    expect(await screen.findByText(/当前缓存仅覆盖最近 \d+ 分钟/)).toBeInTheDocument();
  });

  it("recovers on the next poll after a transient traffic failure", async () => {
    vi.mocked(api.traffic)
      .mockRejectedValueOnce(new Error("temporary failure"))
      .mockResolvedValue([event(0)]);

    render(<I18nProvider><DashboardPage /></I18nProvider>);

    expect(await screen.findByRole("alert")).toHaveTextContent("temporary failure");
    await waitFor(() => expect(screen.queryByRole("alert")).not.toBeInTheDocument(), { timeout: 3_500 });
    expect(screen.getByText("缓存内请求数").closest(".card")).toHaveTextContent("1条");
  });
  it("keeps the dashboard visible when traffic loading fails", async () => {
    vi.mocked(api.traffic).mockRejectedValueOnce(new Error("backend unavailable"));

    render(<I18nProvider><DashboardPage /></I18nProvider>);

    expect(await screen.findByRole("alert")).toHaveTextContent("流量数据暂不可用：backend unavailable");
    expect(screen.getByRole("heading", { name: "看板" })).toBeInTheDocument();
    expect(screen.queryByText(/还没有缓存流量/)).not.toBeInTheDocument();
  });
});
