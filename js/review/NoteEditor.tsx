import { useEffect, useState } from "react";
import {
  loadNote,
  saveNote,
  type NoteStatus,
  type ReviewNote,
} from "./notes.js";

const STATUSES: { id: NoteStatus; label: string }[] = [
  { id: "unset", label: "—" },
  { id: "ok", label: "ok" },
  { id: "problem", label: "problem" },
  { id: "later", label: "later" },
];

export function NoteEditor({ id }: { id: string }) {
  const [note, setNote] = useState<ReviewNote>(() => loadNote(id));

  useEffect(() => {
    setNote(loadNote(id));
  }, [id]);

  function update(patch: Partial<ReviewNote>) {
    setNote(saveNote(id, patch));
  }

  return (
    <div className="note-editor" data-status={note.status}>
      <div className="note-status">
        {STATUSES.map((s) => (
          <label key={s.id}>
            <input
              type="radio"
              name={`note-${id}`}
              checked={note.status === s.id}
              onChange={() => update({ status: s.id })}
            />
            {s.label}
          </label>
        ))}
      </div>
      <textarea
        rows={2}
        placeholder="Note a problem…"
        value={note.text}
        onChange={(e) => update({ text: e.target.value })}
      />
    </div>
  );
}
