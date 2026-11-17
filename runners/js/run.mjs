// JavaScript conformance runner.
//
// Runs every corpus entry through each pinned @stellar/stellar-sdk version
// listed in package.json and writes one result file per version.
//
//   node run.mjs [--corpus ../../corpus] [--out ../../results]

import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { isDeepStrictEqual } from "node:util";

const here = dirname(fileURLToPath(import.meta.url));
const require = createRequire(import.meta.url);

function arg(name, fallback) {
  const i = process.argv.indexOf(name);
  return i > -1 ? process.argv[i + 1] : fallback;
}

const corpusDir = resolve(arg("--corpus", join(here, "../../corpus")));
const outDir = resolve(arg("--out", join(here, "../../results")));
const manifest = JSON.parse(readFileSync(join(corpusDir, "manifest.json"), "utf8"));

// Every dependency aliased as "sdk-<version>" is one SDK under test.
const pkg = JSON.parse(readFileSync(join(here, "package.json"), "utf8"));
const aliases = Object.keys(pkg.dependencies).filter((d) => d.startsWith("sdk-"));

// Language-neutral form of a decoded ScVal (see corpus/schema). The SDK 17
// line decodes with js-xdr 5, where a union is an object with a `type`
// discriminant and named arm fields.
function typed(v) {
  switch (v.type) {
    case "scvBool":   return { type: "bool", value: String(v.b) };
    case "scvU32":    return { type: "u32", value: String(v.u32) };
    case "scvI32":    return { type: "i32", value: String(v.i32) };
    case "scvU64":    return { type: "u64", value: String(v.u64) };
    case "scvI64":    return { type: "i64", value: String(v.i64) };
    case "scvSymbol": return { type: "symbol", value: String(v.sym) };
    case "scvString": return { type: "string", value: String(v.str) };
    case "scvMap":
      return {
        type: "map",
        entries: (v.map ?? []).map((e) => ({ key: typed(e.key), val: typed(e.val) })),
      };
    default:
      throw new Error(`typed(): no language-neutral form for ${v.type}`);
  }
}

function nativeSize(native) {
  if (native instanceof Map) return native.size;
  if (native && typeof native === "object") return Object.keys(native).length;
  return null;
}

function checkEntry(sdk, entry) {
  const { xdr, scValToNative } = sdk;
  const bytes = readFileSync(join(corpusDir, entry.file));
  const r = { id: entry.id, decode: "fail", roundtrip: "fail", structure: "fail", xdr_json: "n/a", native: "n/a", detail: "" };

  if (entry.xdr_type === "SignerKey") return checkSignerKey(sdk, entry, bytes, r);

  let val;
  try {
    val = xdr.ScVal.fromXDR(bytes);
    r.decode = "pass";
  } catch (e) {
    r.detail = `decode: ${e.message}`;
    return r;
  }

  r.roundtrip = Buffer.compare(val.toXDR(), bytes) === 0 ? "pass" : "fail";

  try {
    r.structure = isDeepStrictEqual(typed(val), entry.expected.decoded) ? "pass" : "fail";
  } catch (e) {
    r.detail = `structure: ${e.message}`;
  }

  // xdr-json agreement with rs-stellar-xdr, where the SDK exposes it.
  if (entry.expected.xdr_json !== undefined && typeof val.toJSON === "function") {
    const mine = JSON.parse(JSON.stringify(val.toJSON()));
    r.xdr_json = isDeepStrictEqual(mine, entry.expected.xdr_json) ? "pass" : "fail";
    if (r.xdr_json === "fail") r.detail = `xdr_json: got ${JSON.stringify(mine)}`;
  }

  if (entry.native_conversion) {
    const want = entry.native_conversion.distinct_keys;
    try {
      const native = scValToNative(val);
      const got = nativeSize(native);
      if (got === want) r.native = "preserved";
      else if (got !== null && got < want) {
        r.native = "silent-loss";
        r.detail = `scValToNative returned ${got} of ${want} entries without an error: ${JSON.stringify(native, (_, x) => (typeof x === "bigint" ? `${x}n` : x))}`;
      } else {
        r.native = "unexpected";
        r.detail = `scValToNative returned ${got} entries, expected ${want}`;
      }
    } catch (e) {
      r.native = "rejected";
      r.detail = `scValToNative threw ${e.constructor.name}: ${e.message}`;
    }
  }
  return r;
}

// A SignerKey's language-neutral form is its strkey (SEP-0023), produced
// here by the SDK's own encoder.
function checkSignerKey(sdk, entry, bytes, r) {
  let key;
  try {
    key = sdk.xdr.SignerKey.fromXDR(bytes);
    r.decode = "pass";
  } catch (e) {
    r.detail = `decode: ${e.message}`;
    return r;
  }
  r.roundtrip = Buffer.compare(key.toXDR(), bytes) === 0 ? "pass" : "fail";
  try {
    const got = { type: "signer_key", value: sdk.SignerKey.encodeSignerKey(key) };
    r.structure = isDeepStrictEqual(got, entry.expected.decoded) ? "pass" : "fail";
    if (r.structure === "fail") r.detail = `structure: got ${JSON.stringify(got)}`;
  } catch (e) {
    r.detail = `structure: ${e.message}`;
  }
  if (entry.expected.xdr_json !== undefined && typeof key.toJSON === "function") {
    const mine = JSON.parse(JSON.stringify(key.toJSON()));
    r.xdr_json = isDeepStrictEqual(mine, entry.expected.xdr_json) ? "pass" : "fail";
    if (r.xdr_json === "fail") r.detail = `xdr_json: got ${JSON.stringify(mine)}`;
  }
  return r;
}

mkdirSync(outDir, { recursive: true });
for (const alias of aliases) {
  const sdk = require(alias);
  const version = JSON.parse(readFileSync(join(here, "node_modules", alias, "package.json"), "utf8")).version;
  const results = manifest.entries.map((e) => checkEntry(sdk, e));
  const out = {
    runner: "js",
    sdk: "@stellar/stellar-sdk",
    sdk_version: version,
    native_function: "scValToNative",
    runtime: `node ${process.versions.node}`,
    results,
  };
  const file = join(outDir, `js-${version}.json`);
  writeFileSync(file, JSON.stringify(out, null, 2) + "\n");
  console.log(`js @stellar/stellar-sdk ${version}`);
  for (const r of results) console.log(`  ${r.id.padEnd(28)} decode=${r.decode} roundtrip=${r.roundtrip} structure=${r.structure} xdr_json=${r.xdr_json} native=${r.native}`);
}
