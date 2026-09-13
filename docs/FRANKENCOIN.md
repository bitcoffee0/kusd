# Relationship to Frankencoin

KUSD openly takes inspiration from the economic architecture of
[Frankencoin](https://github.com/Frankencoin-ZCHF/Frankencoin): collateralized
minting positions, parallel modules with immutable terms, veto-based equity
governance, challenges, auctions, an equity reserve, and lazy Savings interest.

KUSD is not source-compatible with Frankencoin and does not claim identical
security properties. Frankencoin is account/EVM based; KUSD is UTXO-native.
The source comparison used Frankencoin commit
[`7409532`](https://github.com/Frankencoin-ZCHF/Frankencoin/tree/7409532eac3ac77e8f7d4f2887bb538c0ee0c72d).
See [Economic parameters](ECONOMICS.md) for explicit TN10 fixtures and the
on-chain Frankencoin reference constraints. KUSD's test values are adaptations,
not copied production recommendations.

## Important adaptations

- Singleton state is represented by explicitly chained covenant UTXOs.
- Every Position, Challenge, Auction, delegation, and Savings account has an
  independent lineage.
- Atomic multi-contract behavior is implemented by input/output introspection,
  template-state validation, Covenant bindings, and strict cardinality checks.
- A stable Asset ID is shared by all Modules; Module updates never create a new
  currency.
- DAA timelocks replace EVM timestamp checks. Where current DAA cannot be read
  arithmetically, signed claims are bounded by consensus age or lock-time rules.
- Interest is transferred from Reserve during refresh rather than continuously
  updating account balances.
- Auction failure remains an explicit live UTXO until a bidder or fully funded
  Reserve path can settle it.

Features intentionally not reproduced include Frankencoin bridges and EVM-only
integration surfaces. Protocol behavior should be evaluated from KUSD's own
covenants and tests, not inferred from Frankencoin.

## Current parity boundaries

The protocol reproduces the main economic roles, not every Frankencoin user
operation. The table below is deliberately explicit so reviewers do not infer
features from the inspiration claim.

| Area | Frankencoin | Current KUSD |
|---|---|---|
| Collateral | Multiple ERC-20 collateral assets | Native KAS only |
| Liquidation-price ceiling | No global economic maximum; a Position owner may raise its price by at most 3x per adjustment, with cooldown | No global economic maximum; price is immutable for a Module/Position and must be positive |
| Position lifecycle | Mint and repay repeatedly, adjust collateral and price, withdraw collateral, clone/roll Positions | Open, partial/full repay, close; no additional mint, collateral adjustment, price adjustment, clone, or roll |
| Expiration | Each Position expires and its collateral can be bought through a separate declining-price path | Module expiration stops new Positions; existing Positions do not yet have Frankencoin's expired-collateral purchase path. This remains an explicit pre-mainnet design item |
| Challenges | Partial collateral amounts; concurrent/partial settlement is supported | One full-Position Challenge lineage at a time |
| Challenge auction | Two time phases derived from the challenge period; price can decay to zero and losses can be covered by the system | Separate warning and Auction durations; price stops at the full-debt/no-deficit floor, then a fully funded Reserve backstop is allowed |
| Bad debt | `coverLoss` can consume equity and ultimately interact with the minter reserve | Settlement is fail-closed: debt must be burned in full; an underfunded Reserve leaves the Auction live |
| Minter proposals | Parallel suggestions are possible; application fee is irrevocably collected on submission | Governance serializes one active Module proposal; KUSD fee is refunded on execute/cancel and retained only after veto |
| Proposal parameters | Minter registration is generic; Position parameters are chosen when opening and constrained by `MintingHub` | A Module proposal fixes allocation, debt/collateral limits, price, durations, reward, reserve contribution, and risk premium |
| Equity shares | FPS uses a cubic bonding curve, 0.3% investment/redemption effects, minimum equity, time-weighted voting, and `kamikaze` vote destruction | KPS deposits/redemptions are pro rata; voting uses explicitly locked/delegated shares with a bounded age multiplier; no `kamikaze` mechanism |
| Reserve accounting | Assigned minter reserve and equity coexist in the Frankencoin reserve accounting | Assigned KUSD is held by each Position; EquityReserve separately tracks liquid KUSD, KPS, and KAS acquired by backstop |
| Savings interest governance | A holder commanding strictly more than 1% of current time-weighted votes may propose a rate; another qualified holder can overwrite it during the seven-day delay (older FPS1 used 2%) | Anyone may propose a Savings series, but veto requires the configured share of maximum weighted KPS supply; the TN10 fixture uses 20% |
| Savings interest payment | Lazy refresh calls `coverLoss`, and referrals may receive up to 25% of interest | Lazy refresh is paid from available EquityReserve KUSD; rates are disabled by default and separately governed |
| Time and state | EVM timestamps and account storage | DAA scores and explicitly chained UTXO state |

The most important remaining functional gaps before claiming close behavioral
parity are therefore Position adjustment/additional minting, per-Position
expiration purchases, partial Challenges, and the exact FPS/KPS equity and
voting economics. Some Frankencoin behavior—especially loss socialization—is
intentionally not copied because KUSD currently requires fully covered
settlement.

## Partial Challenges

Frankencoin lets a challenger select a collateral amount and permits partial
averting or bidding. KUSD currently moves the complete Position into one
`ChallengedPosition` lineage. This all-or-nothing design was selected because it
keeps the first UTXO implementation auditable: debt, assigned Reserve, KAS
collateral, and the Position minter do not have to be split across concurrent
successors.

This simplification has a cost. Challenging a large Position requires a full
collateral deposit, increasing the capital requirement for liquidators and
potentially weakening liveness.

The recommended path toward Frankencoin-like behavior is to allow one partial
Challenge at a time per Position:

```text
active Position + partial challenger deposit
    -> remaining active Position
    + challenged slice
    + Challenge
```

The transition must conserve the exact total KAS value and split debt and
assigned KUSD Reserve deterministically. Rounding must be specified so that no
debt, Reserve balance, or minting authority can be duplicated or discarded.
An avert transition must merge the exact slice back into the Position; Auction
settlement must burn only the challenged debt and preserve the remaining
Position. Restricting a Position to one challenged slice at a time is the
preferred first implementation: it provides Frankencoin-style partial
liquidation without the substantially larger state-space and double-challenge
risk of multiple concurrent slices.

## Parallel Governance proposals

Frankencoin can have multiple minter suggestions inside their application
periods at the same time. KUSD currently consumes and recreates a singleton
Governance UTXO when opening a proposal, so each Governor permits only one
active proposal. This was selected to make proposal provenance, fee escrow,
veto state, and successor authentication explicit without an assumed registry
or mutable global map.

Serialization limits governance throughput and lets one pending proposal delay
unrelated proposals. A closer Frankencoin model can be implemented without
making issuance itself concurrent:

```text
independent Proposal UTXO #1 --\
independent Proposal UTXO #2 ----> RootIssuance singleton at execution
independent Proposal UTXO #3 --/
```

Proposal creation would not consume Governance. Each Proposal would commit to
the Governance policy/template, immutable economic bounds, proposer, terms,
fee escrow, and DAA window. It could be vetoed or expire independently. A
successful execution would consume that Proposal and the current RootIssuance
successor; RootIssuance would atomically enforce remaining global capacity and
produce the next Root successor, Module, and minting authority. Consuming the
Proposal makes execution one-shot, while the Root keeps actual allocations
serialized and prevents concurrent over-allocation.

This change requires consensus tests for competing executions, replay,
duplicate fee refunds, simultaneous veto/execute races, Root-cap exhaustion,
and reorganization recovery. The same pattern can later be applied to Savings
proposals, although rate activations must still be ordered because there is one
canonical Savings rate series.

## Bounds that intentionally differ

Frankencoin enforces a small set of hard contract constraints, including a
maximum 100% risk premium, Reserve contribution between the fixed 2%
challenger reward and 100%, a minimum collateral reference value, minimum
initialization/application delays, and technical token/supply bounds. It does
not define a system-wide maximum liquidation price.

KUSD Governance currently commits a broader immutable policy envelope for
Module allocation, per-Position debt, minimum collateral, Module duration,
warning/Auction duration, risk premium, and Reserve contribution. These bounds
are useful safety rails for the prototype but are not exact Frankencoin parity:
Frankencoin relies more heavily on proposal review/veto and Position-level
rules. KUSD's numerical Testnet fixtures are listed in
[Economic parameters](ECONOMICS.md) and must be reviewed before mainnet.
