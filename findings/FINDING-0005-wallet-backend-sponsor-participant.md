# FINDING-0005: sponsor dropped from a transaction's participants (wallet-backend)

| Field | Value |
| --- | --- |
| Component | wallet-backend, `internal/indexer/processors.ParticipantsProcessor.GetTransactionParticipants` |
| Upstream issue | [stellar/wallet-backend#677](https://github.com/stellar/wallet-backend/issues/677), open |
| Affected | `3e22515a44979fdc3a558fda3991ca2b48e38cef` (current `main` when tested) |
| Fixed | Not fixed upstream at the time of writing |
| Severity | Medium: under-attribution. A transaction is missing from an affected account's history; nothing is invented |
| CAP | CAP-0023 (claimable balances), CAP-0033 (sponsored reserves) |
| Scene | `SCENE-SPONSORED-CB-CLAIM-0001` (testnet ledger 5777140, protocol 30, `TransactionMeta` V4) |
| Status | Reproduced on the pinned commit with the indexer's own code; control patch passes |

This issue is already reported upstream, and others have reproduced it in
its thread. This file records an independent reproduction on our own
scene, linked to the existing issue rather than filed again.

## What happens

`participantsForMeta` reads only the `Operations` field of
`TransactionMeta`, which is set for meta version 0. Current ledgers carry
version 3 or 4, where operations sit under `V3` or `V4`, so the function
returns nothing and participants come only from the envelope and the fee
changes.

The scene makes the consequence visible. Account S creates a claimable
balance for account C and pays its reserve, so S sponsors it. C claims it.
The claim names only C and the balance ID. stellar-core removes the balance
and updates S's account entry in the operation meta, because S's
`numSponsoring` drops. S is a participant through the meta only.

## Reproduction

```bash
./repro/wallet-backend/run.sh
./repro/wallet-backend/run.sh --control
```

`run.sh` checks out wallet-backend at the pinned commit, adds one test file
to `internal/indexer/processors`, and calls
`GetTransactionParticipants` unmodified on the transaction read from the
saved `LedgerCloseMeta` with the Go SDK's ingest reader. It then compares
what the indexer records with the participants the generator derives from
the meta (`corpus/scenes.json`).

| Run | Recorded | Expected (from meta) | Result |
| --- | --- | --- | --- |
| `3e22515` as is | claimant | claimant, sponsor | **dropped**: sponsor missing |
| `3e22515` + `control-677.patch` | claimant, sponsor | claimant, sponsor | pass |

The control patch is ours, not an upstream fix. It reads participants from
every change the ingest SDK's `GetChanges` returns, which is the approach
the issue proposes. Its only purpose is to show that the comparison
separates a correct participant set from a wrong one: the same scene fails
without it and passes with it.

## Scope

This is a component-level reproduction: it calls the participant function
directly, without starting the service or its database. Driving the running
indexer through the replay harness is part of the proposal.
