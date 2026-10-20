import { useEffect, useRef, useState } from "react";
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
  const fileRef = useRef<HTMLInputElement>(null);
  const load = () => {
    api.rules().then((r) => { setRules(r.rules); setRulesRev(r.revision); }).catch(() => undefined);
    api.allowlist().then((a) => { setAllow(a.allowlist.join("\n")); setAllowRev(a.revision); }).catch(() => undefined);
  };
  useEffect(load, []);
  const saveRules = async (next: RuleView[]): Promise<boolean> => {
    try {
      await api.putRules(next, rulesRev);
      setRules(next);
      setRulesRev((r) => r + 1);
      setAllowRev((r) => r + 1);
      setFlash(t.rulesSavedRestart);
      return true;
    } catch (e) {
      const msg = String(e);
      if (msg.startsWith("409")) { setFlash(t.configChangedHint ?? "Config changed; refresh"); void load(); return false; }
      setFlash(t.failed);
      return false;
    }
  };
  const patch = (id: string, part: Partial<RuleView>) =>
    saveRules(rules.map((x) => (x.id === id ? { ...x, ...part } : x)));
  const exportCustom = () => {
    const custom = rules.filter((rule) => rule.source === "custom");
    const blob = new Blob([JSON.stringify({ version: 1, rules: custom }, null, 2)], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = "veil-custom-rules.json";
    link.click();
    URL.revokeObjectURL(url);
    setFlash(t.rulesExported);
  };
  const importCustom = async (file: File) => {
    try {
      const parsed: unknown = JSON.parse(await file.text());
      const rows = Array.isArray(parsed) ? parsed : (parsed && typeof parsed === "object" && Array.isArray((parsed as { rules?: unknown }).rules) ? (parsed as { rules: unknown[] }).rules : null);
      if (!rows) throw new Error("rules must be an array");
      const imported: RuleView[] = rows.map((row, index) => {
        if (!row || typeof row !== "object") throw new Error(`rule ${index + 1} is invalid`);
        const value = row as Partial<RuleView>;
        if (!value.id || !value.type_prefix) throw new Error(`rule ${index + 1} needs id and type`);
        const words = Array.isArray(value.words) ? value.words.filter((word): word is string => typeof word === "string" && word.length > 0) : undefined;
        if (!value.pattern && !words?.length) throw new Error(`rule ${value.id} needs pattern or words`);
        return {
          id: String(value.id),
          type_prefix: String(value.type_prefix),
          enabled: value.enabled !== false,
          priority: Number.isFinite(value.priority) ? Number(value.priority) : 50,
          kind: words?.length ? "dictionary" : "regex",
          source: "custom",
          pattern: words?.length ? null : String(value.pattern),
          words: words?.length ? words : null,
          description: value.description ? String(value.description) : null,
        };
      });
      const existing = rules.filter((rule) => rule.source !== "custom");
      if (await saveRules([...existing, ...imported])) setFlash(t.rulesImported);
    } catch (error) {
      setFlash(`${t.rulesImportFailed}: ${String(error).replace(/^Error:\s*/, "")}`);
    } finally {
      if (fileRef.current) fileRef.current.value = "";
    }
  };
  const resetDefaults = async () => {
    if (!window.confirm(t.rulesResetConfirm)) return;
    try {
      await api.resetRules();
      setOpen(null);
      load();
      setFlash(t.rulesResetRestart);
    } catch {
      setFlash(t.failed);
    }
  };
  return (
    <section>
      <h1>{t.rulesTitle}</h1>
      <p className="help">{t.helpRules}</p>
      {flash && <p className="flash">{flash}</p>}
      <div className="rule-actions">
        <button className="btn" type="button" onClick={resetDefaults}>{t.resetRules}</button>
        <button className="ghost" type="button" onClick={exportCustom}>{t.exportRules}</button>
        <button className="ghost" type="button" onClick={() => fileRef.current?.click()}>{t.importRules}</button>
        <input ref={fileRef} type="file" accept="application/json,.json" hidden onChange={(e) => {
          const file = e.target.files?.[0];
          if (file) void importCustom(file);
        }} />
      </div>
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
