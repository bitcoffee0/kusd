# Testing

Run the complete local suite with:

```bash
cargo test --locked --all-targets
python3 -m py_compile scripts/*.py
```

The retained tests cover:

- compilation and stable circular template identities;
- chained Governance proposal, activation, veto, cancellation, and execution;
- immutable economic bounds and minimum collateral;
- remaining-term risk-premium rounding;
- chained open, repay, and close using produced outpoints;
- fraudulent withdrawal, underpayment, wrong beneficiary, wrong KAS value, and
  duplicated cross-template successors;
- Challenge warning, avert/activate race, decreasing Auction payment, full debt
  burn, and minimum backstop duration;
- Reserve deposit, redemption, accounting, and insufficient-liquidity failure;
- Savings proposal, activation, open, refresh, referral, and withdraw fraud;
- arithmetic boundaries, deterministic property cases, rounding, and overflow;
- indexer RPC evidence, multi-wallet roles, competing branches, and reorg rollback.

Network validation builders persist signed transaction probes. `tx-diagnose`
replays individual inputs with the same UTXO data and compute budgets used by
the builder preflight.
