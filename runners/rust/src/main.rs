//! Rust conformance runner (rs-stellar-xdr).
//!
//! Rust is the reference implementation in this matrix: the generator is
//! built on the same crate, and its ScMap is an ordered list of entries, so
//! there is no native-map conversion that could merge keys. The runner still
//! checks every entry independently, so a regression in rs-stellar-xdr shows
//! up here like in any other SDK.
//!
//!   cargo run -p conformance-runner-rust -- [--corpus corpus] [--out results]

use std::{
    fs,
    path::{Path, PathBuf},
};

use serde_json::{json, Value};
use stellar_xdr::{Limits, ReadXdr, ScVal, Validate, WriteXdr};

const STELLAR_XDR_VERSION: &str = "30.0.0";

fn arg(name: &str, fallback: &str) -> PathBuf {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .map_or_else(|| PathBuf::from(fallback), PathBuf::from)
}

/// Language-neutral form of a decoded value (see corpus/schema).
fn typed(v: &ScVal) -> Result<Value, String> {
    Ok(match v {
        ScVal::Bool(b) => json!({"type": "bool", "value": b.to_string()}),
        ScVal::U32(n) => json!({"type": "u32", "value": n.to_string()}),
        ScVal::I32(n) => json!({"type": "i32", "value": n.to_string()}),
        ScVal::U64(n) => json!({"type": "u64", "value": n.to_string()}),
        ScVal::I64(n) => json!({"type": "i64", "value": n.to_string()}),
        ScVal::Symbol(s) => json!({"type": "symbol", "value": s.0.to_utf8_string_lossy()}),
        ScVal::String(s) => json!({"type": "string", "value": s.0.to_utf8_string_lossy()}),
        ScVal::Map(Some(m)) => {
            let mut entries = Vec::new();
            for e in m.0.iter() {
                entries.push(json!({"key": typed(&e.key)?, "val": typed(&e.val)?}));
            }
            json!({"type": "map", "entries": entries})
        }
        other => {
            return Err(format!(
                "no language-neutral form for {:?}",
                other.discriminant()
            ))
        }
    })
}

fn check(corpus: &Path, entry: &Value) -> Value {
    let id = entry["id"].as_str().unwrap_or_default();
    let file = corpus.join(entry["file"].as_str().unwrap_or_default());
    let mut r = json!({
        "id": id, "decode": "fail", "roundtrip": "fail", "structure": "fail",
        "xdr_json": "fail", "native": "n/a", "detail": ""
    });

    let bytes = match fs::read(&file) {
        Ok(b) => b,
        Err(e) => {
            r["detail"] = json!(format!("read {}: {e}", file.display()));
            return r;
        }
    };
    let val = match ScVal::from_xdr(&bytes, Limits::none()) {
        Ok(v) => v,
        Err(e) => {
            r["detail"] = json!(format!("decode: {e}"));
            return r;
        }
    };
    r["decode"] = json!("pass");

    if val.to_xdr(Limits::none()).ok().as_deref() == Some(bytes.as_slice()) {
        r["roundtrip"] = json!("pass");
    }
    match typed(&val) {
        Ok(t) if t == entry["expected"]["decoded"] => r["structure"] = json!("pass"),
        Ok(t) => r["detail"] = json!(format!("structure: got {t}")),
        Err(e) => r["detail"] = json!(format!("structure: {e}")),
    }
    if serde_json::to_value(&val).ok().as_ref() == Some(&entry["expected"]["xdr_json"]) {
        r["xdr_json"] = json!("pass");
    }
    // A map that is unsorted or holds duplicate keys would be rejected by
    // the Soroban host; every corpus entry must pass the same check.
    if let Err(e) = val.validate() {
        r["detail"] = json!(format!("validate: {e}"));
        r["decode"] = json!("fail");
    }
    r
}

fn main() {
    let corpus = arg("--corpus", "corpus");
    let out = arg("--out", "results");
    let manifest: Value = serde_json::from_str(
        &fs::read_to_string(corpus.join("manifest.json")).expect("read manifest"),
    )
    .expect("parse manifest");

    let results: Vec<Value> = manifest["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .map(|e| check(&corpus, e))
        .collect();

    println!("rust stellar-xdr {STELLAR_XDR_VERSION}");
    for r in &results {
        println!(
            "  {:<28} decode={} roundtrip={} structure={} xdr_json={} native={}",
            r["id"].as_str().unwrap_or_default(),
            r["decode"].as_str().unwrap_or_default(),
            r["roundtrip"].as_str().unwrap_or_default(),
            r["structure"].as_str().unwrap_or_default(),
            r["xdr_json"].as_str().unwrap_or_default(),
            r["native"].as_str().unwrap_or_default(),
        );
    }

    let report = json!({
        "runner": "rust",
        "sdk": "stellar-xdr",
        "sdk_version": STELLAR_XDR_VERSION,
        "native_function": null,
        "runtime": format!("rust (MSRV {})", env!("CARGO_PKG_RUST_VERSION")),
        "results": results,
    });
    fs::create_dir_all(&out).expect("create out dir");
    let path = out.join(format!("rust-{STELLAR_XDR_VERSION}.json"));
    let mut text = serde_json::to_string_pretty(&report).unwrap();
    text.push('\n');
    fs::write(&path, text).expect("write results");
}
