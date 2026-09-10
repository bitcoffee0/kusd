# KUSD — a UTXO-native stablecoin on Kaspa

KUSD is an experimental, overcollateralized USD stablecoin built for Kaspa
with SilverScript/Toccata covenants. It implements independent KAS-backed
Positions, one fungible KUSD Asset ID, parallel MintingModules, challenges,
Dutch auctions, a KPS equity reserve, veto-based governance, and optional
Savings.

> Experimental and unaudited. The current branch is configured exclusively for
> Kaspa Testnet 10. Do not use it with assets of real value.

## Design goals

- preserve every economic invariant directly in UTXO transitions;
- never accept a successor merely because it has the same Covenant ID;
- authenticate successor script, state, value, owner, and cross-contract IDs;
- keep each Position independent rather than maintaining a global position UTXO;
- fail closed when an Auction cannot fully cover debt;
- use integers only: sompi, 8-decimal KUSD/KPS atoms, ppm rates, and DAA time.

The economic architecture is inspired by
[Frankencoin](https://github.com/Frankencoin-ZCHF/Frankencoin). KUSD is an
independent Kaspa implementation, not a port or an affiliated project. See
[Frankencoin comparison](docs/FRANKENCOIN.md) for reused ideas and necessary
UTXO-specific differences.

## Components

```text
RootIssuance
  └─ Governance → ModuleProposal → MintingModule
                                      └─ Position
                                           └─ Challenge → Auction

KUSD Asset ID ───────────────┬─ user balances
                             ├─ Position-assigned reserve
                             ├─ EquityReserve ↔ KPS
                             └─ SavingsAccount
```

Savings is disabled at genesis and can only be enabled through its KPS-vetoed
Governor. Interest is transferred from EquityReserve when an account refreshes;
it is not continuously distributed and does not mint new KUSD.
The TN10 fixture requires 20% of maximum weighted KPS power to veto a Savings
proposal. This is deliberately not presented as a mainnet recommendation:
current Frankencoin qualification requires strictly more than 1% of its
time-weighted votes (older FPS1 used 2%).

## Reference test parameters

The deployment fixtures use `0.035 KUSD/KAS` as a reference liquidation price.
For example, `1,000 KAS` represents `35 KUSD` of reference collateral and backs
`15 KUSD` of gross debt. This is a deterministic Module parameter, not a live
price feed. The complete TN10 values, consensus bounds, units, and Frankencoin
reference points are documented in [Economic parameters](docs/ECONOMICS.md).
Every value must be independently reviewed and recalibrated before mainnet.

## Reproducible setup

- Rust `1.94.0` from `rust-toolchain.toml`;
- SilverScript `1.0.0`, commit `3ed973335b59269293564805cc2c58a14595ec03`;
- rusty-kaspa commit `a41a333b08848f41bf737b72592e463a6011b8ac`;
- `kaspa==2.0.2rc1` for network builders.

```bash
git clone https://github.com/kaspanet/silverscript.git vendor/silverscript
git -C vendor/silverscript checkout 3ed973335b59269293564805cc2c58a14595ec03
python3.12 -m venv .venv
.venv/bin/pip install -r requirements.txt
cargo test --locked --all-targets
```

Copy `.env.example` to `.env` and use a Testnet-10-only private key. Secrets,
transaction probes, manifests, role keys, and validation logs are ignored.

## Commands

```bash
# Complete genesis, Governance, Reserve/KPS, Module, Position, and Savings setup
.venv/bin/python -B scripts/deploy.py dry-run
.venv/bin/python -B scripts/deploy.py deploy
.venv/bin/python -B scripts/deploy.py verify

# Network lifecycles
.venv/bin/python -B scripts/repay_close.py repay-close-dry-run
.venv/bin/python -B scripts/repay_close.py repay-close
.venv/bin/python -B scripts/savings_cli.py savings-dry-run
.venv/bin/python -B scripts/savings_cli.py savings
.venv/bin/python -B scripts/challenge_cli.py dry-run
.venv/bin/python -B scripts/challenge_cli.py deploy
.venv/bin/python -B scripts/multiwallet_cli.py dry-run
.venv/bin/python -B scripts/multiwallet_cli.py deploy

# Build and verify the deployment index
.venv/bin/python -B scripts/build_index_manifest.py
cargo run --bin kusd -- index-validate --manifest kusd-index-manifest.local.json
cargo run --bin kusd -- index-rpc \
  --manifest kusd-index-manifest.local.json \
  --output kusd-index.local.json
```

Network workflows persist progress and can be resumed after an RPC disconnect.
The multi-wallet scenario uses distinct owner, challenger, and bidder keys.

## Documentation

- [Architecture and UTXO transitions](docs/ARCHITECTURE.md)
- [Economic parameters](docs/ECONOMICS.md)
- [Governance](docs/GOVERNANCE.md)
- [Savings](docs/SAVINGS.md)
- [Deployment and indexer](docs/DEPLOYMENT.md)
- [Current Testnet-10 deployment](docs/TESTNET.md)
- [Testing](docs/TESTING.md)
- [Security model and limitations](docs/SECURITY.md)
- [Frankencoin comparison](docs/FRANKENCOIN.md)

## License

[MIT](LICENSE), copyright © 2026 BitCoffee0.
