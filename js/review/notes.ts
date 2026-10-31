export type NoteStatus = "unset" | "ok" | "problem" | "later";

export type ReviewNote = {
  status: NoteStatus;
  text: string;
  updated: string;
};

const KEY = "xpict-review-notes";

function readAll(): Record<string, ReviewNote> {
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return {};
    const parsed = JSON.parse(raw) as Record<string, ReviewNote>;
    return parsed && typeof parsed === "object" ? parsed : {};
  } catch {
    return {};
  }
}

function writeAll(notes: Record<string, ReviewNote>): void {
  localStorage.setItem(KEY, JSON.stringify(notes));
}

export function loadNote(id: string): ReviewNote {
  return readAll()[id] ?? { status: "unset", text: "", updated: "" };
}

export function saveNote(id: string, patch: Partial<ReviewNote>): ReviewNote {
  const notes = readAll();
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

export function allNotes(): Record<string, ReviewNote> {
  return readAll();
}

export function importNotes(payload: Record<string, ReviewNote>): void {
  writeAll(payload);
}

export function problemCount(ids: string[]): number {
  const notes = readAll();
  return ids.filter((id) => notes[id]?.status === "problem").length;
}
