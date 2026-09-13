# Architecture

## Units

- `1 KAS = 100,000,000 sompi`.
- `1 KUSD = 100,000,000 KUSD atoms`.
- `1 KPS = 100,000,000 KPS atoms`.
- Rates and fractions use parts per million (`1,000,000 ppm = 100%`).
- Time uses DAA scores. No floating-point operation is used.

KCC does not currently encode display decimals in covenant state, so the eight
decimals are off-chain metadata.

## Template stack

`RootIssuance` creates one unique KUSD Asset ID and allocates bounded minting
capacity. After its irreversible handover, only an authenticated Governance
execution can create a MintingModule. Modules share the Asset ID but have
independent immutable terms and limits.

Every Position is its own covenant lineage. KAS collateral is the value of the
Position UTXO; debt and risk terms are serialized in its script state.

`EquityReserve` accounts for KUSD liquidity, KPS shares, and acquired KAS
collateral. `equity-reserve-base.sil` is a compilation bootstrap template used
to solve the circular Reserve/Auction template dependency. It is not a second
deployed reserve.

## Transition classes

| Operation | Class | Inputs | Required successors/effects |
|---|---|---|---|
| Root initialization | multi-contract | Root + funding | initialized Root + unique KUSD minter |
| Propose Module | multi-contract, N:M | Governance + proposer KUSD + KAS funding | Governance successor + Proposal + KUSD fee escrow owned by Proposal |
| Create Module | multi-contract, N:M | Governance + Proposal + Root + root minter + fee escrow | exact Governance/Root successors + Module + module minter + KUSD fee refund |
| Cancel Proposal | multi-contract, N:M | Governance + expired Proposal + fee escrow | cleared Governance + KAS deposit and KUSD fee returned to proposer |
| Veto Proposal | multi-contract, N:M | Governance + Proposal + fee escrow + Reserve + delegated KPS | cleared Governance, KAS deposit refund, fee retained by Reserve, unchanged delegation/KPS locks |
| Open Position | multi-contract, N:M | Module + module minter + KAS | exact Module successor, Position, Position minter, usable KUSD, assigned KUSD, Reserve fee |
| Repay | multi-contract, N:M | Position + user KUSD + assigned KUSD | reduced Position state and exact burns |
| Close | N:M | zero-debt Position + owner authorization | no Position successor; exact KAS returned to owner |
| Start Challenge | multi-contract | Position + challenger KAS | ChallengedPosition + full Challenge deposit |
| Avert | multi-contract | ChallengedPosition + Challenge + buyer KUSD | active Position + challenger payment |
| Activate Auction | 1:1 cross-template | mature Challenge | exact Auction successor with preserved state and value |
| Settle Auction | multi-contract, N:M | Auction + ChallengedPosition + KUSD payment + assigned KUSD + minter | full debt burn, reward, owner surplus, collateral to bidder, deposit to challenger |
| Reserve backstop | multi-contract, N:M | mature Auction + ChallengedPosition + Reserve + Reserve KUSD + assigned KUSD + minter | full debt burn, exact Reserve successors, reward, collateral acquired by Reserve |
| Savings refresh | multi-contract, N:M | Registry + Controller + Account + Reserve + KUSD UTXOs | exact interest transfer and state successors |

Each transition verifies input counts, output counts, template-derived scripts,
serialized state, KAS values, asset IDs, owners, and mint/burn conservation.
Covenant ID equality alone is never accepted as successor validation.
