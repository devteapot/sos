import fs from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const packageRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const dist = path.join(packageRoot, "dist");

if (path.dirname(dist) !== packageRoot || path.basename(dist) !== "dist") {
  throw new Error(`refusing to clean unexpected output path ${dist}`);
}

await fs.rm(dist, { recursive: true, force: true });
