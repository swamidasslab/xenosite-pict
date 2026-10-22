/**
 * Atom / bond coords in SVG viewBox space (same numbers Rust ``depict`` returns).
 */

export type SvgAtom = {
  index: number;
  element: string;
  x: number;
  y: number;
  label?: string;
  charge?: number;
};

export type SvgBond = {
  index: number;
  begin: number;
  end: number;
  order: number;
  stereo?: string;
};
