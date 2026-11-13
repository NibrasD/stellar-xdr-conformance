# Security policy

## Reporting a problem in this repository

Open a [private security advisory](https://github.com/NibrasD/stellar-xdr-conformance/security/advisories/new)
on GitHub. Please do not open a public issue for a vulnerability.

## Defects this harness finds in Stellar SDKs

The harness is built to find decoding defects in other projects. When it
finds one:

1. We check whether it is already reported upstream. If it is, we link to
   that report and do not file a duplicate.
2. If a defect could let a crafted transaction crash an indexer or corrupt
   its data, we report it privately to the affected project through its
   security policy first, and add the corpus entry only after a fix is
   available.
3. Other defects are filed as public issues, with the corpus entry that
   reproduces them, and recorded in `findings/`.

Every finding records the upstream link, the corpus entries, the affected
versions and the severity.

## What runs where

The generator, runners and report run offline against files in this
repository. They need no network access after dependencies are installed,
and no keys or funds. The CI workflow has read-only repository permissions.
Dependencies are pinned with integrity checks: `package-lock.json` (npm
integrity hashes), `go.sum`, `Cargo.lock`, and a fully resolved Python
lockfile (`runners/python/requirements.txt`, every transitive package with
its SHA-256, installed with `pip --require-hashes`). The Rust toolchain is
pinned in `rust-toolchain.toml`. GitHub Actions are pinned to full commit
SHAs, and checkout does not persist credentials.
