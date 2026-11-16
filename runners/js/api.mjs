// JavaScript API-vector runner.
//
// Runs corpus/api-vectors.json through each pinned @stellar/stellar-sdk
// version: cases that exercise the SDK's builder and parser API rather than
// its XDR decoder. Writes one result file per version.
//
//   node api.mjs [--corpus ../../corpus] [--out ../../results]

import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const require = createRequire(import.meta.url);

function arg(name, fallback) {
  const i = process.argv.indexOf(name);
  return i > -1 ? process.argv[i + 1] : fallback;
}

const corpusDir = resolve(arg("--corpus", join(here, "../../corpus")));
const outDir = resolve(arg("--out", join(here, "../../results")));
const { vectors } = JSON.parse(readFileSync(join(corpusDir, "api-vectors.json"), "utf8"));

const pkg = JSON.parse(readFileSync(join(here, "package.json"), "utf8"));
const aliases = Object.keys(pkg.dependencies).filter((d) => d.startsWith("sdk-"));

const ok = () => ({ status: "ok", detail: "" });
const defect = (detail) => ({ status: "defect", detail });
const short = (e) => String(e?.message ?? e).split("\n")[0].slice(0, 120);

function liquidityPoolAsset(sdk, v) {
  const { asset_a: a, asset_b: b, fee } = v.input;
  let id;
  try {
    const lp = new sdk.LiquidityPoolAsset(new sdk.Asset(a.code, a.issuer), new sdk.Asset(b.code, b.issuer), fee);
    id = Buffer.from(sdk.getLiquidityPoolId("constant_product", lp.getLiquidityPoolParameters())).toString("hex");
  } catch (e) {
    return v.expected.outcome === "reject"
      ? ok()
      : defect(`rejected a pair the network accepts: ${short(e)}`);
  }
  if (v.expected.outcome === "reject") return defect(`accepted an out-of-order pair as pool ${id}`);
  return id === v.expected.pool_id ? ok() : defect(`pool id ${id}, expected ${v.expected.pool_id}`);
}

function muxedAccountId(sdk, v) {
  const { account, id } = v.input;
  const want = v.expected;
  let m;
  try {
    m = new sdk.MuxedAccount(new sdk.Account(account, "1"), id);
  } catch (e) {
    return want.outcome === "accept" ? defect(`rejected a canonical ID: ${short(e)}`) : ok();
  }
  if (want.outcome === "reject") {
    return defect(`accepted ${JSON.stringify(id)} as ID ${JSON.stringify(m.id())}, address ${m.accountId()}`);
  }
  if (m.accountId() !== want.address) return defect(`address ${m.accountId()}, expected ${want.address}`);
  if (m.id() !== want.canonical_id) {
    return defect(`accepted ${JSON.stringify(id)}: the address encodes ID ${want.canonical_id} but id() reports ${JSON.stringify(m.id())}`);
  }
  return ok();
}

const KINDS = { liquidity_pool_asset: liquidityPoolAsset, muxed_account_id: muxedAccountId };

mkdirSync(outDir, { recursive: true });
for (const alias of aliases) {
  const sdk = require(alias);
  const version = JSON.parse(readFileSync(join(here, "node_modules", alias, "package.json"), "utf8")).version;
  const results = vectors.map((v) => {
    const run = KINDS[v.kind];
    let r;
    try {
      r = run ? run(sdk, v) : { status: "n/a", detail: `kind ${v.kind} not implemented by this runner` };
    } catch (e) {
      r = defect(`runner error: ${short(e)}`);
    }
    return { id: v.id, ...r };
  });
  const out = {
    runner: "js",
    suite: "api",
    sdk: "@stellar/stellar-sdk",
    sdk_version: version,
    runtime: `node ${process.versions.node}`,
    results,
  };
  writeFileSync(join(outDir, `js-api-${version}.json`), JSON.stringify(out, null, 2) + "\n");
  console.log(`js api @stellar/stellar-sdk ${version}`);
  for (const r of results) console.log(`  ${r.id.padEnd(32)} ${r.status}${r.detail ? `: ${r.detail}` : ""}`);
}
