# Corpus manifest

`corpus/manifest.json` lists every corpus entry. Its JSON Schema is
[`corpus/schema/manifest.schema.json`](../corpus/schema/manifest.schema.json).
The generator writes the manifest; do not edit it by hand.

The corpus is language-neutral: raw XDR files plus this manifest. Any
pipeline or SDK can read it without depending on code in this repository.

## Entry fields

| Field | Meaning |
| --- | --- |
| `id` | Stable ID, `<XDR TYPE>-<FAMILY>-<SHAPE>-<NNNN>`. Never reused, even if an entry is removed. |
| `title`, `description` | What the shape is and why it is in the corpus. |
| `xdr_type` | Top-level XDR type of the file, named as in stellar-xdr (`ScVal`, `TransactionEnvelope`, `LedgerCloseMeta`, ...). |
| `file` | `entries/<id>.xdr`, relative to `corpus/`. |
| `encoding` | `raw`: the file holds the XDR bytes themselves. |
| `byte_length`, `sha256` | Integrity of the file. Checked by `tools/validate-manifest.mjs`. |
| `cap` | CAPs that introduced or changed the shape, such as `CAP-0046-01`. |
| `protocol_version.introduced` | First protocol version in which the shape can occur on the network. |
| `synthetic` | `true` when the bytes were built rather than captured from network history or emitted by a local stellar-core. |
| `source.kind` | `synthetic`, `testnet`, `mainnet` or `local-core`. A non-synthetic entry must also give `network_passphrase` and `ledger`. |
| `source.method` | How the bytes were produced or captured, precise enough to repeat. |
| `related_issues` | Upstream issues the entry reproduces or guards against. |
| `expected.decoded` | The decoded value in a language-neutral typed form (see below). Every runner compares its decode against this. |
| `expected.xdr_json` | The value as rs-stellar-xdr serializes it to JSON. Runners compare against it where their SDK exposes xdr-json. |
| `expected.base64` | The file's bytes in base64, for copying into a test by hand. |
| `native_conversion` | For maps: the rule an SDK's native conversion must follow, and how many distinct keys the map has. |

### Typed values

`expected.decoded` uses one form every language can produce: the XDR type
name and the value as a string. Strings avoid precision loss for 64-bit and
larger integers.

```json
{ "type": "map", "entries": [
  { "key": { "type": "u32", "value": "1" }, "val": { "type": "u32", "value": "10" } },
  { "key": { "type": "symbol", "value": "1" }, "val": { "type": "u32", "value": "20" } }
] }
```

### Native-conversion rule

`preserve-or-reject`: converting to native values must keep every entry, or
raise an error. Returning fewer entries than `distinct_keys` without an
error is a defect.

## Adding an entry

1. Add a `Spec` to `specs()` in `generator/src/main.rs`. Use a new ID and
   list the CAP and protocol version that introduced the shape.
2. Run `cargo run -p generator`. It encodes the value, checks that it
   decodes and re-encodes to the same bytes, and rewrites the manifest.
3. Run `./scripts/run-all.sh`. If an SDK diverges, either file the defect
   upstream and record the outcome in `expectations/known-outcomes.json`
   with a link, or fix the runner.
4. Commit the generator change, the new `.xdr` file, the manifest and the
   regenerated `REPORT.md` together.

## Synthetic and captured entries

The entries in this first release are synthetic: built with
`ScMap::sorted_from`, which sorts keys and rejects duplicates the same way
the Soroban host does, so every map is one the network would accept. The
manifest marks them `synthetic: true`. Entries captured from testnet,
mainnet or a local stellar-core will carry the network passphrase, ledger
sequence and transaction hash, so anyone can fetch the same bytes again.
