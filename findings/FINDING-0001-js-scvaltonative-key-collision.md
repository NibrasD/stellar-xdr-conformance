# FINDING-0001: `scValToNative` silently drops colliding map keys (JavaScript)

| Field | Value |
| --- | --- |
| Component | `@stellar/stellar-sdk` (JavaScript), `scValToNative` |
| Upstream issue | [stellar/js-stellar-sdk#1789](https://github.com/stellar/js-stellar-sdk/issues/1789) (fix: PR #1778) |
| Affected | 17.2.1 and earlier |
| First fixed | 17.3.0 (confirmed by reading the published package source) |
| Severity | Medium: silent data loss in decoded contract values, events and invocation trees |
| CAP | CAP-0046-01 (`SCMap`, protocol 20) |
| Corpus entries | `SCVAL-MAP-KEYCOLLIDE-0001`, `-0002`, `-0004`, and their testnet-captured twins `-0101`, `-0102`, `-0104` |
| Status | Reproduced on the pre-fix pin and verified fixed on 17.4.0 |

## What happens

`scValToNative` turns an `scvMap` into a plain JavaScript object with
`Object.fromEntries`. Object keys are strings, so two distinct XDR keys
whose native forms print the same become one property. The last entry wins
and the other is dropped without an error. The upstream issue notes that
`buildInvocationTree()` and `humanizeEvents()` call the same function.

## Reproduction

```bash
cd runners/js && npm ci && node run.mjs
```

Observed on 17.2.1:

| Entry | XDR keys | Result |
| --- | --- | --- |
| `SCVAL-MAP-KEYCOLLIDE-0001` | `u32(1)`, `symbol("1")` | `{"1":20}`: one of two entries |
| `SCVAL-MAP-KEYCOLLIDE-0002` | `string("k")`, `symbol("k")` | `{"k":20}`: one of two entries |
| `SCVAL-MAP-KEYCOLLIDE-0004` | `u32(1)`, `i64(1)` | `{"1":20}`: one of two entries |

Observed on 17.4.0: all three raise
`TypeError: scvMap has more than one key that converts to "<key>"; read the raw map entries instead`.

Decoding, byte-exact re-encoding and xdr-json output are correct on both
versions. The defect is only in the conversion to native values.

## Notes

- `SCVAL-MAP-KEYCOLLIDE-0003` (`bool(true)` and `u32(1)`) does not collide in
  JavaScript: the keys become `"true"` and `"1"`. It does collide in Python
  (see [FINDING-0002](FINDING-0002-python-to-native-key-collision.md)). The
  same XDR fails differently in each SDK, which is why each SDK needs the
  whole corpus rather than a port of one SDK's test.
- The fix chose to reject such maps. Callers that relied on the old
  behavior now get an exception instead of partial data.
- `expectations/known-outcomes.json` keeps the 17.2.1 pin failing on
  purpose. If that pin ever stops reproducing the bug, CI fails, because
  the harness would no longer prove it can detect it.
