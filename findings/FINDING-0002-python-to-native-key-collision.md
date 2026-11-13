# FINDING-0002: `scval.to_native` silently drops colliding map keys (Python)

| Field | Value |
| --- | --- |
| Component | `stellar-sdk` (Python), `stellar_sdk.scval.to_native` |
| Upstream issue | [StellarCN/py-stellar-base#1250](https://github.com/StellarCN/py-stellar-base/issues/1250), opened 2026-11-13 |
| Upstream fix | PR #1252, open and not merged as of 2026-11-13 |
| Affected | 17.0.1 (latest release at the time of writing) |
| Severity | Medium: silent data loss, and in one case a value attached to the wrong key |
| CAP | CAP-0046-01 (`SCMap`, protocol 20) |
| Corpus entries | `SCVAL-MAP-KEYCOLLIDE-0002`, `-0003`, `-0004` |
| Status | Independently confirmed at runtime by this harness; reported upstream by others first |

This is the Python sibling of
[js-stellar-sdk#1789](https://github.com/stellar/js-stellar-sdk/issues/1789).
The harness found it by running the JavaScript corpus entries through the
Python SDK unchanged. It was already reported upstream the same day, so this
file records an independent confirmation, not a new report.

## What happens

The `SCV_MAP` branch of `to_native` builds a Python `dict` with a
comprehension:

```python
return {
    to_native(entry.key): to_native(entry.val) for entry in sc_val.map.sc_map
}
```

Distinct XDR keys that become equal Python keys collapse into one entry,
with no error.

## Reproduction

```bash
cd runners/python
python3 -m venv .venv && .venv/bin/pip install -r requirements.txt
.venv/bin/python run.py
```

Observed on 17.0.1:

| Entry | XDR keys | Result |
| --- | --- | --- |
| `SCVAL-MAP-KEYCOLLIDE-0001` | `u32(1)`, `symbol("1")` | Both kept: `1` and `"1"` are different Python keys |
| `SCVAL-MAP-KEYCOLLIDE-0002` | `string("k")`, `symbol("k")` | `{'k': 20}`: one of two entries |
| `SCVAL-MAP-KEYCOLLIDE-0003` | `bool(true)`, `u32(1)` | `{True: 20}`: one of two entries |
| `SCVAL-MAP-KEYCOLLIDE-0004` | `u32(1)`, `i64(1)` | `{1: 20}`: one of two entries |

## Notes

- **Entry 0003 is worse than a lost entry.** In Python `True == 1`, and a
  dict keeps the first key it saw but the last value. The result `{True: 20}`
  pairs the `bool` key with the value that belonged to the `u32` key, so the
  data is not only incomplete but wrong. JavaScript does not have this case.
  The upstream report already lists it; the harness reproduced it
  independently.
- The upstream issue also names `from_map` and `from_struct`. This harness
  tests only `to_native` so far.
- This harness has not checked whether PR #1252 covers the `bool`/`int` case.
  When a release with the fix ships, bumping the pin in
  `runners/python/requirements.txt` will show it: CI fails until
  `expectations/known-outcomes.json` matches the new behavior.
