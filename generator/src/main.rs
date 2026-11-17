//! Corpus generator.
//!
//! Builds every corpus entry from its specification, encodes it with
//! rs-stellar-xdr, and writes `corpus/entries/<id>.xdr` plus
//! `corpus/manifest.json`. Output is fully deterministic.
//!
//! Usage:
//!   cargo run -p generator            # (re)write the corpus
//!   cargo run -p generator -- --check # fail if the files on disk differ

mod api_vectors;

use std::{
    fs,
    path::{Path, PathBuf},
    process::ExitCode,
};

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use stellar_xdr::{
    LedgerCloseMeta, LedgerEntry, LedgerEntryChange, LedgerEntryData, Limits, ReadXdr, ScMap,
    ScMapEntry, ScString, ScSymbol, ScVal, SignerKey, TransactionMeta, TransactionResultPair,
    TransactionResultResult, Validate, WriteXdr,
};

const STELLAR_XDR_VERSION: &str = "30.0.0";
const MANIFEST_VERSION: &str = "0.1.0";

/// One corpus entry as written by hand. Everything derivable from the
/// value itself (bytes, hash, expected decoded values) is computed.
struct Spec {
    id: &'static str,
    title: &'static str,
    description: &'static str,
    caps: &'static [&'static str],
    protocol_introduced: u32,
    tags: &'static [&'static str],
    related_issues: &'static [&'static str],
    value: ScVal,
}

fn sym(s: &str) -> ScVal {
    ScVal::Symbol(ScSymbol(s.try_into().expect("valid symbol")))
}

fn string(s: &str) -> ScVal {
    ScVal::String(ScString(s.try_into().expect("valid string")))
}

/// Builds a valid ScMap: sorted by key with no duplicate keys, as the
/// Soroban host requires. Panics if the spec itself is invalid.
fn map(pairs: Vec<(ScVal, ScVal)>) -> ScVal {
    let entries = pairs.into_iter().map(|(key, val)| ScMapEntry { key, val });
    ScVal::Map(Some(ScMap::sorted_from(entries).expect("valid ScMap")))
}

const CAP_46_01: &str = "CAP-0046-01";
const JS_1789: &str = "https://github.com/stellar/js-stellar-sdk/issues/1789";
const WB_729: &str = "https://github.com/stellar/wallet-backend/pull/729";
const WB_716: &str = "https://github.com/stellar/wallet-backend/issues/716";

fn specs() -> Vec<Spec> {
    vec![
        Spec {
            id: "SCVAL-MAP-KEYCOLLIDE-0001",
            title: "ScMap with u32(1) and symbol(\"1\") keys",
            description: "Two distinct keys whose native forms print the same in \
                JavaScript. This is the exact reproduction from js-stellar-sdk#1789.",
            caps: &[CAP_46_01],
            protocol_introduced: 20,
            tags: &["scmap", "native-conversion", "key-collision"],
            related_issues: &[JS_1789],
            value: map(vec![
                (ScVal::U32(1), ScVal::U32(10)),
                (sym("1"), ScVal::U32(20)),
            ]),
        },
        Spec {
            id: "SCVAL-MAP-KEYCOLLIDE-0002",
            title: "ScMap with string(\"k\") and symbol(\"k\") keys",
            description: "SCString and SCSymbol with the same text. Most languages \
                map both to their native string type.",
            caps: &[CAP_46_01],
            protocol_introduced: 20,
            tags: &["scmap", "native-conversion", "key-collision"],
            related_issues: &[JS_1789],
            value: map(vec![
                (string("k"), ScVal::U32(10)),
                (sym("k"), ScVal::U32(20)),
            ]),
        },
        Spec {
            id: "SCVAL-MAP-KEYCOLLIDE-0003",
            title: "ScMap with bool(true) and u32(1) keys",
            description: "Collides in languages where true equals 1 as a map key \
                (Python). Does not collide in JavaScript, where the keys become \
                \"true\" and \"1\".",
            caps: &[CAP_46_01],
            protocol_introduced: 20,
            tags: &["scmap", "native-conversion", "key-collision"],
            related_issues: &[JS_1789],
            value: map(vec![
                (ScVal::Bool(true), ScVal::U32(10)),
                (ScVal::U32(1), ScVal::U32(20)),
            ]),
        },
        Spec {
            id: "SCVAL-MAP-KEYCOLLIDE-0004",
            title: "ScMap with u32(1) and i64(1) keys",
            description: "Integer keys of different widths and signedness with the \
                same numeric value.",
            caps: &[CAP_46_01],
            protocol_introduced: 20,
            tags: &["scmap", "native-conversion", "key-collision"],
            related_issues: &[JS_1789],
            value: map(vec![
                (ScVal::U32(1), ScVal::U32(10)),
                (ScVal::I64(1), ScVal::U32(20)),
            ]),
        },
        Spec {
            id: "SCVAL-MAP-CONTROL-0001",
            title: "ScMap with two distinct symbol keys (control)",
            description: "No collision in any language. Every SDK must preserve both \
                entries; a failure here points at the runner, not the SDK.",
            caps: &[CAP_46_01],
            protocol_introduced: 20,
            tags: &["scmap", "native-conversion", "control"],
            related_issues: &[],
            value: map(vec![(sym("a"), ScVal::U32(1)), (sym("b"), ScVal::U32(2))]),
        },
    ]
}

/// Language-neutral description of a decoded value: its XDR type name and
/// its value as a string. Runners in every language produce the same form.
fn typed(v: &ScVal) -> Value {
    match v {
        ScVal::Bool(b) => json!({"type": "bool", "value": b.to_string()}),
        ScVal::U32(n) => json!({"type": "u32", "value": n.to_string()}),
        ScVal::I32(n) => json!({"type": "i32", "value": n.to_string()}),
        ScVal::U64(n) => json!({"type": "u64", "value": n.to_string()}),
        ScVal::I64(n) => json!({"type": "i64", "value": n.to_string()}),
        ScVal::Symbol(s) => json!({"type": "symbol", "value": s.0.to_utf8_string_lossy()}),
        ScVal::String(s) => json!({"type": "string", "value": s.0.to_utf8_string_lossy()}),
        ScVal::Map(Some(m)) => json!({
            "type": "map",
            "entries": m.0.iter().map(|e| json!({"key": typed(&e.key), "val": typed(&e.val)})).collect::<Vec<_>>(),
        }),
        other => panic!("typed(): no language-neutral form defined yet for {other:?}"),
    }
}

fn distinct_keys(v: &ScVal) -> Option<usize> {
    match v {
        ScVal::Map(Some(m)) => Some(m.0.len()),
        _ => None,
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

struct Built {
    file: PathBuf,
    bytes: Vec<u8>,
}

/// Finds `tx_hash` in a saved LedgerCloseMeta and returns its meta and the
/// ledger's protocol. Before trusting the file it checks that the ledger
/// sequence and ledger hash are the ones recorded at capture time and that
/// the transaction succeeded.
fn locate_tx(
    lcm_bytes: &[u8],
    ledger: u32,
    ledger_hash: &str,
    tx_hash: &str,
) -> Result<(TransactionMeta, u32), String> {
    let lcm = LedgerCloseMeta::from_xdr(lcm_bytes, Limits::none())
        .map_err(|e| format!("decode LedgerCloseMeta: {e}"))?;
    let (header, txs): (_, Vec<(&TransactionResultPair, &TransactionMeta)>) = match &lcm {
        LedgerCloseMeta::V0(m) => (
            &m.ledger_header,
            m.tx_processing
                .iter()
                .map(|t| (&t.result, &t.tx_apply_processing))
                .collect(),
        ),
        LedgerCloseMeta::V1(m) => (
            &m.ledger_header,
            m.tx_processing
                .iter()
                .map(|t| (&t.result, &t.tx_apply_processing))
                .collect(),
        ),
        LedgerCloseMeta::V2(m) => (
            &m.ledger_header,
            m.tx_processing
                .iter()
                .map(|t| (&t.result, &t.tx_apply_processing))
                .collect(),
        ),
    };
    if header.header.ledger_seq != ledger {
        return Err(format!(
            "ledger_seq is {}, capture says {ledger}",
            header.header.ledger_seq
        ));
    }
    if hex::encode(header.hash.0) != ledger_hash {
        return Err(format!(
            "ledger hash is {}, capture says {ledger_hash}",
            hex::encode(header.hash.0)
        ));
    }
    let (result, meta) = txs
        .into_iter()
        .find(|(r, _)| hex::encode(r.transaction_hash.0) == tx_hash)
        .ok_or_else(|| format!("tx {tx_hash} not in ledger {ledger}"))?;
    if !matches!(result.result.result, TransactionResultResult::TxSuccess(_)) {
        return Err(format!("tx {tx_hash} did not succeed"));
    }
    Ok((meta.clone(), header.header.ledger_version))
}

/// The Soroban return value recorded in a transaction's meta.
fn return_value(meta: &TransactionMeta) -> Result<ScVal, String> {
    match meta {
        TransactionMeta::V3(m) => m.soroban_meta.as_ref().map(|s| s.return_value.clone()),
        TransactionMeta::V4(m) => m.soroban_meta.as_ref().and_then(|s| s.return_value.clone()),
        _ => None,
    }
    .ok_or_else(|| "transaction has no Soroban return value".to_string())
}

/// Every ledger entry change in a transaction's operation meta, for the
/// meta versions that carry per-operation changes.
fn operation_changes(meta: &TransactionMeta) -> Result<Vec<&LedgerEntryChange>, String> {
    Ok(match meta {
        TransactionMeta::V1(m) => m
            .operations
            .iter()
            .flat_map(|o| o.changes.0.iter())
            .collect(),
        TransactionMeta::V2(m) => m
            .operations
            .iter()
            .flat_map(|o| o.changes.0.iter())
            .collect(),
        TransactionMeta::V3(m) => m
            .operations
            .iter()
            .flat_map(|o| o.changes.0.iter())
            .collect(),
        TransactionMeta::V4(m) => m
            .operations
            .iter()
            .flat_map(|o| o.changes.0.iter())
            .collect(),
        TransactionMeta::V0(_) => return Err("TransactionMeta V0 is not supported".to_string()),
    })
}

/// The one signed-payload signer a transaction added to an account: present
/// in the account entry core wrote after the operation (`Updated`) and absent
/// from the entry before it (`State`). Read from the meta, not the envelope.
fn added_signer(meta: &TransactionMeta) -> Result<SignerKey, String> {
    let signers = |e: &LedgerEntry| -> Vec<SignerKey> {
        match &e.data {
            LedgerEntryData::Account(a) => a.signers.iter().map(|s| s.key.clone()).collect(),
            _ => Vec::new(),
        }
    };
    let (mut before, mut after) = (Vec::new(), Vec::new());
    for change in operation_changes(meta)? {
        match change {
            LedgerEntryChange::State(e) => before.extend(signers(e)),
            LedgerEntryChange::Updated(e) | LedgerEntryChange::Created(e) => {
                after.extend(signers(e));
            }
            _ => {}
        }
    }
    let added: Vec<SignerKey> = after
        .into_iter()
        .filter(|k| !before.contains(k) && matches!(k, SignerKey::Ed25519SignedPayload(_)))
        .collect();
    match added.as_slice() {
        [one] => Ok(one.clone()),
        other => Err(format!(
            "expected one added signed-payload signer, found {}",
            other.len()
        )),
    }
}

/// Corpus entries captured from the network by capture/capture.mjs. The
/// capture step saved raw LedgerCloseMeta files; everything here runs
/// offline, so CI re-derives every captured entry from the saved meta.
fn captured(corpus: &Path, synthetic: &[(&str, &str, Vec<u8>)]) -> Vec<(Value, Built)> {
    let index = corpus.join("captured/captures.json");
    let Ok(text) = fs::read_to_string(&index) else {
        return Vec::new();
    };
    let doc: Value = serde_json::from_str(&text).expect("parse captures.json");
    let mut out = Vec::new();

    for c in doc["captures"].as_array().expect("captures") {
        let s = |k: &str| c[k].as_str().unwrap_or_else(|| panic!("capture field {k}"));
        let id = s("id");
        let ledger = u32::try_from(c["ledger"].as_u64().expect("ledger")).expect("ledger fits u32");
        let lcm_rel = s("ledger_close_meta");
        let lcm =
            fs::read(corpus.join(lcm_rel)).unwrap_or_else(|e| panic!("{id}: read {lcm_rel}: {e}"));
        assert_eq!(
            sha256_hex(&lcm),
            s("ledger_close_meta_sha256"),
            "{id}: LedgerCloseMeta file changed since capture"
        );

        let (meta, protocol) = locate_tx(&lcm, ledger, s("ledger_hash"), s("tx_hash"))
            .unwrap_or_else(|e| panic!("{id}: {e}"));
        let extract = c["extract"].as_str().unwrap_or("soroban_return_value");
        let rel = format!("entries/{id}.xdr");
        let mut source = json!({
            "kind": "testnet",
            "network_passphrase": s("network_passphrase"),
            "ledger": ledger,
            "ledger_hash": s("ledger_hash"),
            "tx_hash": s("tx_hash"),
            "ledger_close_meta": {
                "file": lcm_rel,
                "byte_length": lcm.len(),
                "sha256": sha256_hex(&lcm),
            },
        });

        let (mut entry, bytes) = match extract {
            "soroban_return_value" => {
                let value = return_value(&meta).unwrap_or_else(|e| panic!("{id}: {e}"));
                value
                    .validate()
                    .unwrap_or_else(|e| panic!("{id}: host emitted an invalid ScVal: {e}"));
                let bytes = value.to_xdr(Limits::none()).expect("encode");
                let shape_of = s("shape_of");
                let (_, shape_title, shape_bytes) = synthetic
                    .iter()
                    .find(|(sid, _, _)| *sid == shape_of)
                    .unwrap_or_else(|| panic!("{id}: unknown shape_of {shape_of}"));
                source["method"] = json!(
                    "TransactionMeta.soroban_meta.return_value of tx_hash, extracted from the \
                     LedgerCloseMeta returned by RPC getLedgers (capture/capture.mjs, then \
                     generator/src/main.rs return_value)"
                );
                source["contract_id"] = json!(s("contract_id"));
                let mut entry = json!({
                    "id": id,
                    "title": format!("{shape_title}, captured on testnet"),
                    "description": format!(
                        "Return value of a real testnet transaction, extracted from the \
                         LedgerCloseMeta stellar-core produced for ledger {ledger}. Same \
                         shape as {shape_of}."
                    ),
                    "xdr_type": "ScVal",
                    "file": rel,
                    "encoding": "raw",
                    "byte_length": bytes.len(),
                    "sha256": sha256_hex(&bytes),
                    "cap": [CAP_46_01],
                    "protocol_version": {"introduced": 20, "observed": protocol},
                    "synthetic": false,
                    "source": source,
                    "tags": ["scmap", "native-conversion", "key-collision", "captured"],
                    "related_issues": [JS_1789],
                    "expected": {
                        "decoded": typed(&value),
                        "xdr_json": serde_json::to_value(&value).expect("xdr-json"),
                        "base64": base64_encode(&bytes),
                    },
                });
                if bytes == *shape_bytes {
                    entry["same_bytes_as"] = json!(shape_of);
                }
                if let Some(n) = distinct_keys(&value) {
                    entry["native_conversion"] =
                        json!({"rule": "preserve-or-reject", "distinct_keys": n});
                }
                (entry, bytes)
            }
            "added_signer" => {
                let key = added_signer(&meta).unwrap_or_else(|e| panic!("{id}: {e}"));
                let bytes = key.to_xdr(Limits::none()).expect("encode");
                let SignerKey::Ed25519SignedPayload(sp) = &key else {
                    unreachable!("added_signer returns signed-payload signers only")
                };
                let n = sp.payload.len();
                source["method"] = json!(
                    "The signed-payload signer added by tx_hash: present in the account entry \
                     after the operation and absent before it, in the operation changes of the \
                     LedgerCloseMeta returned by RPC getLedgers (capture/capture-signers.mjs, \
                     then generator/src/main.rs added_signer)"
                );
                let entry = json!({
                    "id": id,
                    "title": format!("Signed-payload signer with a {n}-byte payload, captured on testnet"),
                    "description": format!(
                        "SignerKey of type ED25519_SIGNED_PAYLOAD (CAP-0040) with a {n}-byte \
                         payload ({} bytes of XDR padding), taken from the account entry \
                         stellar-core wrote for ledger {ledger}. The strkey is a P address \
                         (SEP-0023). The shape that halted wallet-backend ingestion (#729) \
                         and was truncated by it (#716).",
                        (4 - n % 4) % 4
                    ),
                    "xdr_type": "SignerKey",
                    "file": rel,
                    "encoding": "raw",
                    "byte_length": bytes.len(),
                    "sha256": sha256_hex(&bytes),
                    "cap": ["CAP-0040"],
                    "protocol_version": {"introduced": 19, "observed": protocol},
                    "synthetic": false,
                    "source": source,
                    "tags": ["signer", "signed-payload", "strkey", "captured"],
                    "related_issues": [WB_729, WB_716],
                    "expected": {
                        "decoded": {"type": "signer_key", "value": key.to_string()},
                        "xdr_json": serde_json::to_value(&key).expect("xdr-json"),
                        "base64": base64_encode(&bytes),
                    },
                });
                (entry, bytes)
            }
            other => panic!("{id}: unknown extract kind {other}"),
        };
        entry["source"]["kind"] = json!("testnet");
        out.push((
            entry,
            Built {
                file: corpus.join(&rel),
                bytes,
            },
        ));
    }
    out
}

fn build(corpus: &Path) -> (Vec<Built>, String) {
    let mut built = Vec::new();
    let mut entries = Vec::new();
    let mut synthetic: Vec<(&str, &str, Vec<u8>)> = Vec::new();

    for s in specs() {
        let bytes = s.value.to_xdr(Limits::none()).expect("encode");

        // Self-check: the bytes must decode back to the same value and
        // re-encode to the same bytes before they enter the corpus.
        let decoded = ScVal::from_xdr(&bytes, Limits::none()).expect("decode");
        assert_eq!(decoded, s.value, "{}: decode mismatch", s.id);
        assert_eq!(
            decoded.to_xdr(Limits::none()).unwrap(),
            bytes,
            "{}: round-trip mismatch",
            s.id
        );

        let rel = format!("entries/{}.xdr", s.id);
        let mut entry = json!({
            "id": s.id,
            "title": s.title,
            "description": s.description,
            "xdr_type": "ScVal",
            "file": rel,
            "encoding": "raw",
            "byte_length": bytes.len(),
            "sha256": sha256_hex(&bytes),
            "cap": s.caps,
            "protocol_version": {"introduced": s.protocol_introduced},
            "synthetic": true,
            "source": {
                "kind": "synthetic",
                "method": format!("generator/src/main.rs via ScMap::sorted_from (rs-stellar-xdr {STELLAR_XDR_VERSION})"),
                "network_passphrase": null,
                "ledger": null,
                "tx_hash": null
            },
            "tags": s.tags,
            "related_issues": s.related_issues,
            "expected": {
                "decoded": typed(&s.value),
                "xdr_json": serde_json::to_value(&s.value).expect("xdr-json"),
                "base64": base64_encode(&bytes),
            },
        });
        if let Some(n) = distinct_keys(&s.value) {
            entry["native_conversion"] = json!({
                "rule": "preserve-or-reject",
                "distinct_keys": n,
            });
        }
        entries.push(entry);
        synthetic.push((s.id, s.title, bytes.clone()));
        built.push(Built {
            file: corpus.join(&rel),
            bytes,
        });
    }

    for (entry, b) in captured(corpus, &synthetic) {
        entries.push(entry);
        built.push(b);
    }

    let manifest = json!({
        "$schema": "./schema/manifest.schema.json",
        "manifest_version": MANIFEST_VERSION,
        "generator": {"name": "generator", "stellar_xdr": STELLAR_XDR_VERSION},
        "entries": entries,
    });
    let mut text = serde_json::to_string_pretty(&manifest).unwrap();
    text.push('\n');
    (built, text)
}

/// Minimal standard base64 (RFC 4648, with padding), so the generator
/// needs no extra dependency.
fn base64_encode(input: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

fn main() -> ExitCode {
    let check = std::env::args().any(|a| a == "--check");
    let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("../corpus");
    let manifest_path = corpus.join("manifest.json");
    let (built, manifest) = build(&corpus);
    let api_path = corpus.join("api-vectors.json");
    let api = api_vectors::build();

    if check {
        let mut drift = Vec::new();
        for b in &built {
            if fs::read(&b.file).ok().as_deref() != Some(b.bytes.as_slice()) {
                drift.push(b.file.display().to_string());
            }
        }
        if fs::read_to_string(&manifest_path).ok().as_deref() != Some(manifest.as_str()) {
            drift.push(manifest_path.display().to_string());
        }
        if fs::read_to_string(&api_path).ok().as_deref() != Some(api.as_str()) {
            drift.push(api_path.display().to_string());
        }
        if drift.is_empty() {
            println!(
                "corpus is reproducible: {} entries match the generator",
                built.len()
            );
            return ExitCode::SUCCESS;
        }
        eprintln!("corpus drift, run `cargo run -p generator` and commit the result:");
        for d in drift {
            eprintln!("  {d}");
        }
        return ExitCode::FAILURE;
    }

    fs::create_dir_all(corpus.join("entries")).unwrap();
    for b in &built {
        fs::write(&b.file, &b.bytes).unwrap();
    }
    fs::write(&manifest_path, manifest).unwrap();
    fs::write(&api_path, api).unwrap();
    println!(
        "wrote {} entries and {}",
        built.len(),
        manifest_path.display()
    );
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique() {
        let mut ids: Vec<_> = specs().iter().map(|s| s.id).collect();
        let n = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), n);
    }

    #[test]
    fn base64_matches_rfc4648_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn every_map_key_is_distinct_at_the_xdr_level() {
        for s in specs() {
            if let ScVal::Map(Some(m)) = &s.value {
                let mut keys: Vec<_> = m.0.iter().map(|e| e.key.clone()).collect();
                let n = keys.len();
                keys.dedup();
                assert_eq!(keys.len(), n, "{}", s.id);
            }
        }
    }
}
