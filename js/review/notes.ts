export type NoteStatus = "unset" | "ok" | "problem" | "later";

export type ReviewNote = {
  status: NoteStatus;
  text: string;
  updated: string;
};

export type NoteMap = Record<string, ReviewNote>;

const KEY = "xpict-review-notes";
const DISK = "/__review-notes";

function emptyNote(): ReviewNote {
  return { status: "unset", text: "", updated: "" };
}

function readLocal(): NoteMap {
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return {};
    const parsed = JSON.parse(raw) as NoteMap;
    return parsed && typeof parsed === "object" ? parsed : {};
  } catch {
    return {};
  }
}

function writeLocal(notes: NoteMap): void {
  localStorage.setItem(KEY, JSON.stringify(notes));
}

function newer(a: ReviewNote | undefined, b: ReviewNote | undefined): ReviewNote | undefined {
  if (!a) return b;
  if (!b) return a;
  return (a.updated || "") >= (b.updated || "") ? a : b;
}

function mergeNotes(a: NoteMap, b: NoteMap): NoteMap {
  const out: NoteMap = { ...a };
  for (const [id, note] of Object.entries(b)) {
    const keep = newer(out[id], note);
    if (keep) out[id] = keep;
  }
  return out;
}

let persistTimer: ReturnType<typeof setTimeout> | undefined;

function persistDisk(notes: NoteMap): void {
  if (typeof fetch === "undefined") return;
  if (persistTimer) clearTimeout(persistTimer);
  persistTimer = setTimeout(() => {
    fetch(DISK, {
      method: "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(notes, null, 2) + "\n",
    }).catch(() => {
      /* Vite plugin missing (static preview) — localStorage still has it. */
    });
  }, 200);
}

function writeAll(notes: NoteMap): void {
  writeLocal(notes);
  persistDisk(notes);
}

/** Pull disk notes, merge with localStorage (newer `updated` wins), write both. */
export async function hydrateNotes(): Promise<NoteMap> {
  let disk: NoteMap = {};
  try {
    const r = await fetch(DISK);
    if (r.ok) {
      const parsed = (await r.json()) as NoteMap;
      if (parsed && typeof parsed === "object") disk = parsed;
    }
  } catch {
    disk = {};
  }
  const merged = mergeNotes(disk, readLocal());
  writeAll(merged);
  return merged;
}

export function loadNote(id: string): ReviewNote {
  return readLocal()[id] ?? emptyNote();
}

export function saveNote(id: string, patch: Partial<ReviewNote>): ReviewNote {
  const notes = readLocal();
  const next: ReviewNote = {
    ...loadNote(id),
    ...patch,
    updated: new Date().toISOString(),
  };
  if (next.status === "unset" && !next.text.trim()) {
    delete notes[id];
  } else {
    notes[id] = next;
  }
  writeAll(notes);
  return next;
}

export function allNotes(): NoteMap {
  return readLocal();
}

export function importNotes(payload: NoteMap): void {
  writeAll(payload);
}

export function problemCount(ids: string[]): number {
  const notes = readLocal();
  return ids.filter((id) => notes[id]?.status === "problem").length;
}
