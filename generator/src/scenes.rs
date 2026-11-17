//! Replay scenes: real ledgers kept whole, with the participants an indexer
//! must record for a transaction, derived from the ledger meta.
//!
//! Reference definition (participants are never taken from the envelope):
//! - `fee`: owners of entries changed in fee processing (before and after
//!   apply).
//! - `state`: owners of entries created, updated, removed, restored or
//!   recorded as prior state in the transaction-level changes and in every
//!   operation's changes.
//! - Owner of an entry: the account for account, trustline, offer and data
//!   entries; the contract (or account) address for contract data. Claimable
//!   balances, liquidity pools, contract code, config and TTL entries have no
//!   owner. A sponsor therefore appears through the change to its own
//!   account entry.
//!
//! The capture step saved raw LedgerCloseMeta files; everything here runs
//! offline and is re-derived in CI.

use std::{collections::BTreeSet, fs, path::Path};

use serde_json::{json, Value};
use stellar_xdr::{
    LedgerCloseMeta, LedgerEntry, LedgerEntryChange, LedgerEntryData, LedgerKey, Limits, ReadXdr,
    TransactionMeta, TransactionResultResult,
};

const WB_677: &str = "https://github.com/stellar/wallet-backend/issues/677";

fn entry_owner(e: &LedgerEntry) -> Option<String> {
    Some(match &e.data {
        LedgerEntryData::Account(a) => a.account_id.to_string(),
        LedgerEntryData::Trustline(t) => t.account_id.to_string(),
        LedgerEntryData::Offer(o) => o.seller_id.to_string(),
        LedgerEntryData::Data(d) => d.account_id.to_string(),
        LedgerEntryData::ContractData(c) => c.contract.to_string(),
        _ => return None,
    })
}

fn key_owner(k: &LedgerKey) -> Option<String> {
    Some(match k {
        LedgerKey::Account(a) => a.account_id.to_string(),
        LedgerKey::Trustline(t) => t.account_id.to_string(),
        LedgerKey::Offer(o) => o.seller_id.to_string(),
        LedgerKey::Data(d) => d.account_id.to_string(),
        LedgerKey::ContractData(c) => c.contract.to_string(),
        _ => return None,
    })
}

fn owners<'a>(changes: impl IntoIterator<Item = &'a LedgerEntryChange>) -> BTreeSet<String> {
    changes
        .into_iter()
        .filter_map(|c| match c {
            LedgerEntryChange::Created(e)
            | LedgerEntryChange::Updated(e)
            | LedgerEntryChange::State(e)
            | LedgerEntryChange::Restored(e) => entry_owner(e),
            LedgerEntryChange::Removed(k) => key_owner(k),
        })
        .collect()
}

/// Every transaction-level and operation-level change, for all five
/// TransactionMeta versions.
fn state_changes(meta: &TransactionMeta) -> Vec<&LedgerEntryChange> {
    match meta {
        TransactionMeta::V0(ops) => ops.iter().flat_map(|o| o.changes.0.iter()).collect(),
        TransactionMeta::V1(m) => m
            .tx_changes
            .0
            .iter()
            .chain(m.operations.iter().flat_map(|o| o.changes.0.iter()))
            .collect(),
        TransactionMeta::V2(m) => m
            .tx_changes_before
            .0
            .iter()
            .chain(m.operations.iter().flat_map(|o| o.changes.0.iter()))
            .chain(m.tx_changes_after.0.iter())
            .collect(),
        TransactionMeta::V3(m) => m
            .tx_changes_before
            .0
            .iter()
            .chain(m.operations.iter().flat_map(|o| o.changes.0.iter()))
            .chain(m.tx_changes_after.0.iter())
            .collect(),
        TransactionMeta::V4(m) => m
            .tx_changes_before
            .0
            .iter()
            .chain(m.operations.iter().flat_map(|o| o.changes.0.iter()))
            .chain(m.tx_changes_after.0.iter())
            .collect(),
    }
}

fn meta_version(meta: &TransactionMeta) -> u32 {
    match meta {
        TransactionMeta::V0(_) => 0,
        TransactionMeta::V1(_) => 1,
        TransactionMeta::V2(_) => 2,
        TransactionMeta::V3(_) => 3,
        TransactionMeta::V4(_) => 4,
    }
}

struct Located {
    protocol: u32,
    meta: TransactionMeta,
    fee_changes: Vec<LedgerEntryChange>,
    successful: bool,
}

fn locate(
    lcm_bytes: &[u8],
    ledger: u32,
    ledger_hash: &str,
    tx_hash: &str,
) -> Result<Located, String> {
    let lcm = LedgerCloseMeta::from_xdr(lcm_bytes, Limits::none())
        .map_err(|e| format!("decode LedgerCloseMeta: {e}"))?;
    let (header, found) = match &lcm {
        LedgerCloseMeta::V0(m) => (
            &m.ledger_header,
            m.tx_processing
                .iter()
                .find(|t| hex::encode(t.result.transaction_hash.0) == tx_hash)
                .map(|t| {
                    (
                        &t.result,
                        &t.tx_apply_processing,
                        t.fee_processing.0.to_vec(),
                    )
                }),
        ),
        LedgerCloseMeta::V1(m) => (
            &m.ledger_header,
            m.tx_processing
                .iter()
                .find(|t| hex::encode(t.result.transaction_hash.0) == tx_hash)
                .map(|t| {
                    (
                        &t.result,
                        &t.tx_apply_processing,
                        t.fee_processing.0.to_vec(),
                    )
                }),
        ),
        LedgerCloseMeta::V2(m) => (
            &m.ledger_header,
            m.tx_processing
                .iter()
                .find(|t| hex::encode(t.result.transaction_hash.0) == tx_hash)
                .map(|t| {
                    let mut fee = t.fee_processing.0.to_vec();
                    fee.extend(t.post_tx_apply_fee_processing.0.iter().cloned());
                    (&t.result, &t.tx_apply_processing, fee)
                }),
        ),
    };
    if header.header.ledger_seq != ledger {
        return Err(format!(
            "ledger_seq is {}, scene says {ledger}",
            header.header.ledger_seq
        ));
    }
    if hex::encode(header.hash.0) != ledger_hash {
        return Err(format!(
            "ledger hash differs from the scene record ({ledger_hash})"
        ));
    }
    let (result, meta, fee_changes) =
        found.ok_or_else(|| format!("tx {tx_hash} not in ledger {ledger}"))?;
    Ok(Located {
        protocol: header.header.ledger_version,
        meta: meta.clone(),
        fee_changes,
        successful: matches!(result.result.result, TransactionResultResult::TxSuccess(_)),
    })
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(bytes))
}

/// Builds `corpus/scenes.json` from `corpus/captured/scenes.json`.
pub fn build(corpus: &Path) -> String {
    let index = corpus.join("captured/scenes.json");
    let mut scenes = Vec::new();
    if let Ok(text) = fs::read_to_string(&index) {
        let doc: Value = serde_json::from_str(&text).expect("parse captured/scenes.json");
        for c in doc["scenes"].as_array().expect("scenes") {
            let s = |k: &str| c[k].as_str().unwrap_or_else(|| panic!("scene field {k}"));
            let id = s("id");
            let ledger = u32::try_from(c["ledger"].as_u64().expect("ledger")).expect("u32");
            let lcm_rel = s("ledger_close_meta");
            let lcm = fs::read(corpus.join(lcm_rel)).unwrap_or_else(|e| panic!("{id}: {e}"));
            assert_eq!(
                sha256_hex(&lcm),
                s("ledger_close_meta_sha256"),
                "{id}: LedgerCloseMeta file changed since capture"
            );
            let l = locate(&lcm, ledger, s("ledger_hash"), s("tx_hash"))
                .unwrap_or_else(|e| panic!("{id}: {e}"));
            assert!(l.successful, "{id}: the scene transaction must succeed");

            let fee = owners(l.fee_changes.iter());
            let state = owners(state_changes(&l.meta));
            let all: BTreeSet<String> = fee.union(&state).cloned().collect();

            // The point of this scene, checked rather than assumed: the
            // sponsor is a participant through the meta only.
            let sponsor = c["roles"]["sponsor"].as_str().expect("roles.sponsor");
            let claimant = c["roles"]["claimant"].as_str().expect("roles.claimant");
            assert!(
                state.contains(sponsor),
                "{id}: sponsor missing from meta-derived participants"
            );
            assert!(!fee.contains(sponsor), "{id}: sponsor must not pay the fee");
            assert!(
                all.contains(claimant),
                "{id}: claimant missing from meta-derived participants"
            );

            scenes.push(json!({
                "id": id,
                "title": "A claimant claims a claimable balance whose reserve another account sponsors",
                "description": "ClaimClaimableBalance names only the claimant (source) and the balance ID. \
                    stellar-core removes the balance and updates the sponsor's account entry in the \
                    operation meta, because the sponsor's numSponsoring drops. The sponsor is therefore \
                    a participant, visible only in the meta.",
                "cap": ["CAP-0023", "CAP-0033"],
                "related_issues": [WB_677],
                "source": {
                    "kind": "testnet",
                    "network_passphrase": s("network_passphrase"),
                    "ledger": ledger,
                    "ledger_hash": s("ledger_hash"),
                    "tx_hash": s("tx_hash"),
                    "setup_tx_hash": s("setup_tx_hash"),
                    "ledger_close_meta": {
                        "file": lcm_rel,
                        "byte_length": lcm.len(),
                        "sha256": sha256_hex(&lcm),
                    },
                },
                "protocol_observed": l.protocol,
                "transaction_meta_version": meta_version(&l.meta),
                "roles": {"sponsor": sponsor, "claimant": claimant},
                "expected_participants": {
                    "fee": fee,
                    "state": state,
                    "all": all,
                },
            }));
        }
    }
    let doc = json!({
        "$schema": "./schema/scenes.schema.json",
        "description": "Replay scenes: whole ledgers with the participants an indexer must record, derived from the ledger meta by generator/src/scenes.rs under the definition at the top of that file.",
        "scenes": scenes,
    });
    let mut text = serde_json::to_string_pretty(&doc).unwrap();
    text.push('\n');
    text
}
