import { useEffect, useMemo, useRef, useState } from "react";
import { xpict, type DepictSpec } from "@xenosite/xpict"; // workspace file:.. → js/src

const svgCache = new Map<string, string>();

export function Depict({ spec }: { spec: DepictSpec }) {
  const key = useMemo(() => JSON.stringify(spec), [spec]);
  const [html, setHtml] = useState<string | null>(() => svgCache.get(key) ?? null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const hit = svgCache.get(key);
    if (hit) {
      setHtml(hit);
      setError(null);
      return;
    }
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
        svgCache.set(key, cells);
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

/** Paint only when the case is open and near the viewport. */
export function LazyDepict({
  spec,
  enabled,
}: {
  spec: DepictSpec;
  enabled: boolean;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [near, setNear] = useState(false);

  useEffect(() => {
    if (!enabled) {
      setNear(false);
      return;
    }
    const el = ref.current;
    if (!el) return;
    const io = new IntersectionObserver(
      ([entry]) => {
        if (entry.isIntersecting) setNear(true);
      },
      { rootMargin: "800px 0px" },
    );
    io.observe(el);
    return () => io.disconnect();
  }, [enabled]);

  return (
    <div ref={ref} className="review-figure">
      {enabled && near ? (
        <Depict spec={spec} />
      ) : (
        <p className="xpict-pending" data-ready="idle">
          {enabled ? "…" : ""}
        </p>
      )}
    </div>
  );
}
