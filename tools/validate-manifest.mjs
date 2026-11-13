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
