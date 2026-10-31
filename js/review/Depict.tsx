import { useEffect, useMemo, useState } from "react";
import { xpict, type DepictSpec } from "@xenosite/xpict";

export function Depict({ spec }: { spec: DepictSpec }) {
  const key = useMemo(() => JSON.stringify(spec), [spec]);
  const [html, setHtml] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setHtml(null);
    setError(null);
    xpict
      .depict(spec)
      .then((rows) => {
        if (cancelled) return;
        const cells = rows
          .map((r) => `<div class="xpict-cell">${xpict.toSvg(r.scene)}</div>`)
          .join("");
        setHtml(cells);
      })
      .catch((err: unknown) => {
        if (cancelled) return;
        setError(err instanceof Error ? err.message : String(err));
      });
    return () => {
      cancelled = true;
    };
  }, [key, spec]);

  if (error) {
    return (
      <pre className="xpict-error" data-ready="error">
        {error}
      </pre>
    );
  }
  if (!html) {
    return (
      <p className="xpict-pending" data-ready="pending">
        painting…
      </p>
    );
  }
  return (
    <div
      className="xpict-depict"
      data-ready="1"
      dangerouslySetInnerHTML={{ __html: html }}
    />
  );
}
