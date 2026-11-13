// Validates corpus/manifest.json against its JSON Schema, then checks every
// entry against the bytes on disk: the file exists, its length and SHA-256
// match, the stored base64 matches, and IDs are unique.
//
//   node tools/validate-manifest.mjs

import { createHash } from "node:crypto";
import { readFileSync, readdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import Ajv2020 from "ajv/dist/2020.js";
import addFormats from "ajv-formats";

const corpus = join(dirname(fileURLToPath(import.meta.url)), "../corpus");
const schema = JSON.parse(readFileSync(join(corpus, "schema/manifest.schema.json"), "utf8"));
const manifest = JSON.parse(readFileSync(join(corpus, "manifest.json"), "utf8"));

const ajv = new Ajv2020({ allErrors: true, strict: true });
addFormats(ajv);
const validate = ajv.compile(schema);

const errors = [];
if (!validate(manifest)) {
  for (const e of validate.errors) errors.push(`schema: ${e.instancePath || "/"} ${e.message}`);
}

const seen = new Set();
const listed = new Set();
for (const e of manifest.entries ?? []) {
  if (seen.has(e.id)) errors.push(`${e.id}: duplicate id`);
  seen.add(e.id);
  listed.add(e.file);

  if (e.file !== `entries/${e.id}.xdr`) errors.push(`${e.id}: file must be entries/${e.id}.xdr`);
  let bytes;
  try {
    bytes = readFileSync(join(corpus, e.file));
  } catch {
    errors.push(`${e.id}: missing ${e.file}`);
    continue;
  }
  if (bytes.length !== e.byte_length) errors.push(`${e.id}: byte_length ${e.byte_length}, file has ${bytes.length}`);
  if (bytes.length % 4 !== 0) errors.push(`${e.id}: XDR length must be a multiple of 4`);
  const sha = createHash("sha256").update(bytes).digest("hex");
  if (sha !== e.sha256) errors.push(`${e.id}: sha256 mismatch`);
  if (bytes.toString("base64") !== e.expected?.base64) errors.push(`${e.id}: expected.base64 does not match the file`);

  // Captured entries: the saved LedgerCloseMeta they came from must be intact.
  const lcm = e.source?.ledger_close_meta;
  if (lcm) {
    try {
      const raw = readFileSync(join(corpus, lcm.file));
      if (raw.length !== lcm.byte_length) errors.push(`${e.id}: ${lcm.file} has ${raw.length} bytes, manifest says ${lcm.byte_length}`);
      if (createHash("sha256").update(raw).digest("hex") !== lcm.sha256) errors.push(`${e.id}: ${lcm.file} sha256 mismatch`);
    } catch {
      errors.push(`${e.id}: missing ${lcm.file}`);
    }
  }
  if (e.same_bytes_as) {
    const twin = (manifest.entries ?? []).find((x) => x.id === e.same_bytes_as);
    if (!twin) errors.push(`${e.id}: same_bytes_as names unknown entry ${e.same_bytes_as}`);
    else if (twin.sha256 !== e.sha256) errors.push(`${e.id}: same_bytes_as ${e.same_bytes_as} but the bytes differ`);
  }
}

for (const f of readdirSync(join(corpus, "entries"))) {
  if (!listed.has(`entries/${f}`)) errors.push(`entries/${f} is not listed in the manifest`);
}

if (errors.length) {
  for (const e of errors) console.error(e);
  console.error(`\nmanifest invalid: ${errors.length} problem(s)`);
  process.exit(1);
}
console.log(`manifest valid: ${manifest.entries.length} entries, schema and bytes agree`);
