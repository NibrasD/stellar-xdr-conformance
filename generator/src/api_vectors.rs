//! API vectors: cases that exercise an SDK's builder or parser API rather
//! than its XDR decoder. Some defects only show on the encode side: the SDK
//! refuses a value the network accepts, or accepts an input that names a
//! different value than the one it then encodes.
//!
//! Every expected value here is computed with rs-stellar-xdr, independently
//! of the SDKs under test, and written to `corpus/api-vectors.json`.

use std::str::FromStr;

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use stellar_xdr::{
    AccountId, AlphaNum4, Asset, AssetCode4, Limits, LiquidityPoolConstantProductParameters,
    LiquidityPoolParameters, MuxedAccount, MuxedAccountMed25519, PublicKey, WriteXdr,
};

const JS_1766: &str = "https://github.com/stellar/js-stellar-sdk/pull/1766";
const JS_1785: &str = "https://github.com/stellar/js-stellar-sdk/issues/1785";

/// Two issuers whose order by raw key bytes (the network's order) is the
/// opposite of their order as strkey text. Derived from fixed seeds
/// sha256("xdr-conformance-issuer-<n>"); these are the first such pair.
const ISSUER_LOW_BYTES: &str = "GCVX2X5XG35YWAAPF6KN4CP77QXXTYUDNTYMKMJU4CRH5MHTDFJKUTRL";
const ISSUER_HIGH_BYTES: &str = "GC5Q7OZUWPM2AHV6K4KT37VXGO2HLGQZOP56XRGWF26JP4NLNSMSE22I";

const LP_FEE: i32 = 30;
const MUXED_ID: u64 = 16;

fn usd(issuer: &str) -> Asset {
    Asset::CreditAlphanum4(AlphaNum4 {
        asset_code: AssetCode4(*b"USD\0"),
        issuer: AccountId::from_str(issuer).expect("valid G address"),
    })
}

fn pool_id(asset_a: Asset, asset_b: Asset) -> String {
    let params = LiquidityPoolParameters::LiquidityPoolConstantProduct(
        LiquidityPoolConstantProductParameters {
            asset_a,
            asset_b,
            fee: LP_FEE,
        },
    );
    hex::encode(Sha256::digest(
        params.to_xdr(Limits::none()).expect("encode"),
    ))
}

fn muxed_address(account: &str, id: u64) -> String {
    let AccountId(PublicKey::PublicKeyTypeEd25519(ed25519)) =
        AccountId::from_str(account).expect("valid G address");
    MuxedAccount::MuxedEd25519(MuxedAccountMed25519 { id, ed25519 }).to_string()
}

fn asset_json(issuer: &str) -> Value {
    json!({"code": "USD", "issuer": issuer})
}

fn muxed_vector(n: u32, input: &str, outcome: &str, title: &str) -> Value {
    let mut expected = json!({"outcome": outcome});
    if outcome != "reject" {
        expected["canonical_id"] = json!(MUXED_ID.to_string());
        expected["address"] = json!(muxed_address(ISSUER_LOW_BYTES, MUXED_ID));
    }
    json!({
        "id": format!("APIVEC-MUXED-ID-{n:04}"),
        "kind": "muxed_account_id",
        "title": title,
        "cap": ["CAP-0027"],
        "related_issues": [JS_1785],
        "input": {"account": ISSUER_LOW_BYTES, "id": input},
        "expected": expected,
    })
}

pub fn build() -> String {
    let (low, high) = (usd(ISSUER_LOW_BYTES), usd(ISSUER_HIGH_BYTES));

    // The premise of these vectors, checked rather than assumed: XDR order
    // and strkey text order disagree for this pair.
    assert!(low < high, "issuers must be in XDR order");
    assert!(
        ISSUER_LOW_BYTES > ISSUER_HIGH_BYTES,
        "strkey text order must be the reverse of XDR order"
    );

    let doc = json!({
        "$schema": "./schema/api-vectors.schema.json",
        "description": "SDK API-level cases. Expected values are computed with rs-stellar-xdr by generator/src/api_vectors.rs. Outcomes: accept (the SDK must accept the input and produce the expected value), reject (the SDK must refuse it), reject_or_canonical (the SDK must refuse it or normalize it to the canonical value; accepting it and reporting a different value is a defect).",
        "vectors": [
            {
                "id": "APIVEC-LPOOL-ISSUER-ORDER-0001",
                "kind": "liquidity_pool_asset",
                "title": "Liquidity pool asset pair in network order, where strkey text order is the reverse",
                "cap": ["CAP-0038"],
                "related_issues": [JS_1766],
                "input": {"asset_a": asset_json(ISSUER_LOW_BYTES), "asset_b": asset_json(ISSUER_HIGH_BYTES), "fee": LP_FEE},
                "expected": {"outcome": "accept", "pool_id": pool_id(low.clone(), high.clone())},
            },
            {
                "id": "APIVEC-LPOOL-ISSUER-ORDER-0002",
                "kind": "liquidity_pool_asset",
                "title": "The same pair reversed: sorted as strkey text, out of order for the network",
                "cap": ["CAP-0038"],
                "related_issues": [JS_1766],
                "input": {"asset_a": asset_json(ISSUER_HIGH_BYTES), "asset_b": asset_json(ISSUER_LOW_BYTES), "fee": LP_FEE},
                "expected": {"outcome": "reject"},
            },
            muxed_vector(1, "16", "accept", "Canonical decimal muxed ID (control)"),
            muxed_vector(2, "0x10", "reject_or_canonical", "Hexadecimal spelling of muxed ID 16"),
            muxed_vector(3, "016", "reject_or_canonical", "Leading-zero spelling of muxed ID 16"),
            muxed_vector(4, " 16 ", "reject_or_canonical", "Whitespace-padded spelling of muxed ID 16"),
            muxed_vector(5, "", "reject", "Empty string as a muxed ID"),
        ],
    });
    let mut text = serde_json::to_string_pretty(&doc).unwrap();
    text.push('\n');
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pool_id_depends_on_asset_order() {
        let (low, high) = (usd(ISSUER_LOW_BYTES), usd(ISSUER_HIGH_BYTES));
        assert_ne!(
            pool_id(low.clone(), high.clone()),
            pool_id(high, low),
            "a reversed pair must not hash to the same pool"
        );
    }

    #[test]
    fn muxed_address_round_trips() {
        let m = muxed_address(ISSUER_LOW_BYTES, MUXED_ID);
        assert!(m.starts_with('M'));
        match MuxedAccount::from_str(&m).expect("parse M address") {
            MuxedAccount::MuxedEd25519(med) => assert_eq!(med.id, MUXED_ID),
            MuxedAccount::Ed25519(_) => panic!("expected a muxed account"),
        }
    }
}
