"""Python API-vector runner (stellar-sdk).

Runs corpus/api-vectors.json: cases that exercise the SDK's builder API
rather than its XDR decoder.

    python api.py [--corpus ../../corpus] [--out ../../results]
"""

from __future__ import annotations

import argparse
import json
import platform
from importlib.metadata import version
from pathlib import Path
from typing import Any

from stellar_sdk import Asset, LiquidityPoolAsset, MuxedAccount

HERE = Path(__file__).resolve().parent


def ok() -> dict[str, str]:
    return {"status": "ok", "detail": ""}


def defect(detail: str) -> dict[str, str]:
    return {"status": "defect", "detail": detail}


def liquidity_pool_asset(v: dict[str, Any]) -> dict[str, str]:
    i, want = v["input"], v["expected"]
    try:
        lp = LiquidityPoolAsset(
            Asset(i["asset_a"]["code"], i["asset_a"]["issuer"]),
            Asset(i["asset_b"]["code"], i["asset_b"]["issuer"]),
            i["fee"],
        )
        got = lp.liquidity_pool_id
    except Exception as e:  # noqa: BLE001 - any refusal is a result
        if want["outcome"] == "reject":
            return ok()
        return defect(f"rejected a pair the network accepts: {e}")
    if want["outcome"] == "reject":
        return defect(f"accepted an out-of-order pair as pool {got}")
    if got != want["pool_id"]:
        return defect(f"pool id {got}, expected {want['pool_id']}")
    return ok()


def muxed_account_id(v: dict[str, Any]) -> dict[str, str]:
    """The Python SDK takes a muxed ID as an int, so the vectors that pass a
    non-canonical string have no matching API. Only the control is checked."""
    want = v["expected"]
    if want["outcome"] != "accept":
        return {
            "status": "n/a",
            "detail": "the Python SDK takes a muxed ID as int; there is no string input to test",
        }
    try:
        m = MuxedAccount(v["input"]["account"], int(want["canonical_id"]))
    except Exception as e:  # noqa: BLE001
        return defect(f"rejected a canonical ID: {e}")
    if m.account_muxed != want["address"]:
        return defect(f"address {m.account_muxed}, expected {want['address']}")
    return ok()


KINDS = {"liquidity_pool_asset": liquidity_pool_asset, "muxed_account_id": muxed_account_id}


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--corpus", default=str(HERE / "../../corpus"))
    p.add_argument("--out", default=str(HERE / "../../results"))
    a = p.parse_args()
    corpus, out = Path(a.corpus).resolve(), Path(a.out).resolve()
    vectors = json.loads((corpus / "api-vectors.json").read_text())["vectors"]
    sdk_version = version("stellar-sdk")

    results = []
    print(f"python api stellar-sdk {sdk_version}")
    for v in vectors:
        run = KINDS.get(v["kind"])
        r = run(v) if run else {"status": "n/a", "detail": f"kind {v['kind']} not implemented by this runner"}
        results.append({"id": v["id"], **r})
        print(f"  {v['id']:<32} {r['status']} {r['detail']}")

    out.mkdir(parents=True, exist_ok=True)
    report = {
        "runner": "python",
        "suite": "api",
        "sdk": "stellar-sdk",
        "sdk_version": sdk_version,
        "runtime": f"python {platform.python_version()}",
        "results": results,
    }
    (out / f"python-api-{sdk_version}.json").write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    main()
