// Captures a replay scene for wallet-backend#677 from Stellar testnet.
//
// Account S creates a claimable balance for account C. S pays the entry's
// reserve, so S is its sponsor (CAP-0033). C then claims it. The claim names
// only C (source) and the balance ID, yet stellar-core changes S's account
// entry in the operation meta, because S's numSponsoring drops when the
// sponsored entry is removed. An indexer that derives participants from the
// meta records S; one that reads only the envelope, or reads the meta
// incorrectly, drops S.
//
// Both accounts are throwaway; their secrets are never written anywhere.
//
//   NODE_USE_ENV_PROXY=1 node capture-sponsor.mjs

import { createHash } from "node:crypto";
import { mkdirSync, readFileSync, writeFileSync, existsSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import {
  Asset, BASE_FEE, Claimant, Keypair, Networks, Operation, TransactionBuilder, rpc, xdr,
} from "@stellar/stellar-sdk";

const RPC_URL = "https://soroban-testnet.stellar.org";
const FRIENDBOT = "https://friendbot.stellar.org";
const PASSPHRASE = Networks.TESTNET;
const SCENE_ID = "SCENE-SPONSORED-CB-CLAIM-0001";

const here = dirname(fileURLToPath(import.meta.url));
const corpus = join(here, "../corpus");
const outDir = join(corpus, "captured/testnet");
const index = join(corpus, "captured/scenes.json");
const sha256 = (b) => createHash("sha256").update(b).digest("hex");

const server = new rpc.Server(RPC_URL);

async function fund(kp) {
  const r = await fetch(`${FRIENDBOT}/?addr=${kp.publicKey()}`);
  if (!r.ok) throw new Error(`friendbot ${r.status}`);
}

async function submit(kp, op) {
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
  return { tx, hash: sent.hash, ledger: done.ledger };
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

const sponsor = Keypair.random();
const claimant = Keypair.random();
console.log(`sponsor ${sponsor.publicKey()}, claimant ${claimant.publicKey()} (throwaway, secrets not saved)`);
await fund(sponsor);
await fund(claimant);

const created = await submit(sponsor, Operation.createClaimableBalance({
  asset: Asset.native(),
  amount: "1",
  claimants: [new Claimant(claimant.publicKey(), Claimant.predicateUnconditional())],
}));
const balanceId = created.tx.getClaimableBalanceId(0);
console.log(`created claimable balance ${balanceId} in ${created.hash}`);

const claim = await submit(claimant, Operation.claimClaimableBalance({ balanceId }));
const ledger = await getLedger(claim.ledger);
const lcm = Buffer.from(ledger.metadataXdr, "base64");
xdr.LedgerCloseMeta.fromXDR(lcm);
mkdirSync(outDir, { recursive: true });
const file = join(outDir, `ledger-${ledger.sequence}.lcm.xdr`);
writeFileSync(file, lcm);
console.log(`claim tx ${claim.hash} ledger ${ledger.sequence} (${lcm.length} bytes of LedgerCloseMeta)`);

const doc = existsSync(index)
  ? JSON.parse(readFileSync(index, "utf8"))
  : {
      description: "Real testnet scenes for the replay harness. Each LedgerCloseMeta was returned by RPC getLedgers for the ledger that included the transaction; the generator derives the expected participants from it offline.",
      captured_with: { script: "capture/capture-sponsor.mjs", sdk: "@stellar/stellar-sdk 17.4.0" },
      scenes: [],
    };
doc.scenes = doc.scenes.filter((s) => s.id !== SCENE_ID);
doc.scenes.push({
  id: SCENE_ID,
  network_passphrase: PASSPHRASE,
  rpc_url: RPC_URL,
  ledger: ledger.sequence,
  ledger_hash: ledger.hash,
  ledger_close_time: Number(ledger.ledgerCloseTime),
  tx_hash: claim.hash,
  roles: { sponsor: sponsor.publicKey(), claimant: claimant.publicKey() },
  setup_tx_hash: created.hash,
  claimable_balance_id: balanceId,
  ledger_close_meta: relative(corpus, file),
  ledger_close_meta_sha256: sha256(lcm),
});
writeFileSync(index, JSON.stringify(doc, null, 2) + "\n");
console.log(`wrote scene ${SCENE_ID}`);
