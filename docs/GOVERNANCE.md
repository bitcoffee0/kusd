# Governance

Governance is veto-based and serialized: only one Module proposal may be active
at once. Root handover is irreversible; no transition returns Root authority to
a private key.

## Proposal lifecycle

1. A proposer locks both the KAS proposal deposit and an exact KUSD fee. The
   KUSD is escrowed under the new Proposal Covenant ID.
2. Governance authenticates an exact `ModuleProposal` successor and verifies
   every term against its immutable global bounds.
3. Activation becomes permissionless after the voting delay.
4. An activated proposal can be executed during its execution window.
5. Sufficient mature, delegated KPS can veto it.
6. After expiration, anyone may cancel it and release the serialized singleton.

Execution and expiry cancellation return both deposits to the proposer. A veto
returns the KAS deposit but atomically merges the escrowed KUSD fee into
EquityReserve. The Proposal, Governance, Reserve, and KCC20 inputs independently
authenticate this multi-contract transition; no private key controls the
escrowed fee.

Veto aggregation authenticates each delegation UTXO and prevents duplicate
weight. KPS weight grows in bounded DAA steps. The economic owner can recover
delegated KPS after the holding rules; a delegate can only exercise veto.

Savings uses a separate Governor but the same KPS veto base and Reserve. This
limits the blast radius of interest-policy changes.
