import { readdir, readFile, writeFile, mkdir } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.dirname(fileURLToPath(import.meta.url));
const attacksDir = path.join(root, "attacks");
const reportsDir = path.join(root, "reports");

await mkdir(reportsDir, { recursive: true });

const attacks = [];
for (const file of await readdir(attacksDir)) {
  if (!file.endsWith(".json")) {
    continue;
  }
  const raw = await readFile(path.join(attacksDir, file), "utf8");
  attacks.push(JSON.parse(raw));
}

const timestamp = new Date().toISOString();
const report = [
  "# ESPASS Security Lab Report",
  "",
  `Generated: ${timestamp}`,
  "",
  "| Attack | Severity | Expected Result | Status |",
  "| --- | --- | --- | --- |",
  ...attacks.map((attack) =>
    `| ${attack.name} | ${attack.severity} | ${attack.expected_result} | designed |`,
  ),
  "",
  "## Mitigations",
  "",
  ...attacks.map((attack) =>
    `- ${attack.name}: exploitability ${attack.exploitability}/10; mitigation: ${attack.mitigation}`,
  ),
  "",
].join("\n");

await writeFile(path.join(reportsDir, "latest.md"), report, "utf8");
console.log(report);
