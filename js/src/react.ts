/**
 * React adapter: `<Pict>` draws a mol or document through the shared
 * {@link xpict.svg} cache. No layout or paint here — only fetch, lazy
 * visibility, and SSR hand-off.
 *
 * ```tsx
 * import { Pict } from "@xenosite/xpict/react";
 * <Pict smiles="CCO" opts={{ atom_shade: [0, 0.4, 0.9] }} />
 * <Pict spec={{ type: "reaction_scheme", children: [...] }} lazy={false} />
 * ```
 */
import { createElement, useEffect, useRef, useState, type CSSProperties, type ReactNode } from "react";
import { xpict, type DepictSpec, type MolRenderOptions } from "./api.js";

export type PictProps = {
  /** SMILES / CXSMILES / molfile (use `opts` for render options). */
  smiles?: string;
  /** Or a full depict document (mol, group, reaction_scheme, grid). */
  spec?: DepictSpec;
  opts?: MolRenderOptions;
  /** Draw when scrolled near the viewport (default) or at once. */
  lazy?: boolean;
  /** SVG prerendered on the server (`await xpict.svg(...)`); shown as-is. */
  svg?: string;
  /** Shown until the SVG is ready. */
  fallback?: ReactNode;
  /** Shown if the render fails (default: the input text). */
  error?: (err: unknown) => ReactNode;
  className?: string;
  style?: CSSProperties;
  title?: string;
};

let observer: IntersectionObserver | null = null;
const waiting = new WeakMap<Element, () => void>();

function whenVisible(el: Element, go: () => void): () => void {
  if (typeof IntersectionObserver === "undefined") {
    go();
    return () => {};
  }
  observer ??= new IntersectionObserver(
    (entries) => {
      for (const e of entries) {
        if (!e.isIntersecting) continue;
        observer!.unobserve(e.target);
        waiting.get(e.target)?.();
        waiting.delete(e.target);
      }
    },
    { rootMargin: "300px" }
  );
  waiting.set(el, go);
  observer.observe(el);
  return () => {
    observer?.unobserve(el);
    waiting.delete(el);
  };
}

type State = { svg?: string; err?: unknown };

/** A molecule or document as inline SVG, cached across the page. */
export function Pict(props: PictProps) {
  const { smiles, spec, opts, lazy = true, svg: ssr, fallback = null, error, className, style, title } = props;
  const input = spec ?? smiles ?? "";
  const key = JSON.stringify([input, spec ? {} : opts ?? {}]);
  const box = useRef<HTMLDivElement>(null);
  const [visible, setVisible] = useState(!lazy || !!ssr);
  const [state, setState] = useState<State>(ssr ? { svg: ssr } : {});

  useEffect(() => {
    if (visible || !box.current) return;
    return whenVisible(box.current, () => setVisible(true));
  }, [visible]);

  useEffect(() => {
    if (!visible || ssr || !input) return;
    let live = true;
    xpict.svg(input, spec ? {} : opts).then(
      (svg) => live && setState({ svg }),
      (err) => live && setState({ err })
    );
    return () => {
      live = false;
    };
    // `key` is the render identity; `opts` objects are often recreated.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [visible, key, ssr]);

  const cls = ["xpict-pict", className].filter(Boolean).join(" ");
  const label = title ?? (typeof input === "string" ? input : undefined);
  if (state.svg !== undefined) {
    return createElement("div", { ref: box, className: cls, style, title: label, dangerouslySetInnerHTML: { __html: state.svg } });
  }
  const body = state.err !== undefined ? (error ? error(state.err) : label ?? "") : fallback;
  return createElement("div", { ref: box, className: state.err !== undefined ? `${cls} xpict-error` : cls, style, title: label }, body);
}
