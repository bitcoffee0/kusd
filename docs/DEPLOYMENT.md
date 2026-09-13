# Deployment and indexing

The checked-in branch is network-agnostic at the covenant layer but its current
configuration accepts only `testnet-10`. `.env` must contain:

```dotenv
KASPA_NETWORK=testnet-10
KASPA_RPC_URL=public
KASPA_PRIVATE_KEY=<32-byte testnet key>
```

`scripts/deploy.py` builds the complete transaction graph, calculates compute
mass, storage mass, and relay fees, executes every covenant input in the local
consensus VM, then optionally broadcasts transactions sequentially. Progress is
persisted after every confirmation. DAA-sensitive descendants are rebuilt from
confirmed parent DAA scores when their transaction identity cannot be known in
advance.

Generated files use the `KUSD_TAG` prefix, defaulting to `kusd`. They are local
and ignored by Git.

The index manifest records exact transaction history and final UTXOs. RPC
validation requires a simultaneous match of outpoint, full state-script address,
and Covenant ID. The indexer replays history atomically, derives supply/debt and
protocol counters, and supports deterministic branch rollback and replacement.

A public RPC is sufficient for deployment and observation. It cannot create a
controlled network fork, so actual consensus reorganization testing requires an
ephemeral multi-node test harness before mainnet.

The identifiers and validation transactions for the currently deployed public
test stack are recorded in [TESTNET.md](TESTNET.md). They are evidence for this
specific build only and are never reused as protocol configuration.
