// Captures real, core-emitted corpus entries from Stellar testnet.
//
// 1. Funds a fresh throwaway account with friendbot (its secret is never
//    written anywhere).
// 2. Uploads and deploys the keycollide test contract.
// 3. Invokes shape(n) once per corpus shape. The return value is written
//    by stellar-core into the transaction meta of the ledger that closes.
// 4. Fetches that ledger's LedgerCloseMeta with RPC getLedgers and saves
//    the raw bytes plus where they came from in corpus/captured/.
//
// The generator then extracts each return value from the saved
// LedgerCloseMeta offline; nothing in CI touches the network.
//
//   node capture.mjs <path/to/keycollide_contract.wasm>

import { createHash, randomBytes } from "node:crypto";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import {
  BASE_FEE, Contract, Keypair, Networks, Operation, TransactionBuilder, rpc, xdr,
  Address, nativeToScVal,
} from "@stellar/stellar-sdk";

const RPC_URL = "https://soroban-testnet.stellar.org";
const FRIENDBOT = "https://friendbot.stellar.org";
const PASSPHRASE = Networks.TESTNET;
const SHAPES = [1, 2, 3, 4];

const here = dirname(fileURLToPath(import.meta.url));
const corpus = join(here, "../corpus");
const outDir = join(corpus, "captured/testnet");

const wasmPath = process.argv[2];
if (!wasmPath) {
  console.error("usage: node capture.mjs <keycollide_contract.wasm>");
  process.exit(2);
}
const wasm = readFileSync(wasmPath);
const sha256 = (b) => createHash("sha256").update(b).digest("hex");

const server = new rpc.Server(RPC_URL);
const kp = Keypair.random();

async function submit(op) {
  const account = await server.getAccount(kp.publicKey());
  const tx = new TransactionBuilder(account, { fee: BASE_FEE, networkPassphrase: PASSPHRASE })
    .addOperation(op)
    .setTimeout(60)
    .build();
  const prepared = await server.prepareTransaction(tx);
  prepared.sign(kp);
  const sent = await server.sendTransaction(prepared);
  if (sent.status === "ERROR") throw new Error(`send failed: ${JSON.stringify(sent.errorResult)}`);
  const done = await server.pollTransaction(sent.hash, { attempts: 30 });
  if (done.status !== "SUCCESS") throw new Error(`tx ${sent.hash} ended ${done.status}`);
  return { hash: sent.hash, ledger: done.ledger, returnValue: done.returnValue };
}

// Raw JSON-RPC so the saved bytes are exactly what the server returned.
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

const upload = await submit(Operation.uploadContractWasm({ wasm }));
const wasmHash = createHash("sha256").update(wasm).digest();
console.log(`uploaded wasm ${wasmHash.toString("hex")} in ${upload.hash}`);

const created = await submit(Operation.createCustomContract({
  address: new Address(kp.publicKey()),
  wasmHash,
  salt: randomBytes(32),
}));
const contractId = Address.fromScVal(created.returnValue).toString();
console.log(`contract ${contractId} in ${created.hash}`);

mkdirSync(outDir, { recursive: true });
const captures = [];
for (const n of SHAPES) {
  const call = await submit(new Contract(contractId).call("shape", nativeToScVal(n, { type: "u32" })));
  const ledger = await getLedger(call.ledger);
  const lcm = Buffer.from(ledger.metadataXdr, "base64");
  // Sanity check before saving: the bytes decode as LedgerCloseMeta.
  xdr.LedgerCloseMeta.fromXDR(lcm);
  const file = join(outDir, `ledger-${ledger.sequence}.lcm.xdr`);
  writeFileSync(file, lcm);
  console.log(`shape ${n}: tx ${call.hash} ledger ${ledger.sequence} (${lcm.length} bytes of LedgerCloseMeta)`);
  captures.push({
    id: `SCVAL-MAP-KEYCOLLIDE-${String(100 + n).padStart(4, "0")}`,
    shape_of: `SCVAL-MAP-KEYCOLLIDE-${String(n).padStart(4, "0")}`,
    network_passphrase: PASSPHRASE,
    rpc_url: RPC_URL,
    ledger: ledger.sequence,
    ledger_hash: ledger.hash,
    ledger_close_time: Number(ledger.ledgerCloseTime),
    tx_hash: call.hash,
    contract_id: contractId,
    wasm_sha256: wasmHash.toString("hex"),
    ledger_close_meta: relative(corpus, file),
    ledger_close_meta_sha256: sha256(lcm),
  });
}

writeFileSync(join(corpus, "captured/captures.json"), JSON.stringify({
  description: "Real testnet captures. Each LedgerCloseMeta was returned by RPC getLedgers for the ledger that included the transaction; the generator extracts the contract return value from it offline.",
  captured_with: { script: "capture/capture.mjs", sdk: "@stellar/stellar-sdk 17.4.0" },
  captures,
}, null, 2) + "\n");
console.log(`wrote ${captures.length} captures`);
