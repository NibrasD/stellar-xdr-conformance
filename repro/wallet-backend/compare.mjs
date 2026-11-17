// Compares the participants wallet-backend recorded with the participants
// derived from the ledger meta. Missing ones make the scene "dropped";
// extra ones are reported for information only.
//
//   node compare.mjs <scenes.json> <recorded.json> <commit>

import { readFileSync } from "node:fs";

const [scenesPath, recordedPath, commit, flag] = process.argv.slice(2);
const control = flag === "--control";
const { scenes } = JSON.parse(readFileSync(scenesPath, "utf8"));
const recorded = JSON.parse(readFileSync(recordedPath, "utf8"));

const results = scenes.map((s) => {
  const got = new Set(recorded[s.id] ?? []);
  const want = s.expected_participants.all;
  const missing = want.filter((p) => !got.has(p));
  const extra = [...got].filter((p) => !want.includes(p));
  const role = (p) => Object.entries(s.roles ?? {}).find(([, v]) => v === p)?.[0] ?? "participant";
  return {
    id: s.id,
    status: recorded[s.id] === undefined ? "inconclusive" : missing.length ? "dropped" : "pass",
    recorded: [...got].sort(),
    expected: want,
    missing: missing.map((p) => ({ address: p, role: role(p) })),
    extra,
  };
});

process.stdout.write(JSON.stringify({
  runner: "wallet-backend",
  suite: "scenes",
  target: "github.com/stellar/wallet-backend",
  commit,
  control_patch: control ? "repro/wallet-backend/control-677.patch (local, not an upstream fix)" : null,
  component: "internal/indexer/processors.ParticipantsProcessor.GetTransactionParticipants",
  results,
}, null, 2) + "\n");
