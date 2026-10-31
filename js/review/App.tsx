import { useEffect, useMemo, useState } from "react";
import { LazyDepict } from "./Depict.js";
import { NoteEditor } from "./NoteEditor.js";
import { loadCasesFromGlob } from "./catalog/fromGlob.js";
import {
  GROUP_ORDER,
  GROUP_TITLE,
  type ReviewCase,
  type ReviewGroup,
} from "./catalog/types.js";
import {
  allNotes,
  hydrateNotes,
  importNotes,
  problemCount,
  type ReviewNote,
} from "./notes.js";

const CASES = loadCasesFromGlob();

function params() {
  return new URLSearchParams(window.location.search);
}

function allGroupSet() {
  return new Set<ReviewGroup>(GROUP_ORDER);
}

function allCaseSet() {
  return new Set(CASES.map((c) => c.id));
}

function initialOpen(): { groups: Set<ReviewGroup>; cases: Set<string> } {
  const q = params();
  const only = q.get("only");
  const group = q.get("group") as ReviewGroup | null;
  if (only) {
    const c = CASES.find((x) => x.id === only);
    return {
      groups: new Set(c ? [c.group] : []),
      cases: new Set([only]),
    };
  }
  if (group && GROUP_ORDER.includes(group)) {
    return {
      groups: new Set([group]),
      cases: new Set(CASES.filter((c) => c.group === group).map((c) => c.id)),
    };
  }
  return { groups: allGroupSet(), cases: allCaseSet() };
}

export function App() {
  const seed = useMemo(() => initialOpen(), []);
  const [openGroups, setOpenGroups] = useState<Set<ReviewGroup>>(
    () => seed.groups,
  );
  const [openCases, setOpenCases] = useState<Set<string>>(() => seed.cases);
  const [problemsOnly, setProblemsOnly] = useState(false);
  const [tick, setTick] = useState(0);

  const notes = useMemo(() => allNotes(), [tick]);
  const nProblems = problemCount(CASES.map((c) => c.id));

  useEffect(() => {
    hydrateNotes().then(() => setTick((n) => n + 1));
  }, []);

  const sections = useMemo(() => {
    return GROUP_ORDER.map((g) => ({
      group: g,
      title: GROUP_TITLE[g],
      cases: CASES.filter((c) => {
        if (c.group !== g) return false;
        if (problemsOnly && notes[c.id]?.status !== "problem") return false;
        return true;
      }),
    })).filter((s) => s.cases.length);
  }, [problemsOnly, notes]);

  const visibleCount = sections.reduce((n, s) => n + s.cases.length, 0);

  useEffect(() => {
    const only = params().get("only");
    if (!only) return;
    document.querySelector(`[data-case="${only}"]`)?.scrollIntoView({
      block: "start",
    });
  }, []);

  function expandAll() {
    setOpenGroups(allGroupSet());
    setOpenCases(allCaseSet());
  }

  function collapseAll() {
    setOpenGroups(new Set());
    setOpenCases(new Set());
  }

  function groupsOnly() {
    setOpenGroups(allGroupSet());
    setOpenCases(new Set());
  }

  function setGroupOpen(g: ReviewGroup, open: boolean) {
    setOpenGroups((prev) => {
      const next = new Set(prev);
      if (open) next.add(g);
      else next.delete(g);
      return next;
    });
  }

  function setGroupCases(ids: string[], open: boolean) {
    setOpenCases((prev) => {
      const next = new Set(prev);
      for (const id of ids) {
        if (open) next.add(id);
        else next.delete(id);
      }
      return next;
    });
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
      <header className="intro">
        <p className="kicker">xpict · local review</p>
        <h1>Depiction gallery</h1>
        <p className="lede">
          Painted by this checkout’s <code>@xenosite/xpict</code>. YAML under{" "}
          <code>js/review/cases/</code>. Figures paint when they enter view.
        </p>
      </header>
      <div className="bar">
        <span className="meta">
          {visibleCount}/{CASES.length} cases
          {nProblems ? ` · ${nProblems} problems` : ""}
        </span>
        <button type="button" onClick={expandAll}>
          Expand all
        </button>
        <button type="button" onClick={groupsOnly}>
          Groups
        </button>
        <button type="button" onClick={collapseAll}>
          Collapse all
        </button>
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

      <div className="tree">
        {sections.length === 0 ? (
          <p className="empty">No cases match.</p>
        ) : (
          sections.map((s) => (
            <GroupBlock
              key={s.group}
              group={s.group}
              title={s.title}
              cases={s.cases}
              notes={notes}
              open={openGroups.has(s.group)}
              openCases={openCases}
              onToggle={(open) => setGroupOpen(s.group, open)}
              onOpenCases={(open) =>
                setGroupCases(
                  s.cases.map((c) => c.id),
                  open,
                )
              }
              onToggleCase={(id, open) => setGroupCases([id], open)}
              onNote={() => setTick((n) => n + 1)}
            />
          ))
        )}
      </div>
    </div>
  );
}

function GroupBlock({
  group,
  title,
  cases,
  notes,
  open,
  openCases,
  onToggle,
  onOpenCases,
  onToggleCase,
  onNote,
}: {
  group: ReviewGroup;
  title: string;
  cases: ReviewCase[];
  notes: Record<string, ReviewNote>;
  open: boolean;
  openCases: Set<string>;
  onToggle: (open: boolean) => void;
  onOpenCases: (open: boolean) => void;
  onToggleCase: (id: string, open: boolean) => void;
  onNote: () => void;
}) {
  const nOpen = cases.filter((c) => openCases.has(c.id)).length;
  const nProb = cases.filter((c) => notes[c.id]?.status === "problem").length;

  return (
    <details
      className="group"
      data-group={group}
      open={open}
      onToggle={(e) => {
        const next = e.currentTarget.open;
        if (next !== open) onToggle(next);
      }}
    >
      <summary>
        <span className="group-title">{title}</span>
        <span className="group-count">
          {cases.length}
          {nOpen && nOpen !== cases.length ? ` · ${nOpen} open` : ""}
          {nProb ? ` · ${nProb} problem${nProb === 1 ? "" : "s"}` : ""}
        </span>
        <span className="group-tools">
          <button
            type="button"
            onClick={(e) => {
              e.preventDefault();
              e.stopPropagation();
              onToggle(true);
              onOpenCases(true);
            }}
          >
            Expand
          </button>
          <button
            type="button"
            onClick={(e) => {
              e.preventDefault();
              e.stopPropagation();
              onOpenCases(false);
            }}
          >
            Collapse
          </button>
        </span>
      </summary>
      {cases.map((c) => (
        <CaseBlock
          key={c.id}
          c={c}
          note={notes[c.id]}
          open={openCases.has(c.id)}
          onToggle={(next) => onToggleCase(c.id, next)}
          onNote={onNote}
        />
      ))}
    </details>
  );
}

function CaseBlock({
  c,
  note,
  open,
  onToggle,
  onNote,
}: {
  c: ReviewCase;
  note: ReviewNote | undefined;
  open: boolean;
  onToggle: (open: boolean) => void;
  onNote: () => void;
}) {
  const status = note?.status ?? "unset";
  return (
    <details
      className="case"
      data-case={c.id}
      open={open}
      onToggle={(e) => {
        const next = e.currentTarget.open;
        if (next !== open) onToggle(next);
      }}
    >
      <summary>
        <span className="case-title">{c.title}</span>
        {c.expect ? <span className="case-expect">{c.expect}</span> : null}
        {status !== "unset" ? (
          <span className={`chip chip-${status}`}>{status}</span>
        ) : null}
      </summary>
      <code className="file">{c.file}</code>
      <LazyDepict spec={c.spec} enabled={open} />
      <NoteEditor id={c.id} onChange={onNote} />
      <details className="source">
        <summary>source</summary>
        <pre>{c.raw.trimEnd()}</pre>
      </details>
    </details>
  );
}
