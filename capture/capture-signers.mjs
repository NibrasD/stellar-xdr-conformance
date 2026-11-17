// Captures core-emitted CAP-0040 signed-payload signers from Stellar testnet.
//
// A fresh throwaway account (its secret is never written anywhere) runs one
// SetOptions per shape, adding an ed25519 signed-payload signer. stellar-core
// records the new signer in the account entry it writes into the
// transaction meta; the generator later extracts that SignerKey offline from
// the saved LedgerCloseMeta. This is the shape that halted wallet-backend
// ingestion (wallet-backend#729) and was truncated by it (#716).
//
// Shapes: a payload at the CAP-0040 maximum (64 bytes) and at the minimum
// (1 byte, which XDR pads with 3 zero bytes).
//
//   NODE_USE_ENV_PROXY=1 node capture-signers.mjs

import { createHash } from "node:crypto";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import {
  BASE_FEE, Keypair, Networks, Operation, StrKey, TransactionBuilder, rpc, xdr,
} from "@stellar/stellar-sdk";

const RPC_URL = "https://soroban-testnet.stellar.org";
const FRIENDBOT = "https://friendbot.stellar.org";
const PASSPHRASE = Networks.TESTNET;

const here = dirname(fileURLToPath(import.meta.url));
const corpus = join(here, "../corpus");
const outDir = join(corpus, "captured/testnet");
const index = join(corpus, "captured/captures.json");
const sha256 = (b) => createHash("sha256").update(b).digest();

// The ed25519 key inside the signer: fixed, so the shape is the same on
// every run. It never signs anything.
const signerKey = Keypair.fromRawEd25519Seed(sha256("xdr-conformance-signed-payload-signer"));

const SHAPES = [
  { id: "SIGNERKEY-SIGNEDPAYLOAD-MAXLEN-0101", payload: Buffer.from(Array.from({ length: 64 }, (_, i) => i)) },
  { id: "SIGNERKEY-SIGNEDPAYLOAD-MINLEN-0102", payload: Buffer.from([0x2a]) },
];

const server = new rpc.Server(RPC_URL);
const kp = Keypair.random();

async function submit(op) {
  const account = await server.getAccount(kp.publicKey());
  const tx = new TransactionBuilder(account, { fee: BASE_FEE, networkPassphrase: PASSPHRASE })
    .addOperation(op)
    .setTimeout(60)
    .build();
  tx.sign(kp);
  const sent = await server.sendTransaction(tx);
  if (sent.status === "ERROR") throw new Error(`send failed: ${JSON.stringify(sent.errorResult)}`);
  const done = await server.pollTransaction(sent.hash, { attempts: 30 });
  if (done.status !== "SUCCESS") throw new Error(`tx ${sent.hash} ended ${done.status}`);
  return { hash: sent.hash, ledger: done.ledger };
}

async function getLedger(sequence) {
  const res = await fetch(RPC_URL, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      jsonrpc: "2.0", id: 1, method: "getLedgers",
      params: { startLedger: sequence, pagination: { limit: 1 }, xdrFormat: "base64" },
    }),
  });
  const body = await res.json();
  if (body.error) throw new Error(`getLedgers: ${JSON.stringify(body.error)}`);
  const l = body.result.ledgers[0];
  if (!l || l.sequence !== sequence) throw new Error(`getLedgers did not return ledger ${sequence}`);
  return l;
}

console.log(`account ${kp.publicKey()} (throwaway, secret not saved)`);
const fb = await fetch(`${FRIENDBOT}/?addr=${kp.publicKey()}`);
if (!fb.ok) throw new Error(`friendbot ${fb.status}`);

const doc = JSON.parse(readFileSync(index, "utf8"));
mkdirSync(outDir, { recursive: true });

for (const shape of SHAPES) {
  // XDR of SignerKeyEd25519SignedPayload: 32-byte key, 4-byte length,
  // payload padded with zeros to a multiple of 4 (SEP-0023).
  const len = Buffer.alloc(4);
  len.writeUInt32BE(shape.payload.length);
  const pad = Buffer.alloc((4 - (shape.payload.length % 4)) % 4);
  const strkey = StrKey.encodeSignedPayload(Buffer.concat([signerKey.rawPublicKey(), len, shape.payload, pad]));
  const call = await submit(Operation.setOptions({ signer: { ed25519SignedPayload: strkey, weight: 1 } }));
  const ledger = await getLedger(call.ledger);
  const lcm = Buffer.from(ledger.metadataXdr, "base64");
  xdr.LedgerCloseMeta.fromXDR(lcm);
  const file = join(outDir, `ledger-${ledger.sequence}.lcm.xdr`);
  writeFileSync(file, lcm);
  console.log(`${shape.id}: tx ${call.hash} ledger ${ledger.sequence} (${lcm.length} bytes of LedgerCloseMeta)`);
  doc.captures = doc.captures.filter((c) => c.id !== shape.id);
  doc.captures.push({
    id: shape.id,
    extract: "added_signer",
    network_passphrase: PASSPHRASE,
    rpc_url: RPC_URL,
    ledger: ledger.sequence,
    ledger_hash: ledger.hash,
    ledger_close_time: Number(ledger.ledgerCloseTime),
    tx_hash: call.hash,
    ledger_close_meta: relative(corpus, file),
    ledger_close_meta_sha256: sha256(lcm).toString("hex"),
  });
}

doc.captured_with.scripts = ["capture/capture.mjs", "capture/capture-signers.mjs"];
writeFileSync(index, JSON.stringify(doc, null, 2) + "\n");
console.log(`wrote ${SHAPES.length} captures`);
