//! Corpus generator.
//!
//! Builds every corpus entry from its specification, encodes it with
//! rs-stellar-xdr, and writes `corpus/entries/<id>.xdr` plus
//! `corpus/manifest.json`. Output is fully deterministic.
//!
//! Usage:
//!   cargo run -p generator            # (re)write the corpus
//!   cargo run -p generator -- --check # fail if the files on disk differ

use std::{
    fs,
    path::{Path, PathBuf},
    process::ExitCode,
};

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use stellar_xdr::{Limits, ReadXdr, ScMap, ScMapEntry, ScString, ScSymbol, ScVal, WriteXdr};

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

fn build(corpus: &Path) -> (Vec<Built>, String) {
    let mut built = Vec::new();
    let mut entries = Vec::new();

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
        built.push(Built {
            file: corpus.join(&rel),
            bytes,
        });
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
