import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import type { Plugin } from "vite";

const here = dirname(fileURLToPath(import.meta.url));
export const NOTES_PATH = resolve(here, "notes.local.json");
const ROUTE = "/__review-notes";

function readNotesFile(): string {
  if (!existsSync(NOTES_PATH)) return "{}\n";
  return readFileSync(NOTES_PATH, "utf8");
}

/** GET/PUT `js/review/notes.local.json` for the local gallery (not published). */
export function reviewNotesPlugin(): Plugin {
  return {
    name: "review-notes",
    configureServer(server) {
      server.middlewares.use((req, res, next) => {
        const path = req.url?.split("?")[0];
        if (path !== ROUTE) {
          next();
          return;
        }
        if (req.method === "GET") {
          res.setHeader("Content-Type", "application/json");
          res.end(readNotesFile());
          return;
        }
        if (req.method === "PUT" || req.method === "POST") {
          const chunks: Buffer[] = [];
          req.on("data", (c) => chunks.push(Buffer.from(c)));
          req.on("end", () => {
            try {
              const raw = Buffer.concat(chunks).toString("utf8") || "{}";
              const parsed = JSON.parse(raw) as unknown;
              if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
                throw new Error("notes must be a JSON object");
              }
              writeFileSync(NOTES_PATH, `${JSON.stringify(parsed, null, 2)}\n`);
              res.statusCode = 204;
              res.end();
            } catch (err) {
              res.statusCode = 400;
              res.setHeader("Content-Type", "text/plain");
              res.end(err instanceof Error ? err.message : String(err));
            }
          });
          return;
        }
        next();
      });
    },
  };
}
