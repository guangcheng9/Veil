import { useEffect, useMemo, useState } from "react";
import { api, classifyUpdateError, type LifecycleStatus, type TrafficEvent, type UpdateErrorKind } from "../api";
import { useT } from "../i18n";

type UpdateInfo = { current: string; latest: string; newer: boolean };

const WINDOW_MIN = 20;
const CACHE_CAP = 512;
const WINDOW_MS = WINDOW_MIN * 60_000;
const REFRESH_MS = 2_000;

function errorText(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

function isAbortError(error: unknown) {
  return typeof error === "object" && error !== null && "name" in error && (error as { name?: string }).name === "AbortError";
}

function formatClock(ts: number) {
  return new Date(ts).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", hour12: false });
}

function formatTime(ts: number) {
  return new Date(ts).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit", hour12: false });
}

function buckets(events: TrafficEvent[], now: number) {
  const arr = Array.from({ length: WINDOW_MIN }, () => 0);
  for (const e of events) {
    const i = Math.floor((now - e.ts) / 60_000);
    if (i >= 0 && i < WINDOW_MIN) arr[WINDOW_MIN - 1 - i] += 1;
  }
  return arr;
}

function hitMix(events: TrafficEvent[]) {
  const map: Record<string, number> = {};
  for (const e of events) {
    e.hit_types.forEach((ty, i) => {
      map[ty] = (map[ty] ?? 0) + (e.hit_counts[i] ?? 1);
    });
  }
  return Object.entries(map).sort((a, b) => b[1] - a[1]);
}

function BarChart({ values, labels, ariaLabel }: { values: number[]; labels: string[]; ariaLabel: string }) {
  const max = Math.max(1, ...values);
  const w = 560;
  const h = 120;
  const gap = 3;
  const bw = (w - gap * (values.length + 1)) / values.length;
  return (
    <svg viewBox={"0 0 " + w + " " + h} className="chart" role="img" aria-label={ariaLabel}>
      <title>{ariaLabel}</title>
      {values.map((v, i) => {
        const bh = (v / max) * (h - 18);
        const x = gap + i * (bw + gap);
        const showLabel = i === 0 || i === labels.length - 1 || i % 5 === 0;
        return (
          <g key={i}>
            <rect x={x} y={h - 16 - bh} width={bw} height={bh} rx="2" fill="currentColor" opacity={v ? 0.9 : 0.15} />
            {showLabel && (
              <text
                x={i === 0 ? x : i === labels.length - 1 ? x + bw : x + bw / 2}
                y={h - 4}
                textAnchor={i === 0 ? "start" : i === labels.length - 1 ? "end" : "middle"}
                fontSize="9"
                fill="currentColor"
                opacity="0.55"
              >
                {labels[i]}
              </text>
            )}
          </g>
        );
      })}
    </svg>
  );
}

function HBars({ rows }: { rows: [string, number][] }) {
  const max = Math.max(1, ...rows.map((r) => r[1]));
  if (!rows.length) return <p className="muted">—</p>;
  return (
    <div className="hbars">
      {rows.map(([name, n]) => (
        <div className="hbar" key={name}>
          <span className="hbar-l">{name}</span>
          <span className="hbar-t">
            <span style={{ width: ((n / max) * 100) + "%" }} />
          </span>
          <span className="hbar-n">{n.toLocaleString()}</span>
        </div>
      ))}
    </div>
  );
}

function MetricCard({
  title,
  value,
  unit,
  scope,
  hint,
}: {
  title: string;
  value: number | string;
  unit: string;
  scope: string;
  hint: string;
}) {
  return (
    <div className="card metric-card">
      <div className="label">{title}</div>
      <div className="value">
        {typeof value === "number" ? value.toLocaleString() : value}
        <span className="unit">{unit}</span>
      </div>
      <div className="metric-scope">{scope}</div>
      <div className="metric-hint">{hint}</div>
    </div>
  );
}

export default function DashboardPage() {
  const { t } = useT();
  const updErrText = (k: UpdateErrorKind) =>
    k === "network" ? t.updErrNetwork
    : k === "rateLimit" ? t.updErrRateLimit
    : k === "noRelease" ? t.updErrNoRelease
    : t.updErrUnknown;
  const [events, setEvents] = useState<TrafficEvent[]>([]);
  const [trafficErr, setTrafficErr] = useState<string | null>(null);
  const [lifecycleErr, setLifecycleErr] = useState<string | null>(null);
  const [now, setNow] = useState(Date.now());
  const [lastUpdated, setLastUpdated] = useState<number | null>(null);
  const [ver, setVer] = useState("");
  const [upd, setUpd] = useState<UpdateInfo | null>(null);
  const [updMsg, setUpdMsg] = useState("");
  const [life, setLife] = useState<LifecycleStatus | null>(null);
  const [lifeMsg, setLifeMsg] = useState("");
  const [lifeBusy, setLifeBusy] = useState(false);

  useEffect(() => {
    let on = true;
    let controller: AbortController | null = null;
    const tick = () => {
      controller?.abort();
      controller = new AbortController();
      const signal = controller.signal;

      api.traffic(signal)
        .then((next) => {
          if (!on) return;
          const receivedAt = Date.now();
          setEvents(next);
          setNow(receivedAt);
          setLastUpdated(receivedAt);
          setTrafficErr(null);
        })
        .catch((error) => {
          if (on && !isAbortError(error)) setTrafficErr(errorText(error));
        });

      api.lifecycle(signal)
        .then((next) => {
          if (!on) return;
          setLife(next);
          setLifecycleErr(null);
        })
        .catch((error) => {
          if (on && !isAbortError(error)) setLifecycleErr(errorText(error));
        });
    };
    tick();
    const id = window.setInterval(tick, REFRESH_MS);
    api.status().then((status) => { if (on) setVer(status.version ?? ""); }).catch(() => undefined);
    return () => {
      on = false;
      controller?.abort();
      window.clearInterval(id);
    };
  }, []);

  const stats = useMemo(() => {
    const n = events.length;
    const hits = events.filter((e) => e.hit_types.length > 0).length;
    const errors = events.filter((e) => e.status >= 400 || e.error_class).length;
    const avg = n ? Math.round(events.reduce((s, e) => s + e.latency_ms, 0) / n) : 0;
    const ok = events.filter((e) => e.status >= 200 && e.status < 300).length;
    const sse = events.filter((e) => e.streaming).length;
    return { n, hits, errors, avg, ok, sse };
  }, [events]);

  const series = useMemo(() => buckets(events, now), [events, now]);
  const mix = useMemo(() => hitMix(events), [events]);
  const labels = series.map((_, i) => formatClock(now - (WINDOW_MIN - 1 - i) * 60_000));
  const cacheFull = events.length >= CACHE_CAP;
  const coverage = useMemo(() => {
    if (!events.length) return { complete: false, minutes: 0 };
    const oldest = events.reduce((min, event) => Math.min(min, event.ts), events[0].ts);
    const minutes = Math.max(0, Math.floor((now - oldest) / 60_000));
    return { complete: oldest <= now - WINDOW_MS, minutes: Math.min(WINDOW_MIN, Math.max(1, minutes)) };
  }, [events, now]);

  const stateText = (state: LifecycleStatus["proxy_state"]) =>
    state === "starting" ? t.stateStarting
    : state === "running" ? t.stateRunning
    : state === "stopping" ? t.stateStopping
    : state === "stopped" ? t.stateStopped
    : t.stateDegraded;
  const controlPlaneText = (state: LifecycleStatus["control_plane"]) =>
    state === "running" ? t.controlRunning : t.controlStopping;

  const act = async (kind: "start" | "stop" | "restart") => {
    setLifeBusy(true);
    setLifeMsg("");
    try {
      const next = kind === "start" ? await api.startProxy() : kind === "stop" ? await api.stopProxy() : await api.restartProxy();
      setLife(next);
      setLifecycleErr(null);
      setLifeMsg(kind === "stop" ? t.proxyStoppedHint : t.proxyRunningHint);
    } catch (error) {
      setLifecycleErr(errorText(error));
    }
    setLifeBusy(false);
  };

  return (
    <section>
      <h1>{t.dashTitle}</h1>
      <p className="help">{t.helpDash}</p>

      <div className="data-scope" aria-busy={lastUpdated === null && !trafficErr}>
        <div className="data-scope-main">
          <strong>{t.dataScope}</strong>
          <span>{t.cacheScopeShort}</span>
          <span>·</span>
          <span>{t.updatedAt(lastUpdated ? formatTime(lastUpdated) : "—")}</span>
          <span>·</span>
          <span>{t.refreshEvery2s}</span>
        </div>
        <span className={"scope-badge" + (cacheFull ? " full" : "")}>
          {cacheFull ? t.cacheFull : lastUpdated ? t.cacheUsage(events.length, CACHE_CAP) : "—"}
        </span>
      </div>

      {!lastUpdated && !trafficErr && <p className="muted loading-line">{t.loadingTraffic}</p>}
      {trafficErr && <p className="inline-error" role="alert">{t.dashboardDataError}{trafficErr}</p>}

      <div className="service-control">
        <div>
          <h2>{t.serviceControl}</h2>
          <p className="help service-status">
            {life
              ? `${t.dataPlane}: ${stateText(life.proxy_state)} · ${t.controlPlane}: ${controlPlaneText(life.control_plane)} · ${t.processId}: ${life.pid} · ${t.dataPlanePorts}: ${life.listeners.join(", ") || t.none}`
              : t.loading}
          </p>
        </div>
        <div className="service-actions">
          <button type="button" disabled={lifeBusy || !life || life.proxy_state === "running" || life.proxy_state === "starting" || life.proxy_state === "stopping"} onClick={() => void act("start")}>{t.startProxy}</button>
          <button type="button" disabled={lifeBusy || !life || life.proxy_state !== "running"} onClick={() => void act("stop")}>{t.stopProxy}</button>
          <button type="button" className={life?.pending_config_change ? "pending" : ""} disabled={lifeBusy || !life || life.proxy_state !== "running"} onClick={() => void act("restart")}>{t.restartProxy}{life?.pending_config_change ? " ●" : ""}</button>
          <button className="danger" type="button" disabled={lifeBusy} onClick={async () => {
            if (!window.confirm(t.exitConfirm)) return;
            setLifeBusy(true);
            setLifeMsg(t.exiting);
            try {
              await api.exitVeil();
              // Control plane has acknowledged exit; poll until it stops
              // responding so the UI reflects the offline state promptly
              // instead of waiting up to 2s for the global status refresh.
              for (let i = 0; i < 30; i++) {
                try { await api.lifecycle(); } catch { break; }
                await new Promise((resolve) => setTimeout(resolve, 200));
              }
              setLifeMsg(t.exitedHint);
            } catch (error) {
              setLifecycleErr(errorText(error));
            }
            setLifeBusy(false);
          }}>{t.exitVeil}</button>
        </div>
        {lifecycleErr && <p className="err">{t.statusUnavailable}{lifecycleErr}</p>}
        {life?.last_lifecycle_error && <p className="err">{life.last_lifecycle_error}</p>}
        {lifeMsg && <p className="flash">{lifeMsg}</p>}
      </div>

      <section className="metric-section" aria-labelledby="cache-metrics-title">
        <h2 id="cache-metrics-title">{t.cacheMetrics}</h2>
        <p className="section-help">{t.cacheMetricHint}</p>
        <div className="cards metric-cards">
          <MetricCard title={t.statReqs} value={lastUpdated ? stats.n : "—"} unit={t.unitEvents} scope={t.cacheScopeShort} hint={t.statReqsHint} />
          <MetricCard title={t.statHits} value={lastUpdated ? stats.hits : "—"} unit={t.unitRequests} scope={t.cacheScopeShort} hint={t.statHitsHint} />
          <MetricCard title={t.statErrs} value={lastUpdated ? stats.errors : "—"} unit={t.unitRequests} scope={t.cacheScopeShort} hint={t.statErrsHint} />
          <MetricCard title={t.statAvg} value={lastUpdated ? stats.avg : "—"} unit="ms" scope={t.cacheScopeShort} hint={t.statAvgHint} />
          <MetricCard title={t.statOk} value={lastUpdated ? stats.ok : "—"} unit={t.unitResponses} scope={t.cacheScopeShort} hint={t.statOkHint} />
          <MetricCard title={t.statSse} value={lastUpdated ? stats.sse : "—"} unit={t.unitResponses} scope={t.cacheScopeShort} hint={t.statSseHint} />
        </div>
        {events.length === 0 && !trafficErr && <p className="muted">{t.emptyDash}</p>}
        <div className="card type-card">
          <div className="metric-head">
            <div>
              <div className="label">{t.hitMix}</div>
              <div className="metric-scope">{t.cacheScopeShort} · {t.unitOccurrences}</div>
            </div>
          </div>
          <p className="metric-hint">{t.hitMixHint}</p>
          <HBars rows={mix} />
        </div>
      </section>

      <section className="metric-section" aria-labelledby="window-metrics-title">
        <h2 id="window-metrics-title">{t.windowMetrics}</h2>
        <p className="section-help">{t.windowMetricHint}</p>
        <div className="card chart-card">
          <div className="metric-head">
            <div>
              <div className="label">{t.reqOverTime}</div>
              <div className="metric-scope">{t.windowScopeShort} · {t.unitPerMinute} · {t.refreshEvery2s}</div>
            </div>
          </div>
          {!coverage.complete && events.length > 0 && (
            <p className="metric-note">{t.windowCoverageShort(coverage.minutes)}</p>
          )}
          <BarChart values={series} labels={labels} ariaLabel={t.reqOverTime} />
        </div>
      </section>

      <div className="update-row">
        <span className="muted">{t.version}: {ver || "?"}{upd ? (upd.newer ? ` → ${upd.latest}` : ` · ${t.upToDate}`) : ""}</span>
        <button type="button" onClick={async () => {
          setUpdMsg("");
          try {
            const next = await api.checkUpdate();
            setUpd(next);
            setUpdMsg(next.newer ? t.updateAvail : t.upToDate);
          } catch (error) { setUpdMsg(updErrText(classifyUpdateError(String(error)))); }
        }}>{t.checkUpdate}</button>
        {upd?.newer && (
          <button type="button" onClick={async () => {
            setUpdMsg(t.updating);
            try { await api.applyUpdate(); } catch (error) { setUpdMsg(updErrText(classifyUpdateError(String(error)))); }
          }}>{t.updateNow}</button>
        )}
        {updMsg && <span className="flash">{updMsg}</span>}
      </div>
    </section>
  );
}
