// Independent re-extraction of every captured entry with the JavaScript SDK.
//
// The generator extracts captured entries with rs-stellar-xdr. This script
// repeats the extraction from the same saved LedgerCloseMeta with a second,
// unrelated codec (@stellar/stellar-sdk, js-xdr) and requires byte-identical
// results, so no expected value is produced and checked by one codec alone.
//
//   node crosscheck.mjs [--corpus ../../corpus]

import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const require = createRequire(import.meta.url);
const { xdr } = require("sdk-17.4.0");

const i = process.argv.indexOf("--corpus");
const corpus = resolve(i > -1 ? process.argv[i + 1] : join(here, "../../corpus"));
const manifest = JSON.parse(readFileSync(join(corpus, "manifest.json"), "utf8"));
const hex = (v) => Buffer.from(v.toXDR()).toString("hex");

function locate(e) {
  const lcm = xdr.LedgerCloseMeta.fromXDR(readFileSync(join(corpus, e.source.ledger_close_meta.file)));
  const body = lcm[lcm.type];
  if (body.ledgerHeader.header.ledgerSeq !== e.source.ledger) throw new Error("ledger sequence differs");
  if (hex(body.ledgerHeader.hash) !== e.source.ledger_hash) throw new Error("ledger hash differs");
  const tx = body.txProcessing.find((t) => hex(t.result.transactionHash) === e.source.tx_hash);
  if (!tx) throw new Error("transaction not in ledger");
  return tx.txApplyProcessing[tx.txApplyProcessing.type];
}

const STATE = { ledgerEntryState: "state", ledgerEntryUpdated: "updated", ledgerEntryCreated: "created" };

function addedSigner(meta) {
  const before = [];
  const after = [];
  for (const op of meta.operations) {
    for (const ch of op.changes) {
      const entry = ch[STATE[ch.type]];
      if (!entry || entry.data.type !== "account") continue;
      const keys = entry.data.account.signers.map((s) => s.key);
      (ch.type === "ledgerEntryState" ? before : after).push(...keys);
    }
  }
  const added = after.filter(
    (k) => k.type === "signerKeyTypeEd25519SignedPayload" && !before.some((b) => hex(b) === hex(k)),
  );
  if (added.length !== 1) throw new Error(`expected one added signed-payload signer, found ${added.length}`);
  return added[0];
}

const EXTRACT = {
  ScVal: (meta) => meta.sorobanMeta.returnValue,
  SignerKey: addedSigner,
};

let failed = 0;
const captured = manifest.entries.filter((e) => !e.synthetic);
for (const e of captured) {
  try {
    const value = EXTRACT[e.xdr_type](locate(e));
    const same = Buffer.compare(Buffer.from(value.toXDR()), readFileSync(join(corpus, e.file))) === 0;
    if (!same) throw new Error("bytes differ from the rs-stellar-xdr extraction");
    console.log(`  ${e.id.padEnd(36)} identical`);
  } catch (err) {
    failed++;
    console.error(`  ${e.id.padEnd(36)} FAILED: ${err.message}`);
  }
}
if (failed) {
  console.error(`${failed} of ${captured.length} captured entries differ`);
  process.exit(1);
}
console.log(`all ${captured.length} captured entries re-extracted identically with @stellar/stellar-sdk`);
