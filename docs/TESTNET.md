# Current Testnet-10 deployment

This record identifies the clean KUSD stack deployed and validated on Kaspa
Testnet 10 on 2026-09-10. Testnet KAS and every asset below have no monetary
value. A source or constructor change produces new template identities and
requires a fresh deployment.

This deployment was compiled from the current source tree with SilverScript
1.0.0 commit `3ed973335b59269293564805cc2c58a14595ec03`. It includes the
uncapped positive liquidation-price policy and overflow-safe collateral-value
calculation covered by the current consensus suite.

## Covenant and asset identifiers

| Component | Identifier |
|---|---|
| KUSD Asset ID | `a2d81080bd74ab419f0bfea73b20c5154100520be15d68a190a5d1adf52f32b5` |
| RootIssuance | `5adf5c5bde359a4ac67f8a5cc14b570bac89e503a0e369b019acc470036eced6` |
| EquityReserve | `a07b07b3cb3d0658f258092e65f315837a8a2646c89cab5bbe1094adf0a7a4fd` |
| KPS Asset ID | `9afc6670400d5615205b2f8549ad64cfe91b4cf374ed72ab57eff7598460d9c0` |
| Governance | `5611bead5db547c6b0c75a79f604ba8cb3b9e745af8c71540eacd74b8d1f479e` |
| Bootstrap MintingModule | `b2ca093ae9178360878ec861df8867c4776467ec16fb7fc5170fc1e50f49670a` |
| Governed MintingModule | `c42f74584dbdebb7713f9128732a80c14075f5806ade5f37448205450f0fedb9` |
| Savings Governor | `4e58fe766af7cb032f9c66bf54d710c4de217674d98fbc8d6b2320240ea74cd3` |
| Savings Registry | `1e78dabd70e0d2d3a831bfa9034e76643a7631f66ef142d9953cec797f55e3ca` |
| Savings Controller | `2cae2b70fe94ea99b83c554c2ce5898afd1443d59c1f6af87b2c702851301d4b` |

The Module proposal used a KUSD fee escrow. Its successful execution refunded
the fee instead of crediting the Reserve, as required by the governance rules.

## Validation transactions

| Path | Terminal transaction ID | Result |
|---|---|---|
| Governed Module execution | `2d571d6b9b12d7f24e745cbb72ea4a825cd04566760773b77ac350624a324453` | Module and minter created; proposal fee refunded |
| Savings activation | `472af9468824ae3a2a3cda0f82d28bc7633d5bd872111f21c8d4342e0b331a92` | Savings enabled by its Governor |
| Repay and close | `680c64086774ae30d027dd745ad9d75d6496cb3d2ac270efefec1a652157c327` | 8 KUSD debt burned; 1,000 KAS returned |
| Savings refresh and withdraw | `d34343598556d997cd190012f637b76e7c413180ea9ab4293145010ace1120c5` | Interest paid from Reserve; account closed |
| Challenge Auction backstop | `67179c8b50079d4454027c5b4a8acc536faad2a224c3b4b018eff11fb441c678` | Full debt settlement; 1,000 KAS acquired by Reserve |
| Multi-wallet Auction settlement | `b4d7e2e4efdf24efe1bfe2200f96245a516b503baa397579726966bf555738e5` | Distinct bidder settled without Reserve backstop |

## Indexed final state

The checked RPC snapshot replayed 30 economic transactions and matched 27 live
outpoints by outpoint, full state-script address, and Covenant ID:

```text
KUSD supply                 1,500,000,000 atoms (15 KUSD)
Position debt               1,500,000,000 atoms (15 KUSD)
KPS supply                  1,000,000,000 atoms (10 KPS)
Reserve KUSD                  271,999,871 atoms
Reserve KAS collateral      100,000,000,000 sompi (1,000 KAS)
active proposals                         0
active Savings accounts                  0
active Challenges                        0
active Auctions                          0
```

This demonstrates acceptance and state consistency on Testnet 10. It is not a
security audit or evidence of mainnet readiness. The complete ignored local
manifest can be regenerated from the deployment files with
`scripts/build_index_manifest.py`.
