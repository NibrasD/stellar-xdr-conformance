// Builds the divergence matrix from results/*.json and checks it against
// expectations/known-outcomes.json.
//
//   node tools/report.mjs            # write REPORT.md, exit 1 on any regression
//
// Exit code 1 when:
//   - any decode, round-trip, structure or xdr-json check fails;
//   - a native conversion loses entries and that outcome is not listed
//     in expectations/known-outcomes.json;
//   - a listed outcome no longer occurs (an upstream fix landed: update
//     the expectations file in the same pull request).

import { readFileSync, readdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const manifest = JSON.parse(readFileSync(join(root, "corpus/manifest.json"), "utf8"));
const expectations = JSON.parse(readFileSync(join(root, "expectations/known-outcomes.json"), "utf8"));
const known = expectations.outcomes;
const knownApi = expectations.api_outcomes ?? [];
const apiVectors = JSON.parse(readFileSync(join(root, "corpus/api-vectors.json"), "utf8")).vectors;

const RUNNER_ORDER = ["rust", "go", "js", "python"];
const allRuns = readdirSync(join(root, "results"))
  .filter((f) => f.endsWith(".json"))
  .map((f) => JSON.parse(readFileSync(join(root, "results", f), "utf8")))
  .sort((a, b) =>
    RUNNER_ORDER.indexOf(a.runner) - RUNNER_ORDER.indexOf(b.runner) ||
    a.sdk_version.localeCompare(b.sdk_version, undefined, { numeric: true }));
const runs = allRuns.filter((r) => r.suite !== "api");
const apiRuns = allRuns.filter((r) => r.suite === "api");

if (runs.length === 0) {
  console.error("no results in results/: run the runners first");
  process.exit(1);
}

const CODEC_CHECKS = ["decode", "roundtrip", "structure", "xdr_json"];
const problems = [];

function expected(run, id) {
  return known.find((k) => k.runner === run.runner && k.sdk_version === run.sdk_version && k.entries.includes(id));
}

for (const run of runs) {
  const label = `${run.runner} ${run.sdk_version}`;
  const ids = new Set(run.results.map((r) => r.id));
  for (const e of manifest.entries) {
    if (!ids.has(e.id)) problems.push(`${label}: no result for ${e.id}`);
  }
  for (const r of run.results) {
    for (const c of CODEC_CHECKS) {
      if (r[c] === "fail") problems.push(`${label} ${r.id}: ${c} failed. ${r.detail}`);
    }
    const k = expected(run, r.id);
    if (k && r.native !== k.native) {
      problems.push(`${label} ${r.id}: expected native=${k.native} (${k.reason}) but got ${r.native}. Update expectations/known-outcomes.json if this is an upstream fix.`);
    }
    if (!k && !["preserved", "rejected", "n/a"].includes(r.native)) {
      problems.push(`${label} ${r.id}: native=${r.native} is a new divergence. ${r.detail}`);
    }
  }
}

// API vectors: a defect must be listed in api_outcomes, and a listed defect
// must still occur.
for (const run of apiRuns) {
  const label = `${run.runner} ${run.sdk_version} (api)`;
  const ids = new Set(run.results.map((r) => r.id));
  for (const v of apiVectors) {
    if (!ids.has(v.id)) problems.push(`${label}: no result for ${v.id}`);
  }
  for (const r of run.results) {
    const k = knownApi.find((x) => x.runner === run.runner && x.sdk_version === run.sdk_version && x.vectors.includes(r.id));
    if (k && r.status !== "defect") {
      problems.push(`${label} ${r.id}: expected a defect (${k.reason}) but got ${r.status}. Update expectations/known-outcomes.json if this is an upstream fix.`);
    }
    if (!k && r.status === "defect") problems.push(`${label} ${r.id}: new defect. ${r.detail}`);
  }
}

const apiCell = (r) => (!r ? "missing" : r.status === "defect" ? "**defect**" : r.status === "ok" ? "correct" : "no matching API");
const apiHeader = ["API vector", "Expected", ...apiRuns.map((r) => `${r.runner}<br>\`${r.sdk}\` ${r.sdk_version}`)];
const apiRows = apiVectors.map((v) => [
  `\`${v.id}\`<br>${v.title}`,
  v.expected.outcome.replaceAll("_", " "),
  ...apiRuns.map((run) => apiCell(run.results.find((r) => r.id === v.id))),
]);
const apiDetails = apiRuns.flatMap((run) =>
  run.results.filter((r) => r.status === "defect").map((r) => `- **${run.runner} ${run.sdk_version}**, \`${r.id}\`: ${r.detail}`));

const cell = (r) => {
  if (!r) return "missing";
  const codec = CODEC_CHECKS.every((c) => r[c] !== "fail");
  const native = {
    preserved: "keeps all entries",
    rejected: "raises an error",
    "silent-loss": "**loses entries silently**",
    unexpected: "**unexpected result**",
    "n/a": "no native conversion",
  }[r.native] ?? r.native;
  return codec ? native : `**codec check failed**; ${native}`;
};

const header = ["Corpus entry", ...runs.map((r) => `${r.runner}<br>\`${r.sdk}\` ${r.sdk_version}`)];
const rows = manifest.entries.map((e) => [
  `\`${e.id}\`<br>${e.title}`,
  ...runs.map((run) => cell(run.results.find((r) => r.id === e.id))),
]);

const codecSummary = runs.map((run) => {
  const fails = run.results.flatMap((r) => CODEC_CHECKS.filter((c) => r[c] === "fail").map((c) => `${r.id}:${c}`));
  const ran = CODEC_CHECKS.filter((c) => run.results.some((r) => r[c] !== "n/a"));
  return `| ${run.runner} ${run.sdk_version} | ${run.runtime} | ${ran.join(", ")} | ${fails.length ? fails.join(", ") : "all pass"} |`;
});

const details = runs.flatMap((run) =>
  run.results
    .filter((r) => r.native === "silent-loss" || r.native === "unexpected")
    .map((r) => `- **${run.runner} ${run.sdk_version}**, \`${r.id}\`: ${r.detail}`));

const md = `# Conformance report

Generated by \`tools/report.mjs\` from \`results/\`. Do not edit by hand.

Corpus: ${manifest.entries.length} entries, manifest ${manifest.manifest_version}, generated with rs-stellar-xdr ${manifest.generator.stellar_xdr}.

## Native conversion

Rule for every map entry: the SDK's conversion to native values must keep every entry or raise an error. Returning fewer entries without an error is a defect.

| ${header.join(" | ")} |
| ${header.map(() => "---").join(" | ")} |
${rows.map((r) => `| ${r.join(" | ")} |`).join("\n")}

## Codec checks

Decode, byte-exact re-encode, decoded structure against the manifest, and xdr-json against rs-stellar-xdr where the SDK exposes it.

| Runner | Runtime | Checks run | Result |
| --- | --- | --- | --- |
${codecSummary.join("\n")}

## Divergences

${details.length ? details.join("\n") : "None."}

## API vectors

Cases that exercise an SDK's builder or parser API rather than its decoder. Expected values come from rs-stellar-xdr (\`corpus/api-vectors.json\`).

${apiRuns.length ? `| ${apiHeader.join(" | ")} |
| ${apiHeader.map(() => "---").join(" | ")} |
${apiRows.map((r) => `| ${r.join(" | ")} |`).join("\n")}

${apiDetails.length ? apiDetails.join("\n") : "No defects."}` : "No API-vector results."}

## Status

${problems.length ? problems.map((p) => `- ${p}`).join("\n") : "Every outcome matches \`expectations/known-outcomes.json\`."}
`;

writeFileSync(join(root, "REPORT.md"), md);
console.log(md);
if (problems.length) {
  console.error(`\n${problems.length} problem(s) found`);
  process.exit(1);
}
