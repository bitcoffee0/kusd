# Security model and limitations

## Enforced properties

- A Position cannot release collateral while debt is nonzero.
- Repayment and Auction settlement burn the exact full debt components.
- Minting capacity is bounded independently at Root, Module, and Position level.
- Cross-contract transitions authenticate templates, state, IDs, values, and
  cardinality on both sides of the transaction.
- Reserve backstop cannot settle one atom below full coverage.
- Governance cannot execute terms outside immutable global bounds.
- Savings cannot mint interest or overstate the elapsed DAA term.

## Known limitations

- SilverScript/Toccata remains experimental.
- No independent external audit has been completed.
- There is no price oracle. A Module liquidation price is fixed, and economic
  actors initiate Challenges.
- Module expiration prevents new Positions but does not expire existing
  Positions or enable Frankencoin-style declining-price collateral purchases.
  A deliberate expiration path is required before mainnet parity can be
  claimed.
- Challenges currently cover a whole Position. Partial Challenge splitting and
  its debt/Reserve rounding rules remain a pre-mainnet design item. The first
  partial implementation should permit only one challenged slice per Position
  at a time to prevent overlapping collateral/debt claims.
- Module and Savings proposal creation is serialized through their Governor
  singleton UTXOs. Independent Proposal UTXOs with Root-serialized execution
  are the recommended route to parallel application periods; replay, fee,
  execution-race, and Root-cap invariants require new consensus coverage.
- Toccata does not expose current DAA as a general arithmetic value. The design
  uses consensus timelocks, transaction lock time, immutable terms, and
  permissionless post-deadline transitions.
- Permissionless `mint/expire` and `avert/activate` pairs are confirmation races;
  only one branch can consume the shared UTXO.
- Reserve liveness depends on sufficient KUSD liquidity. Failure stays explicit
  as an open Auction rather than becoming unbacked supply.
- The indexer starts from a known deployment manifest; it does not yet discover
  every covenant lineage autonomously from the DAG.
- KUSD decimals are off-chain metadata under the currently used KCC format.
- A public RPC cannot provide controlled fork testing. A temporary multi-node
  consensus testbed and external review remain mandatory before mainnet.

The Testnet-10 demonstrations prove that the intended paths were accepted by
the network. They are not evidence of production safety.
