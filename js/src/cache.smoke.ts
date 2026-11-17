/** `xpict.svg` caches on the spec: same spec → same promise; failures evict. */
import { xpict, SvgCache } from "./index.js";

const a = xpict.svg("CCO");
const b = xpict.svg("CCO");
if (a !== b) throw new Error("same spec should share one promise");
const svg = await a;
if (!svg.startsWith("<svg")) throw new Error("expected svg");
if (xpict.svg("CCO", { color: "#c00" }) === a) throw new Error("options must change the key");

const doc = { type: "mol", smiles: "CCN" } as const;
if (xpict.svg(doc) !== xpict.svg({ ...doc })) throw new Error("equal docs share an entry");

const c = new SvgCache(2);
let made = 0;
const make = () => Promise.resolve(`s${++made}`);
c.get("x", make);
c.get("y", make);
c.get("x", make); // touch x so y is oldest
c.get("z", make);
if (c.peek("y") || !c.peek("x") || c.size !== 2) throw new Error("LRU should evict y");

const bad = c.get("e", () => Promise.reject(new Error("boom")));
await bad.catch(() => {});
await Promise.resolve();
if (c.peek("e")) throw new Error("failed render should be evicted");
console.log("cache smoke ok");
