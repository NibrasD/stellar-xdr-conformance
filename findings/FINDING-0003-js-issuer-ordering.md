# FINDING-0003: asset issuers ordered as strkey text, not as key bytes (JavaScript)

| Field | Value |
| --- | --- |
| Component | `@stellar/stellar-sdk` (JavaScript), `Asset.compare`, `LiquidityPoolAsset`, `getLiquidityPoolId` |
| Upstream fix | [stellar/js-stellar-sdk#1766](https://github.com/stellar/js-stellar-sdk/pull/1766), merged 2026-10-01 |
| Affected | 17.2.0 (observed) |
| First fixed | 17.2.1 (observed) |
| Severity | High: a valid liquidity-pool asset pair is rejected, and the reversed pair is accepted with a pool ID the network does not use |
| CAP | CAP-0038 (liquidity pools) |
| API vectors | `APIVEC-LPOOL-ISSUER-ORDER-0001`, `-0002` |
| Status | Reproduced on the pre-fix pin 17.2.0; verified fixed on 17.2.1 and 17.4.0 |

## What happens

The network orders the two assets of a liquidity pool by their XDR value, so
two assets with the same code are ordered by the issuer's raw key bytes. The
SDK ordered them by the issuer's `G...` strkey text. In base32 the digits
`2`-`7` stand for the values 26-31 but sort before the letters in ASCII, so
the two orders can disagree.

The vectors use two issuers for which they do disagree. The generator
asserts this with rs-stellar-xdr, and computes the expected pool ID as
`sha256(LiquidityPoolParameters)` from the pair in network order.

## Reproduction

```bash
cd runners/js && npm ci && node api.mjs
```

| Vector | Input | 17.2.0 | 17.2.1, 17.4.0 |
| --- | --- | --- | --- |
| `-0001` | pair in network order | rejected: "Assets are not in lexicographic order" | accepted, pool ID equals the rs-stellar-xdr value |
| `-0002` | the same pair reversed | accepted, with a different pool ID | rejected |

## Notes

- The upstream fix also changed how ScVal map keys are sorted
  (`scvSortedMap`). These vectors cover the issuer-ordering half only.
- The RFP notes that the Go SDK had already fixed the same bug. A Go runner
  for API vectors is not written yet, so that is not checked here.
