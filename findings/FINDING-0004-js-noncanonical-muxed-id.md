# FINDING-0004: non-canonical muxed account IDs accepted (JavaScript)

| Field | Value |
| --- | --- |
| Component | `@stellar/stellar-sdk` (JavaScript), `MuxedAccount` |
| Upstream issue | [stellar/js-stellar-sdk#1785](https://github.com/stellar/js-stellar-sdk/issues/1785), closed 2026-10-08 (fix: PR #1780) |
| Affected | 17.2.0 and 17.2.1 (observed) |
| First fixed | 17.3.0 (confirmed by reading the published package source); 17.4.0 observed |
| Severity | Medium: two objects for the same `M...` address can report different IDs, and an empty string becomes ID 0 |
| CAP | CAP-0027 (muxed accounts) |
| API vectors | `APIVEC-MUXED-ID-0002` to `-0005` (`-0001` is the control) |
| Status | Reproduced on the pre-fix pins; verified fixed on 17.4.0 |

## What happens

The SDK parsed a muxed ID with `BigInt()`, which also accepts `"0x10"`,
`"016"` and `" 16 "`. The `M...` address is built from the parsed value, 16,
but `id()` returns the string as it was typed. So the address encodes one ID
while the object reports another. An empty string is accepted as ID 0.

The expected `M...` address for ID 16 is computed with rs-stellar-xdr.

## Reproduction

```bash
cd runners/js && npm ci && node api.mjs
```

| Vector | Input ID | 17.2.0, 17.2.1 | 17.4.0 |
| --- | --- | --- | --- |
| `-0001` | `"16"` | accepted, canonical | accepted, canonical |
| `-0002` | `"0x10"` | accepted; address encodes 16, `id()` reports `"0x10"` | rejected |
| `-0003` | `"016"` | accepted; address encodes 16, `id()` reports `"016"` | accepted and normalized to `"16"` |
| `-0004` | `" 16 "` | accepted; address encodes 16, `id()` reports `" 16 "` | rejected |
| `-0005` | `""` | accepted as ID 0 | rejected |

## Notes

- The upstream issue also covers offer IDs (`manageSellOffer`,
  `manageBuyOffer`): values above `Number.MAX_SAFE_INTEGER` and negative
  IDs. Those are not covered by these vectors yet.
