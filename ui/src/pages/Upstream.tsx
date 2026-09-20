import { useEffect, useState } from "react";
import { api, type ClientInfo, type ClientRoute } from "../api";
import { useT } from "../i18n";
import { normalizeUpstreamBase } from "../upstream";

export default function UpstreamPage() {
  const { t } = useT();
  const [a, setA] = useState("");
  const [c, setC] = useState("");
  const [r, setR] = useState("");
  const [proxy, setProxy] = useState("");
  const [revision, setRevision] = useState(0);
  const [routes, setRoutes] = useState<ClientRoute[]>([]);
  const [apps, setApps] = useState<ClientInfo[]>([]);
  const [ttl, setTtl] = useState(90);
  const [lim, setLim] = useState(32);
  const [shutdown, setShutdown] = useState(5);
  const [flash, setFlash] = useState("");
  const [normalizedFields, setNormalizedFields] = useState<string[]>([]);
  const [busy, setBusy] = useState("");
  const [tab, setTab] = useState("general");
  const normalizeField = (label: string, value: string, setter: (value: string) => void) => {
    const normalized = normalizeUpstreamBase(value);
    if (normalized !== value) {
      setter(normalized);
      setNormalizedFields((current) => current.includes(label) ? current : [...current, label]);
      setFlash(t.upstreamNormalized(label, normalized));
    }
  };
  const load = () => {
    api.upstream().then((u) => {
      setA(u.anthropic_upstream);
      setC(u.openai_completions_upstream);
      setR(u.openai_responses_upstream);
      setProxy(u.upstream_proxy);
      setRevision(u.revision);
      setRoutes(u.routes ?? []);
    }).catch(() => undefined);
    api.clients().then(setApps).catch(() => undefined);
    api.status().then((s) => { setTtl(s.mapping_ttl_days); setLim(s.request_body_limit_mib); setShutdown(s.graceful_shutdown_timeout_secs ?? 5); }).catch(() => undefined);
  };
  useEffect(load, []);
  const save = async () => {
    const normalizedA = normalizeUpstreamBase(a);
    const normalizedC = normalizeUpstreamBase(c);
    const normalizedR = normalizeUpstreamBase(r);
    const normalizedRoutes = routes.map((item) => ({
      ...item,
      upstream: normalizeUpstreamBase(item.upstream),
    }));
    const newlyNormalized = [
      ...(normalizedA !== a ? ["Anthropic"] : []),
      ...(normalizedC !== c ? ["OpenAI chat"] : []),
      ...(normalizedR !== r ? ["OpenAI responses"] : []),
      ...routes.filter((item, index) => normalizedRoutes[index].upstream !== item.upstream).map((item) => item.label),
    ];
    try {
      await api.putUpstream({
        anthropic_upstream: normalizedA,
        openai_completions_upstream: normalizedC,
        openai_responses_upstream: normalizedR,
        upstream_proxy: proxy, revision, routes: normalizedRoutes,
      });
      setA(normalizedA);
      setC(normalizedC);
      setR(normalizedR);
      setRoutes(normalizedRoutes);
      const changed = [...new Set([...normalizedFields, ...newlyNormalized])];
      setNormalizedFields([]);
      setFlash(changed.length ? t.upstreamNormalizedSaved(changed.join(", ")) : t.savedRestart);
      load();
    } catch (e) { setFlash(String(e)); }
  };
  const install = async (id: string) => {
    setBusy(id);
    setFlash("");
    try {
      await api.setupClient(id);
      setFlash(t.installedHint);
      load();
      setTab(id);
    } catch { setFlash(t.failed); }
    setBusy("");
  };
  const active = apps.find((x) => x.id === tab);
  const route = routes.find((x) => x.id === tab);
  return (
    <section>
      <h1>{t.upTitle}</h1>
      <p className="help">{t.helpUp}</p>
      {flash && <p className="flash">{flash}</p>}
      <div className="ptabs">
        <button type="button" className={tab === "general" ? "on" : ""} onClick={() => setTab("general")}>{t.tabGeneral}</button>
        {apps.map((row) => (
          <button key={row.id} type="button" className={tab === row.id ? "on" : ""} onClick={() => setTab(row.id)}>
            {row.label}{row.detected ? "" : ""}
          </button>
        ))}
      </div>
      {tab === "general" && (
        <>
          <p className="help">{t.helpGeneralUp}</p>
          <p className="muted">{t.upstreamBaseHint}</p>
          <label>Anthropic<input value={a} onChange={(e) => { setA(e.target.value); setNormalizedFields((current) => current.filter((x) => x !== "Anthropic")); }} onBlur={() => normalizeField("Anthropic", a, setA)} /></label>
          <label>OpenAI chat<input value={c} onChange={(e) => { setC(e.target.value); setNormalizedFields((current) => current.filter((x) => x !== "OpenAI chat")); }} onBlur={() => normalizeField("OpenAI chat", c, setC)} /></label>
          <label>OpenAI responses<input value={r} onChange={(e) => { setR(e.target.value); setNormalizedFields((current) => current.filter((x) => x !== "OpenAI responses")); }} onBlur={() => normalizeField("OpenAI responses", r, setR)} /></label>
          <label>{t.upstreamProxy}<input value={proxy} placeholder={t.upstreamProxyPh} onChange={(e) => setProxy(e.target.value)} /></label>
          <button type="button" onClick={() => void save()}>{t.saveUp}</button>
          <div className="row">
            <label>{t.ttl}<input type="number" value={ttl} onChange={(e) => setTtl(Number(e.target.value))} /></label>
            <label>{t.bodyMib}<input type="number" value={lim} onChange={(e) => setLim(Number(e.target.value))} /></label>
            <label>{t.shutdownTimeout}<input type="number" min={1} max={60} value={shutdown} onChange={(e) => setShutdown(Number(e.target.value))} /></label>
          </div>
          <button type="button" onClick={async () => {
            try {
              await api.putSettings({ mapping_ttl_days: ttl, request_body_limit_mib: lim, graceful_shutdown_timeout_secs: shutdown });
              setFlash(t.saved);
            } catch { setFlash(t.failed); }
          }}>{t.saveLim}</button>
        </>
      )}
      {active && (
        <>
          <p className="help">
            {active.detected ? t.appFound : t.appMissing}
            {active.wired ? " · " + t.appWired : " · " + t.appNotWired}
          </p>
          {active.app_base_url && (
            <p className="muted">{t.appCurrentUrl}: <code>{active.app_base_url}</code></p>
          )}
          {active.wired && route ? (
            <>
              <p className="help">{t.helpAppUp(active.label, String(route.port))}</p>
              <p className="muted">{t.upstreamBaseHint}</p>
              <label>{t.tabUpstream}
                <input value={route.upstream} onChange={(e) => {
                  setNormalizedFields((current) => current.filter((x) => x !== route.label));
                  setRoutes(routes.map((x) => x.id === route.id ? { ...x, upstream: e.target.value } : x));
                }} onBlur={() => {
                  const normalized = normalizeUpstreamBase(route.upstream);
                  if (normalized !== route.upstream) {
                    setRoutes(routes.map((x) => x.id === route.id ? { ...x, upstream: normalized } : x));
                    setNormalizedFields((current) => current.includes(route.label) ? current : [...current, route.label]);
                    setFlash(t.upstreamNormalized(route.label, normalized));
                  }
                }} />
              </label>
              <p className="muted">{t.clientBase}: <code>{active.veil_url}</code></p>
              <button type="button" onClick={() => void save()}>{t.saveUp}</button>
            </>
          ) : (
            <p className="help">{t.installLead}</p>
          )}
          <button type="button" disabled={busy === active.id} onClick={() => void install(active.id)}>
            {busy === active.id ? t.loading : (active.wired ? t.reinstallApp : t.installApp)}
          </button>
        </>
      )}
    </section>
  );
}
