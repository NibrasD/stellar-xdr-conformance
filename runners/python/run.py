"""Python conformance runner (stellar-sdk).

Runs every corpus entry through the pinned stellar-sdk release and writes
one result file.

    python run.py [--corpus ../../corpus] [--out ../../results]
"""

from __future__ import annotations

import argparse
import json
from importlib.metadata import version
from pathlib import Path
import platform
from typing import Any

from stellar_sdk import scval, xdr

HERE = Path(__file__).resolve().parent


def typed(v: xdr.SCVal) -> dict[str, Any]:
    """Language-neutral form of a decoded value (see corpus/schema)."""
    t = v.type
    if t == xdr.SCValType.SCV_BOOL:
        return {"type": "bool", "value": "true" if v.b else "false"}
    if t == xdr.SCValType.SCV_U32:
        return {"type": "u32", "value": str(v.u32.uint32)}
    if t == xdr.SCValType.SCV_I32:
        return {"type": "i32", "value": str(v.i32.int32)}
    if t == xdr.SCValType.SCV_U64:
        return {"type": "u64", "value": str(v.u64.uint64)}
    if t == xdr.SCValType.SCV_I64:
        return {"type": "i64", "value": str(v.i64.int64)}
    if t == xdr.SCValType.SCV_SYMBOL:
        return {"type": "symbol", "value": v.sym.sc_symbol.decode("utf-8")}
    if t == xdr.SCValType.SCV_STRING:
        return {"type": "string", "value": v.str.sc_string.decode("utf-8")}
    if t == xdr.SCValType.SCV_MAP:
        entries = v.map.sc_map if v.map is not None else []
        return {
            "type": "map",
            "entries": [{"key": typed(e.key), "val": typed(e.val)} for e in entries],
        }
    raise ValueError(f"no language-neutral form for {t}")


def check(corpus: Path, entry: dict[str, Any]) -> dict[str, Any]:
    r = {
        "id": entry["id"],
        "decode": "fail",
        "roundtrip": "fail",
        "structure": "fail",
        "xdr_json": "n/a",
        "native": "n/a",
        "detail": "",
    }
    raw = (corpus / entry["file"]).read_bytes()
    try:
        v = xdr.SCVal.from_xdr_bytes(raw)
    except Exception as e:  # noqa: BLE001 - any decode failure is a result
        r["detail"] = f"decode: {e!r}"
        return r
    r["decode"] = "pass"

    if v.to_xdr_bytes() == raw:
        r["roundtrip"] = "pass"

    try:
        got = typed(v)
        if got == entry["expected"]["decoded"]:
            r["structure"] = "pass"
        else:
            r["detail"] = f"structure: got {json.dumps(got)}"
    except Exception as e:  # noqa: BLE001
        r["detail"] = f"structure: {e!r}"

    if "xdr_json" in entry["expected"]:
        mine = json.loads(json.dumps(v.to_json_dict()))
        r["xdr_json"] = "pass" if mine == entry["expected"]["xdr_json"] else "fail"
        if r["xdr_json"] == "fail":
            r["detail"] = f"xdr_json: got {json.dumps(mine)}"

    nc = entry.get("native_conversion")
    if nc:
        want = nc["distinct_keys"]
        try:
            native = scval.to_native(v)
        except Exception as e:  # noqa: BLE001
            r["native"] = "rejected"
            r["detail"] = f"to_native raised {type(e).__name__}: {e}"
        else:
            got_n = len(native) if isinstance(native, dict) else None
            if got_n == want:
                r["native"] = "preserved"
            elif got_n is not None and got_n < want:
                r["native"] = "silent-loss"
                r["detail"] = (
                    f"scval.to_native returned {got_n} of {want} entries without an error: {native!r}"
                )
            else:
                r["native"] = "unexpected"
                r["detail"] = f"scval.to_native returned {native!r}, expected {want} entries"
    return r


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--corpus", default=str(HERE / "../../corpus"))
    p.add_argument("--out", default=str(HERE / "../../results"))
    a = p.parse_args()

    corpus = Path(a.corpus).resolve()
    out = Path(a.out).resolve()
    manifest = json.loads((corpus / "manifest.json").read_text())
    sdk_version = version("stellar-sdk")

    results = [check(corpus, e) for e in manifest["entries"]]
    print(f"python stellar-sdk {sdk_version}")
    for r in results:
        print(
            f"  {r['id']:<28} decode={r['decode']} roundtrip={r['roundtrip']} "
            f"structure={r['structure']} xdr_json={r['xdr_json']} native={r['native']}"
        )

    out.mkdir(parents=True, exist_ok=True)
    report = {
        "runner": "python",
        "sdk": "stellar-sdk",
        "sdk_version": sdk_version,
        "native_function": "scval.to_native",
        "runtime": f"python {platform.python_version()}",
        "results": results,
    }
    (out / f"python-{sdk_version}.json").write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    main()
