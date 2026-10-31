import { useMemo, useState } from "react";
import { Depict } from "./Depict.js";
import { NoteEditor } from "./NoteEditor.js";
import { loadCasesFromGlob } from "./catalog/fromGlob.js";
import { GROUP_ORDER, GROUP_TITLE } from "./catalog/types.js";
import {
  allNotes,
  importNotes,
  problemCount,
  type ReviewNote,
} from "./notes.js";

const CASES = loadCasesFromGlob();

function params() {
  return new URLSearchParams(window.location.search);
}

export function App() {
  const initial = params();
  const [group, setGroup] = useState(initial.get("group") ?? "");
  const [only, setOnly] = useState(initial.get("only") ?? "");
  const [problemsOnly, setProblemsOnly] = useState(false);
  const [tick, setTick] = useState(0);

  const visible = useMemo(() => {
    const notes = allNotes();
    return CASES.filter((c) => {
      if (only) return c.id === only;
      if (group && c.group !== group) return false;
      if (problemsOnly && notes[c.id]?.status !== "problem") return false;
      return true;
    });
  }, [group, only, problemsOnly, tick]);

  const nProblems = problemCount(CASES.map((c) => c.id));

  function pushQuery(next: { group?: string; only?: string }) {
    const q = new URLSearchParams();
    const g = next.group ?? group;
    const o = next.only ?? only;
    if (g) q.set("group", g);
    if (o) q.set("only", o);
    const qs = q.toString();
    window.history.replaceState(null, "", qs ? `?${qs}` : ".");
  }

  function exportNotes() {
    const blob = new Blob([JSON.stringify(allNotes(), null, 2)], {
      type: "application/json",
    });
    const a = document.createElement("a");
    a.href = URL.createObjectURL(blob);
    a.download = "xpict-review-notes.json";
    a.click();
    URL.revokeObjectURL(a.href);
  }

  function onImport(file: File) {
    file.text().then((text) => {
      importNotes(JSON.parse(text) as Record<string, ReviewNote>);
      setTick((n) => n + 1);
    });
  }

  return (
    <div className="app">
      <header className="top">
        <div>
          <p className="kicker">xpict · local review</p>
          <h1>Depiction gallery</h1>
          <p className="lede">
            Painted by <code>@xenosite/xpict</code> (JS/WASM). Cases are YAML
            under <code>js/review/cases/</code>.
          </p>
        </div>
        <div className="actions">
          <span className="meta">
            {visible.length}/{CASES.length} cases
            {nProblems ? ` · ${nProblems} problems` : ""}
          </span>
          <label className="chk">
            <input
              type="checkbox"
              checked={problemsOnly}
              onChange={(e) => {
                setProblemsOnly(e.target.checked);
                setTick((n) => n + 1);
              }}
            />
            problems
          </label>
          <button type="button" onClick={exportNotes}>
            Export notes
          </button>
          <label className="btn">
            Import
            <input
              type="file"
              accept="application/json"
              hidden
              onChange={(e) => {
                const f = e.target.files?.[0];
                if (f) onImport(f);
                e.target.value = "";
              }}
            />
          </label>
        </div>
      </header>

      <div className="body">
        <nav className="nav">
          <button
            type="button"
            className={!group && !only ? "on" : ""}
            onClick={() => {
              setGroup("");
              setOnly("");
              pushQuery({ group: "", only: "" });
            }}
          >
            All
          </button>
          {GROUP_ORDER.map((g) => (
            <button
              key={g}
              type="button"
              className={group === g && !only ? "on" : ""}
              onClick={() => {
                setGroup(g);
                setOnly("");
                pushQuery({ group: g, only: "" });
              }}
            >
              {GROUP_TITLE[g]}
              <span>
                {CASES.filter((c) => c.group === g).length}
              </span>
            </button>
          ))}
        </nav>

        <main>
          {visible.length === 0 ? (
            <p className="empty">No cases match.</p>
          ) : (
            visible.map((c) => (
              <article key={c.id} className="card" data-case={c.id}>
                <header>
                  <h2>
                    <a
                      href={`?only=${encodeURIComponent(c.id)}`}
                      onClick={(e) => {
                        e.preventDefault();
                        setOnly(c.id);
                        setGroup(c.group);
                        pushQuery({ group: c.group, only: c.id });
                      }}
                    >
                      {c.title}
                    </a>
                  </h2>
                  <code className="file">{c.file}</code>
                </header>
                {c.expect ? <p className="expect">{c.expect}</p> : null}
                <div className="review-figure">
                  <Depict spec={c.spec} />
                </div>
                <NoteEditor id={c.id} />
                <details className="source">
                  <summary>source</summary>
                  <pre>{c.raw.trimEnd()}</pre>
                </details>
              </article>
            ))
          )}
        </main>
      </div>
    </div>
  );
}
