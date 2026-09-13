# Economic parameters

## Status of the numbers

The values below are deliberately small Testnet-10 fixtures. They make every
path practical to exercise and are **not recommendations for mainnet**. Before
any production deployment, protocol developers, economic reviewers, and
security auditors must reconsider every amount, rate, DAA interval, capacity,
and voting threshold using observed Kaspa network behavior and market-liquidity
assumptions.

The design roles are inspired by Frankencoin, but the KUSD numbers are not
claimed to reproduce Frankencoin's production parameters. Frankencoin uses
seconds, 18-decimal ZCHF and arbitrary collateral tokens; KUSD uses DAA,
8-decimal KUSD/KPS atoms and KAS collateral. Direct numerical copying would be
misleading.

All bounds in the first table are immutable constructor commitments of the
Governance template. Governance checks proposed Modules against them, but
cannot rewrite the bounds in place. Changing them requires a deliberately new
protocol deployment and review.

## Governance bounds used by the TN10 fixture

| Module parameter | Consensus range | TN10 proposal | Unit |
|---|---:|---:|---|
| Module allocation | 1–10,000 | 500 | KUSD |
| Debt cap per Position | >0–2,000 | 20 | KUSD |
| Minimum collateral | 1–1,000,000 | 1 | KAS |
| Liquidation price | >0, no economic maximum | 0.03500000 | KUSD/KAS |
| Remaining Module duration | 100–126,144,000 | 31,536,000 | DAA |
| Challenge warning period | 10–2,592,000 | 3,600 | DAA |
| Auction duration | 10–2,592,000 | 3,600 | DAA |
| Challenger reward | >0–10% | 1% | ppm |
| Annual risk premium | 0–20% | 2% | ppm/year |
| Assigned Reserve contribution | 1–50% | 10% | ppm |

The genesis Root allocation is 10,000 KUSD. The bootstrap Module receives
1,000 KUSD and the governed test Module receives 500 KUSD. The demonstrated
Position locks 1,000 KAS, creates 15 KUSD of gross debt, and uses 20 KUSD as
its per-Position cap. At the fixed test reference of 0.035 KUSD/KAS, the
collateral reference value is 35 KUSD.

DAA values are consensus units, not wall-clock promises. Any approximate
duration depends on the observed DAA progression of the target network and
must be recalibrated before mainnet.

There is no protocol-level maximum liquidation price. This follows
Frankencoin's proposal-and-veto model and avoids forcing a migration solely
because KAS appreciates. A positive price must still fit the signed integer
representation. The collateral check compares against the exact mathematical
capacity without multiplying two potentially large `i64` values. The resulting
KUSD reference value must remain representable because Challenge and Auction
payments use that same integer amount. This is a technical bound on a concrete
Position, not a globally configured market-price ceiling. KPS reviewers must
veto economically unrealistic prices.

## Other TN10 policy fixtures

| Parameter | TN10 value | Unit / behavior |
|---|---:|---|
| Module proposal KUSD fee | 1 | KUSD; escrowed, returned on execute/cancel, retained by Reserve only after veto |
| Module proposal KAS deposit | 1 | KAS; returned on execute, cancel, or veto |
| Module voting delay | 100 | DAA |
| Module execution window | 10,000 | DAA |
| Module veto threshold | 2% | weighted KPS supply |
| Minimum KPS holding interval | 90 | DAA test fixture |
| Maximum KPS vote weight | 4× | bounded multiplier |
| Savings annual rate | 2% | ppm/year; Savings starts disabled |
| Savings interest delay | 50 | DAA |
| Savings maximum accrual claim | 10,000 | DAA per refresh |
| Savings referral share | 10% | percentage of interest, not principal |
| Savings veto threshold | 20% | maximum weighted KPS supply; with 4x maximum weight, 20% of raw KPS suffices only when all vetoing KPS have reached 4x |
| Savings voting delay | 100 | DAA |
| Savings execution window | 10,000 | DAA |
| Covenant output value / proposal deposit | 1 | KAS test fixture |

The deployment reserves a 10 KUSD initial equity deposit and mints 10 KPS in
the one-for-one empty-Reserve case. Later deposits and redemptions are pro rata.

For Savings veto, each locked delegation contributes `amount × weight`, while
the denominator is `totalKps × maxVoteWeight`. With the TN10 maximum weight of
4x, a fully mature 4x delegation needs 20% of raw KPS supply; a 2x delegation
would need 40%, and a 1x delegation 80%. This conservative denominator prevents
low participation from lowering the absolute veto requirement.

## Frankencoin reference points

For transparency, the comparison was checked against Frankencoin commit
[`7409532`](https://github.com/Frankencoin-ZCHF/Frankencoin/tree/7409532eac3ac77e8f7d4f2887bb538c0ee0c72d).
Its contracts, rather than its interface alone, enforce among other things:

- a 1,000 ZCHF minter/application fee and a deployed minimum application
  period documented as 14 days in `Frankencoin.sol`;
- a 1,000 ZCHF Position opening fee, a 2% challenger reward, risk premium no
  greater than 100%, and Reserve contribution between the challenger reward
  and 100% in `MintingHub.sol`;
- at least three days of Position initialization and at least 5,000 ZCHF of
  minimum-collateral reference value;
- a 1,000 ZCHF minimum equity level, a 90-day FPS holding period, and a
  three-day Savings interest delay.

KUSD intentionally lowers amounts and delays on TN10. It also separates the
Challenge warning period from the Auction duration and applies the proposal-fee
policy selected for this prototype. These are adaptations, not assertions that
the systems have identical security or economics.

## MintingModule terms

Each proposal commits immutable values for:

- module allocation and per-Position debt cap;
- minimum Position collateral;
- liquidation price in KUSD atoms per KAS;
- module expiration DAA;
- annual risk premium in ppm;
- assigned Reserve contribution in ppm;
- Challenge warning period and Auction duration;
- challenger reward in ppm.

Governance enforces immutable global bounds when accepting a proposal. Those
bounds are constructor commitments, not interface-only checks.

Initial collateral is chosen when each Position opens and must satisfy the
Module minimum. It is deliberately not a Module-wide proposal field because a
Module proposal does not create a Position in the UTXO design.

## Opening a Position

The collateral constraint is:

```text
grossDebtAtoms <= floor(collateralSompi × liquidationPrice / 100,000,000)
```

The risk premium is charged upfront according to remaining Module duration:

```text
effectiveRiskPpm = floor(annualRiskPpm × remainingDaa / daaPerYear)
riskFee          = floor(grossDebt × effectiveRiskPpm / 1,000,000)
assignedReserve  = floor(grossDebt × reserveContributionPpm / 1,000,000)
userAmount       = grossDebt - riskFee - assignedReserve
```

The risk fee belongs to EquityReserve. Assigned Reserve remains owned by the
Position covenant and is burned as part of repayment or Auction settlement.

## Auction price

Auction settlement uses a linearly decreasing price between the collateral
reference value and the no-deficit floor:

```text
floorPayment = debt - assignedReserve + challengerReward
startPayment = max(collateralReferenceValue, floorPayment)
payment(t)   = startPayment
             - floor((startPayment - floorPayment) × elapsedDaa / auctionDurationDaa)
```

After the Auction duration, Reserve may backstop only if it can pay the full
floor amount. Otherwise the Auction remains open: the protocol never hides or
socializes an uncovered deficit.

## KPS equity

Reserve deposits mint KPS pro rata; redemption burns KPS and pays pro-rata KUSD
and KAS. Wide `mulDivFloor` arithmetic prevents intermediate i64 overflow.

The proposal fee is locked as a KUSD UTXO owned by the Proposal Covenant. It is
refunded on successful execution or permissionless cancellation after expiry.
Only veto transfers it permanently to EquityReserve, which deters proposals
that KPS holders reject without taxing accepted or harmlessly expired ones.
