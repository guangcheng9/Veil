import { useEffect, useState } from "react";
import { api, type RuleView } from "../api";
import { useT } from "../i18n";

export default function RulesPage() {
  const { t } = useT();
  const [rules, setRules] = useState<RuleView[]>([]);
  const [open, setOpen] = useState<string | null>(null);
  const [allow, setAllow] = useState("");
  const [flash, setFlash] = useState("");
  const [nid, setNid] = useState("custom1");
  const [ntp, setNtp] = useState("CUSTOM");
  const [npat, setNpat] = useState("");
  const [ndesc, setNdesc] = useState("");
  const [rulesRev, setRulesRev] = useState(0);
  const [allowRev, setAllowRev] = useState(0);
  const load = () => {
    api.rules().then((r) => { setRules(r.rules); setRulesRev(r.revision); }).catch(() => undefined);
    api.allowlist().then((a) => { setAllow(a.allowlist.join("\n")); setAllowRev(a.revision); }).catch(() => undefined);
  };
  useEffect(load, []);
  const saveRules = async (next: RuleView[]) => {
    try {
      await api.putRules(next, rulesRev);
      setRules(next);
      setRulesRev((r) => r + 1);
      setAllowRev((r) => r + 1);
      setFlash(t.saved);
    } catch (e) {
      const msg = String(e);
      if (msg.startsWith("409")) { setFlash(t.configChangedHint ?? "Config changed; refresh"); void load(); return; }
      setFlash(t.failed);
    }
  };
  const patch = (id: string, part: Partial<RuleView>) =>
    saveRules(rules.map((x) => (x.id === id ? { ...x, ...part } : x)));
  return (
    <section>
      <h1>{t.rulesTitle}</h1>
      <p className="help">{t.helpRules}</p>
      {flash && <p className="flash">{flash}</p>}
      {rules.map((r) => (
        <div className="rule" key={r.id}>
          <div className="rule-head" onClick={() => setOpen(open === r.id ? null : r.id)} role="button" tabIndex={0}>
            <span className={"switch" + (r.enabled ? " on" : "")} onClick={(e) => { e.stopPropagation(); void patch(r.id, { enabled: !r.enabled }); }} />
            <strong>{r.id}</strong>
            <span className="chip">{r.type_prefix}</span>
            <span className="meta">{r.kind} · prio {r.priority} · {r.source}</span>
            {r.description && <span className="desc">{r.description}</span>}
          </div>
          {open === r.id && (
            <div className="rule-body">
              {r.kind === "ip" && <p className="help">{t.builtinHint}</p>}
              <div className="row">
                <div className="field"><label>{t.colType}</label><input value={r.type_prefix} onChange={(e) => setRules(rules.map((x) => x.id === r.id ? { ...x, type_prefix: e.target.value } : x))} /></div>
                <div className="field"><label>{t.colPrio}</label><input type="number" value={r.priority} onChange={(e) => setRules(rules.map((x) => x.id === r.id ? { ...x, priority: Number(e.target.value) } : x))} /></div>
              </div>
              <div className="field"><label>{t.colDesc}</label><input value={r.description ?? ""} onChange={(e) => setRules(rules.map((x) => x.id === r.id ? { ...x, description: e.target.value || null } : x))} /></div>
              {r.kind !== "ip" && r.kind !== "dictionary" && (
                <div className="field"><label>{t.pattern}</label><textarea rows={3} value={r.pattern ?? ""} onChange={(e) => setRules(rules.map((x) => x.id === r.id ? { ...x, pattern: e.target.value } : x))} /></div>
              )}
              {r.kind === "dictionary" && (
                <div className="field"><label>{t.words}</label><textarea rows={4} value={(r.words ?? []).join("\n")} onChange={(e) => setRules(rules.map((x) => x.id === r.id ? { ...x, words: e.target.value.split(/\n/).map((w) => w.trim()).filter(Boolean) } : x))} /></div>
              )}
              <button className="btn" type="button" onClick={() => void saveRules(rules)}>{t.saveRule}</button>
              {r.source === "custom" && (
                <button className="danger" type="button" onClick={() => { setOpen(null); void saveRules(rules.filter((x) => x.id !== r.id)); }}>{t.deleteRule}</button>
              )}
            </div>
          )}
        </div>
      ))}
      <h2>{t.addRule}</h2>
      <div className="row">
        <div className="field"><label>{t.colId}</label><input value={nid} onChange={(e) => setNid(e.target.value)} /></div>
        <div className="field"><label>{t.colType}</label><input value={ntp} onChange={(e) => setNtp(e.target.value)} /></div>
      </div>
      <div className="field"><label>{t.pattern}</label><input value={npat} onChange={(e) => setNpat(e.target.value)} placeholder="(?<!\\d)1[3-9]\\d{9}(?!\\d)" /></div>
      <div className="field"><label>{t.colDesc}</label><input value={ndesc} onChange={(e) => setNdesc(e.target.value)} /></div>
      <button className="btn" type="button" onClick={() => {
        if (!nid || !ntp || !npat) return;
        void saveRules([...rules, { id: nid, type_prefix: ntp, enabled: true, priority: 50, kind: "regex", source: "custom", pattern: npat, description: ndesc || null }]);
        setNpat("");
        setNdesc("");
      }}>{t.addRule}</button>
      <h2>{t.allowlist}</h2>
      <p className="help">{t.helpRules}</p>
      <textarea value={allow} onChange={(e) => setAllow(e.target.value)} rows={4} />
      <button className="btn" type="button" onClick={async () => {
        try {
          await api.putAllowlist(allow.split(/\s+/).filter(Boolean), allowRev);
          setAllowRev((r) => r + 1);
          setRulesRev((r) => r + 1);
          setFlash(t.saved);
        } catch (e) {
          const msg = String(e);
          if (msg.startsWith("409")) { setFlash(t.configChangedHint ?? "Config changed; refresh"); void load(); return; }
          setFlash(t.failed);
        }
      }}>{t.saveAllow}</button>
    </section>
  );
}
