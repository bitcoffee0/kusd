use kaspa_addresses::{Address, Prefix, Version};
use kaspa_consensus_core::hashing::sighash::{
    SigHashReusedValuesUnsync, calc_schnorr_signature_hash,
};
use kaspa_consensus_core::hashing::sighash_type::SIG_HASH_ALL;
use kaspa_consensus_core::tx::{
    CovenantBinding, MutableTransaction, PopulatedTransaction, Transaction, TransactionId,
    TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry, VerifiableTransaction,
};
use kaspa_consensus_core::{Hash, hashing};
use kaspa_kusd::silverscript::{
    CompileOptions, CompiledContract, CovenantDeclCallOptions, compile_contract, struct_object,
};
use kaspa_txscript::caches::Cache;
use kaspa_txscript::covenants::CovenantsContext;
use kaspa_txscript::opcodes::codes::OpTrue;
use kaspa_txscript::script_builder::ScriptBuilder;
use kaspa_txscript::{
    EngineCtx, EngineFlags, TxScriptEngine, pay_to_address_script, pay_to_script_hash_script,
};
use secp256k1::{Keypair, Secp256k1, SecretKey};
use silverscript_lang::ast::{Expr, parse_type_ref};

const ASSET: Hash = Hash::from_bytes(*b"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA");
const MODULE: Hash = Hash::from_bytes(*b"MMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMM");
const RESERVE: Hash = Hash::from_bytes(*b"EEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEE");
const CHALLENGE: Hash = Hash::from_bytes(*b"HHHHHHHHHHHHHHHHHHHHHHHHHHHHHHHH");
const DEBT: i64 = 1_500_000_000;
const ASSIGNED: i64 = 150_000_000;
const FEE: i64 = 30_000_000;
const USABLE: i64 = DEBT - ASSIGNED - FEE;
const USER_BURN: i64 = DEBT - ASSIGNED;
const COLLATERAL: u64 = 100_000_000_000;
const YEAR_DAA: i64 = 31_536_000;

fn owner() -> Keypair {
    Keypair::from_secret_key(
        &Secp256k1::new(),
        &SecretKey::from_slice(&[23; 32]).unwrap(),
    )
}

#[test]
fn auction_burns_assigned_reserve_before_external_payment() {
    let kcc_src = std::fs::read_to_string("contracts/kcc20.sil").unwrap();
    let auction_src = std::fs::read_to_string("contracts/auction.sil").unwrap();
    let challenger = Keypair::from_secret_key(
        &Secp256k1::new(),
        &SecretKey::from_slice(&[24; 32]).unwrap(),
    );
    let bidder = Keypair::from_secret_key(
        &Secp256k1::new(),
        &SecretKey::from_slice(&[25; 32]).unwrap(),
    );
    let challenger_pk = challenger.x_only_public_key().0.serialize().to_vec();
    let bidder_pk = bidder.x_only_public_key().0.serialize().to_vec();
    let position_owner = vec![9; 32];
    let pid = Hash::from_bytes([0x71; 32]);
    let probe = kcc(&kcc_src, vec![0; 32], 0, 2, true);
    let (kp, ks, kh) = parts(&probe);
    let auction = compile_contract(
        &auction_src,
        &[
            Expr::bytes(position_owner.clone()),
            Expr::bytes(challenger_pk.clone()),
            Expr::bytes(pid.as_bytes().to_vec()),
            Expr::bytes(ASSET.as_bytes().to_vec()),
            Expr::int(DEBT),
            Expr::int(ASSIGNED),
            Expr::bytes(RESERVE.as_bytes().to_vec()),
            Expr::int(3_500_000),
            Expr::int(10_000),
            Expr::int(3_600),
            Expr::int(COLLATERAL as i64),
            Expr::int(kp.len() as i64),
            Expr::int(ks.len() as i64),
            Expr::bytes(kh),
            Expr::int(0),
            Expr::int(0),
            Expr::bytes(vec![0; 32]),
        ],
        CompileOptions::default(),
    )
    .unwrap();
    let reward = 15_000_000;
    // At the reference price of 0.035 KUSD/KAS, 1,000 KAS starts the
    // Dutch Auction at exactly 35 KUSD before decaying toward its floor.
    let external = 3_500_000_000;
    let owner_surplus = external - (DEBT - ASSIGNED + reward);
    let minter = kcc(&kcc_src, pid.as_bytes().to_vec(), 0, 2, true);
    let payment = kcc(&kcc_src, bidder_pk.clone(), external, 0, false);
    let assigned = kcc(&kcc_src, pid.as_bytes().to_vec(), ASSIGNED, 2, false);
    let reward_token = kcc(&kcc_src, challenger_pk.clone(), reward, 0, false);
    let surplus_token = kcc(&kcc_src, position_owner.clone(), owner_surplus, 0, false);
    let outputs = vec![
        out(&reward_token, 1_000, 2, ASSET),
        out(&surplus_token, 1_000, 2, ASSET),
        TransactionOutput {
            value: COLLATERAL,
            script_public_key: pay_to_address_script(&Address::new(
                Prefix::Testnet,
                Version::PubKey,
                &bidder_pk,
            )),
            covenant: None,
        },
        TransactionOutput {
            value: COLLATERAL,
            script_public_key: pay_to_address_script(&Address::new(
                Prefix::Testnet,
                Version::PubKey,
                &challenger_pk,
            )),
            covenant: None,
        },
    ];
    let ops = (0..5)
        .map(|i| TransactionOutpoint {
            transaction_id: TransactionId::from_bytes([120 + i; 32]),
            index: 0,
        })
        .collect::<Vec<_>>();
    let entries = vec![
        UtxoEntry::new(
            COLLATERAL,
            pay_to_script_hash_script(&auction.bytecode),
            0,
            false,
            Some(CHALLENGE),
        ),
        UtxoEntry::new(
            COLLATERAL,
            kaspa_consensus_core::tx::ScriptPublicKey::new(0, vec![OpTrue].into()),
            0,
            false,
            Some(pid),
        ),
        UtxoEntry::new(
            1_000,
            pay_to_script_hash_script(&minter.bytecode),
            0,
            false,
            Some(ASSET),
        ),
        UtxoEntry::new(
            1_000,
            pay_to_script_hash_script(&payment.bytecode),
            0,
            false,
            Some(ASSET),
        ),
        UtxoEntry::new(
            1_000,
            pay_to_script_hash_script(&assigned.bytecode),
            0,
            false,
            Some(ASSET),
        ),
    ];
    let unsigned = Transaction::new(
        1,
        ops.iter().map(|o| inp(*o, vec![])).collect(),
        outputs.clone(),
        0,
        Default::default(),
        0,
        vec![],
    );
    let bidder_sig = sign(unsigned, entries.clone(), 3, &bidder);
    let tx = Transaction::new(
        1,
        vec![
            inp(
                ops[0],
                call(
                    &auction,
                    "settlePolicy",
                    vec![
                        Expr::array(parse_type_ref("State[]").unwrap(), vec![]),
                        Expr::bytes(bidder_pk.clone()),
                        Expr::byte(2),
                        Expr::byte(3),
                        token("TokenState", challenger_pk.clone(), 0, reward, false),
                        token(
                            "TokenState",
                            position_owner.clone(),
                            0,
                            owner_surplus,
                            false,
                        ),
                    ],
                    true,
                ),
            ),
            inp(ops[1], vec![]),
            inp(
                ops[2],
                call(
                    &minter,
                    "transferPolicy",
                    vec![
                        tokens(vec![
                            (challenger_pk.clone(), 0, reward, false),
                            (position_owner.clone(), 0, owner_surplus, false),
                        ]),
                        Expr::bytes(vec![0; 65]),
                        Expr::byte(1),
                    ],
                    true,
                ),
            ),
            inp(
                ops[3],
                call(
                    &payment,
                    "transferPolicy",
                    vec![Expr::bytes(bidder_sig), Expr::byte(1)],
                    false,
                ),
            ),
            inp(
                ops[4],
                call(
                    &assigned,
                    "transferPolicy",
                    vec![Expr::bytes(vec![0; 65]), Expr::byte(1)],
                    false,
                ),
            ),
        ],
        outputs.clone(),
        0,
        Default::default(),
        0,
        vec![],
    );
    for i in [0usize, 2, 3, 4] {
        execute(&tx, entries.clone(), i).unwrap_or_else(|e| panic!("auction input {i}: {e}"));
    }

    // Halfway through the Auction, exactly half of the premium above the
    // fully-funded settlement floor has decayed.
    let halfway_external = (external + (DEBT - ASSIGNED + reward)) / 2;
    let halfway_surplus = halfway_external - (DEBT - ASSIGNED + reward);
    let halfway_payment = kcc(&kcc_src, bidder_pk.clone(), halfway_external, 0, false);
    let halfway_surplus_token = kcc(&kcc_src, position_owner.clone(), halfway_surplus, 0, false);
    let mut halfway_outputs = outputs.clone();
    halfway_outputs[1].script_public_key =
        pay_to_script_hash_script(&halfway_surplus_token.bytecode);
    let mut halfway_entries = entries.clone();
    halfway_entries[3].script_public_key = pay_to_script_hash_script(&halfway_payment.bytecode);
    let halfway_unsigned = Transaction::new(
        1,
        ops.iter().map(|o| inp(*o, vec![])).collect(),
        halfway_outputs.clone(),
        1_800,
        Default::default(),
        0,
        vec![],
    );
    let halfway_signature = sign(halfway_unsigned, halfway_entries.clone(), 3, &bidder);
    let halfway = Transaction::new(
        1,
        vec![
            inp(
                ops[0],
                call(
                    &auction,
                    "settlePolicy",
                    vec![
                        Expr::array(parse_type_ref("State[]").unwrap(), vec![]),
                        Expr::bytes(bidder_pk.clone()),
                        Expr::byte(2),
                        Expr::byte(3),
                        token("TokenState", challenger_pk.clone(), 0, reward, false),
                        token(
                            "TokenState",
                            position_owner.clone(),
                            0,
                            halfway_surplus,
                            false,
                        ),
                    ],
                    true,
                ),
            ),
            inp(ops[1], vec![]),
            inp(
                ops[2],
                call(
                    &minter,
                    "transferPolicy",
                    vec![
                        tokens(vec![
                            (challenger_pk.clone(), 0, reward, false),
                            (position_owner.clone(), 0, halfway_surplus, false),
                        ]),
                        Expr::bytes(vec![0; 65]),
                        Expr::byte(1),
                    ],
                    true,
                ),
            ),
            inp(
                ops[3],
                call(
                    &halfway_payment,
                    "transferPolicy",
                    vec![Expr::bytes(halfway_signature), Expr::byte(1)],
                    false,
                ),
            ),
            inp(
                ops[4],
                call(
                    &assigned,
                    "transferPolicy",
                    vec![Expr::bytes(vec![0; 65]), Expr::byte(1)],
                    false,
                ),
            ),
        ],
        halfway_outputs,
        1_800,
        Default::default(),
        0,
        vec![],
    );
    for index in [0usize, 2, 3, 4] {
        execute(&halfway, halfway_entries.clone(), index)
            .unwrap_or_else(|e| panic!("halfway Auction input {index}: {e}"));
    }

    let mut after_auction = halfway.clone();
    after_auction.lock_time = 3_601;
    assert!(execute(&after_auction, halfway_entries.clone(), 0).is_err());

    let mut under = entries.clone();
    under[3] = UtxoEntry::new(
        1_000,
        pay_to_script_hash_script(
            &kcc(&kcc_src, bidder_pk.clone(), external - 1, 0, false).bytecode,
        ),
        0,
        false,
        Some(ASSET),
    );
    assert!(execute(&tx, under, 0).is_err());
    let mut stolen = entries.clone();
    stolen[4] = UtxoEntry::new(
        1_000,
        pay_to_script_hash_script(
            &kcc(&kcc_src, RESERVE.as_bytes().to_vec(), ASSIGNED, 2, false).bytecode,
        ),
        0,
        false,
        Some(ASSET),
    );
    assert!(execute(&tx, stolen, 0).is_err());
    let mut wrong_reward = tx.clone();
    wrong_reward.outputs[0].script_public_key =
        pay_to_script_hash_script(&kcc(&kcc_src, challenger_pk, reward - 1, 0, false).bytecode);
    assert!(execute(&wrong_reward, entries.clone(), 0).is_err());
    for idx in [2usize, 3] {
        let mut wrong = tx.clone();
        wrong.outputs[idx].value -= 1;
        assert!(execute(&wrong, entries.clone(), 0).is_err());
    }
}

#[test]
fn challenge_activation_preserves_assigned_reserve_and_reserve_id() {
    let kcc_src = std::fs::read_to_string("contracts/kcc20.sil").unwrap();
    let auction_src = std::fs::read_to_string("contracts/auction.sil").unwrap();
    let challenge_src = std::fs::read_to_string("contracts/challenge.sil").unwrap();
    let challenger_pk = Keypair::from_secret_key(
        &Secp256k1::new(),
        &SecretKey::from_slice(&[26; 32]).unwrap(),
    )
    .x_only_public_key()
    .0
    .serialize()
    .to_vec();
    let position_owner = vec![10; 32];
    let pid = Hash::from_bytes([0x72; 32]);
    let probe = kcc(&kcc_src, vec![0; 32], 0, 2, true);
    let (kp, ks, kh) = parts(&probe);
    let auction_args = |assigned| {
        vec![
            Expr::bytes(position_owner.clone()),
            Expr::bytes(challenger_pk.clone()),
            Expr::bytes(pid.as_bytes().to_vec()),
            Expr::bytes(ASSET.as_bytes().to_vec()),
            Expr::int(DEBT),
            Expr::int(assigned),
            Expr::bytes(RESERVE.as_bytes().to_vec()),
            Expr::int(3_500_000),
            Expr::int(10_000),
            Expr::int(3_600),
            Expr::int(COLLATERAL as i64),
            Expr::int(kp.len() as i64),
            Expr::int(ks.len() as i64),
            Expr::bytes(kh.clone()),
            Expr::int(0),
            Expr::int(0),
            Expr::bytes(vec![0; 32]),
        ]
    };
    let auction = compile_contract(
        &auction_src,
        &auction_args(ASSIGNED),
        CompileOptions::default(),
    )
    .unwrap();
    let forged_auction = compile_contract(
        &auction_src,
        &auction_args(ASSIGNED - 1),
        CompileOptions::default(),
    )
    .unwrap();
    let (ap, as_, ah) = parts(&auction);
    let challenge = compile_contract(
        &challenge_src,
        &[
            Expr::bytes(position_owner.clone()),
            Expr::bytes(challenger_pk.clone()),
            Expr::bytes(pid.as_bytes().to_vec()),
            Expr::bytes(ASSET.as_bytes().to_vec()),
            Expr::int(DEBT),
            Expr::int(ASSIGNED),
            Expr::bytes(RESERVE.as_bytes().to_vec()),
            Expr::int(3_500_000),
            Expr::int(10_000),
            Expr::int(3_600),
            Expr::int(3_600),
            Expr::int(COLLATERAL as i64),
            Expr::int(kp.len() as i64),
            Expr::int(ks.len() as i64),
            Expr::bytes(kh.clone()),
            Expr::dynamic_bytes(ap),
            Expr::dynamic_bytes(as_),
            Expr::bytes(ah),
        ],
        CompileOptions::default(),
    )
    .unwrap();
    let auction_state = |assigned| {
        struct_object(
            "AuctionState",
            vec![
                ("positionOwner", Expr::bytes(position_owner.clone())),
                ("challenger", Expr::bytes(challenger_pk.clone())),
                ("positionId", Expr::bytes(pid.as_bytes().to_vec())),
                ("assetId", Expr::bytes(ASSET.as_bytes().to_vec())),
                ("debt", Expr::int(DEBT)),
                ("assignedReserve", Expr::int(assigned)),
                ("reserveId", Expr::bytes(RESERVE.as_bytes().to_vec())),
                ("liquidationPrice", Expr::int(3_500_000)),
                ("rewardPpm", Expr::int(10_000)),
                ("auctionDurationDaa", Expr::int(3_600)),
                ("collateralSompi", Expr::int(COLLATERAL as i64)),
                ("kccPrefixLen", Expr::int(kp.len() as i64)),
                ("kccSuffixLen", Expr::int(ks.len() as i64)),
                ("kccTemplateHash", Expr::bytes(kh.clone())),
            ],
        )
    };
    let flags = EngineFlags {
        covenants_enabled: true,
        sigop_script_units: 0.into(),
    };
    let build = |sequence: u64, assigned: i64, output_contract: &CompiledContract<'_>| {
        let mut script = challenge
            .build_sig_script(
                "activatePolicy",
                vec![Expr::byte(0), auction_state(assigned)],
            )
            .unwrap();
        script.extend_from_slice(
            &ScriptBuilder::with_flags(flags)
                .add_data(&challenge.bytecode)
                .unwrap()
                .drain(),
        );
        Transaction::new(
            1,
            vec![TransactionInput::new_with_compute_budget(
                TransactionOutpoint {
                    transaction_id: TransactionId::from_bytes([130; 32]),
                    index: 0,
                },
                script,
                sequence,
                65_535,
            )],
            vec![out(output_contract, COLLATERAL, 0, CHALLENGE)],
            0,
            Default::default(),
            0,
            vec![],
        )
    };
    let entries = vec![UtxoEntry::new(
        COLLATERAL,
        pay_to_script_hash_script(&challenge.bytecode),
        0,
        false,
        Some(CHALLENGE),
    )];
    assert!(execute(&build(3_599, ASSIGNED, &auction), entries.clone(), 0).is_err());
    execute(&build(3_600, ASSIGNED, &auction), entries.clone(), 0)
        .expect("mature base protocol challenge");
    assert!(execute(&build(3_600, ASSIGNED - 1, &forged_auction), entries, 0).is_err());
}
fn parts(c: &CompiledContract<'_>) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let l = c.state_layout;
    (
        c.bytecode[..l.start].to_vec(),
        c.bytecode[l.start + l.len..].to_vec(),
        c.template_hash().to_vec(),
    )
}
fn kcc<'a>(
    s: &'a str,
    owner: Vec<u8>,
    amount: i64,
    kind: u8,
    minter: bool,
) -> CompiledContract<'a> {
    compile_contract(
        s,
        &[
            Expr::bytes(owner),
            Expr::int(amount),
            Expr::byte(kind),
            Expr::bool(minter),
            Expr::int(8),
            Expr::int(8),
        ],
        CompileOptions::default(),
    )
    .unwrap()
}
fn position<'a>(
    s: &'a str,
    pk: Vec<u8>,
    id: Hash,
    debt: i64,
    assigned: i64,
    kp: &[u8],
    ks: &[u8],
    kh: &[u8],
) -> CompiledContract<'a> {
    position_with_price(s, pk, id, debt, assigned, kp, ks, kh, 3_500_000)
}
fn position_with_price<'a>(
    s: &'a str,
    pk: Vec<u8>,
    id: Hash,
    debt: i64,
    assigned: i64,
    kp: &[u8],
    ks: &[u8],
    kh: &[u8],
    liquidation_price: i64,
) -> CompiledContract<'a> {
    let build = |position_prefix_len: i64, position_suffix_len: i64, position_hash: Vec<u8>| {
        compile_contract(
            s,
            &[
                Expr::bytes(pk.clone()),
                Expr::bytes(ASSET.as_bytes().to_vec()),
                Expr::int(debt),
                Expr::int(assigned),
                Expr::bytes(RESERVE.as_bytes().to_vec()),
                Expr::int(100_000),
                Expr::int(liquidation_price),
                Expr::int(3_600),
                Expr::int(3_600),
                Expr::int(10_000),
                Expr::bytes(id.as_bytes().to_vec()),
                Expr::int(kp.len() as i64),
                Expr::int(ks.len() as i64),
                Expr::bytes(kh.to_vec()),
                Expr::int(position_prefix_len),
                Expr::int(position_suffix_len),
                Expr::bytes(position_hash),
                Expr::dynamic_bytes(vec![]),
                Expr::dynamic_bytes(vec![]),
                Expr::bytes(vec![0; 32]),
                Expr::dynamic_bytes(vec![]),
                Expr::dynamic_bytes(vec![]),
                Expr::bytes(vec![0; 32]),
            ],
            CompileOptions::default(),
        )
        .unwrap()
    };
    let probe = build(0, 0, vec![0; 32]);
    let layout = probe.state_layout;
    build(
        layout.start as i64,
        (probe.bytecode.len() - layout.start - layout.len) as i64,
        probe.template_hash().to_vec(),
    )
}
fn module_with_price<'a>(
    s: &'a str,
    remaining: i64,
    nonce: i64,
    kp: &[u8],
    ks: &[u8],
    kh: &[u8],
    ph: &[u8],
    liquidation_price: i64,
) -> CompiledContract<'a> {
    compile_contract(
        s,
        &[
            Expr::bytes(ASSET.as_bytes().to_vec()),
            Expr::int(remaining),
            Expr::int(2_000_000_000),
            Expr::int(1_000_000_000),
            Expr::int(liquidation_price),
            Expr::int(YEAR_DAA),
            Expr::int(3_600),
            Expr::int(3_600),
            Expr::int(10_000),
            Expr::int(nonce),
            Expr::bytes(RESERVE.as_bytes().to_vec()),
            Expr::int(100_000),
            Expr::int(20_000),
            Expr::int(kp.len() as i64),
            Expr::int(ks.len() as i64),
            Expr::bytes(kh.to_vec()),
            Expr::bytes(ph.to_vec()),
        ],
        CompileOptions::default(),
    )
    .unwrap()
}
fn token(name: &'static str, owner: Vec<u8>, kind: u8, amount: i64, minter: bool) -> Expr<'static> {
    struct_object(
        name,
        vec![
            ("ownerIdentifier", Expr::bytes(owner)),
            ("identifierType", Expr::byte(kind)),
            ("amount", Expr::int(amount)),
            ("isMinter", Expr::bool(minter)),
        ],
    )
}
fn tokens(v: Vec<(Vec<u8>, u8, i64, bool)>) -> Expr<'static> {
    Expr::array(
        parse_type_ref("State[]").unwrap(),
        v.into_iter()
            .map(|(o, k, a, m)| token("State", o, k, a, m))
            .collect(),
    )
}
fn pos_state_with_template(
    name: &'static str,
    pk: Vec<u8>,
    debt: i64,
    assigned: i64,
    kp: &[u8],
    ks: &[u8],
    kh: &[u8],
    pp: &[u8],
    ps: &[u8],
    ph: &[u8],
) -> Expr<'static> {
    pos_state_with_template_and_price(name, pk, debt, assigned, kp, ks, kh, pp, ps, ph, 3_500_000)
}
fn pos_state_with_template_and_price(
    name: &'static str,
    pk: Vec<u8>,
    debt: i64,
    assigned: i64,
    kp: &[u8],
    ks: &[u8],
    kh: &[u8],
    pp: &[u8],
    ps: &[u8],
    ph: &[u8],
    liquidation_price: i64,
) -> Expr<'static> {
    struct_object(
        name,
        vec![
            ("owner", Expr::bytes(pk)),
            ("assetId", Expr::bytes(ASSET.as_bytes().to_vec())),
            ("debt", Expr::int(debt)),
            ("assignedReserve", Expr::int(assigned)),
            ("reserveId", Expr::bytes(RESERVE.as_bytes().to_vec())),
            ("reserveContributionPpm", Expr::int(100_000)),
            ("liquidationPrice", Expr::int(liquidation_price)),
            ("challengePeriodDaa", Expr::int(3_600)),
            ("auctionDurationDaa", Expr::int(3_600)),
            ("challengeRewardPpm", Expr::int(10_000)),
            ("challengeId", Expr::bytes(vec![0; 32])),
            ("kccPrefixLen", Expr::int(kp.len() as i64)),
            ("kccSuffixLen", Expr::int(ks.len() as i64)),
            ("kccTemplateHash", Expr::bytes(kh.to_vec())),
            ("positionPrefixLen", Expr::int(pp.len() as i64)),
            ("positionSuffixLen", Expr::int(ps.len() as i64)),
            ("positionTemplateHash", Expr::bytes(ph.to_vec())),
        ],
    )
}
fn mod_state_with_price(remaining: i64, nonce: i64, liquidation_price: i64) -> Expr<'static> {
    struct_object(
        "State",
        vec![
            ("assetId", Expr::bytes(ASSET.as_bytes().to_vec())),
            ("remainingMint", Expr::int(remaining)),
            ("maxDebtPerPosition", Expr::int(2_000_000_000)),
            ("minimumCollateralSompi", Expr::int(1_000_000_000)),
            ("liquidationPrice", Expr::int(liquidation_price)),
            ("expirationDaa", Expr::int(YEAR_DAA)),
            ("challengePeriodDaa", Expr::int(3_600)),
            ("auctionDurationDaa", Expr::int(3_600)),
            ("challengeRewardPpm", Expr::int(10_000)),
            ("positionNonce", Expr::int(nonce)),
            ("reserveId", Expr::bytes(RESERVE.as_bytes().to_vec())),
            ("reserveContributionPpm", Expr::int(100_000)),
            ("riskPremiumPpm", Expr::int(20_000)),
        ],
    )
}
fn call(c: &CompiledContract<'_>, policy: &str, args: Vec<Expr<'_>>, leader: bool) -> Vec<u8> {
    let flags = EngineFlags {
        covenants_enabled: true,
        sigop_script_units: 0.into(),
    };
    let mut x = c
        .build_sig_script_for_covenant_decl(
            policy,
            args,
            CovenantDeclCallOptions { is_leader: leader },
        )
        .unwrap();
    x.extend_from_slice(
        &ScriptBuilder::with_flags(flags)
            .add_data(&c.bytecode)
            .unwrap()
            .drain(),
    );
    x
}
fn inp(outpoint: TransactionOutpoint, script: Vec<u8>) -> TransactionInput {
    TransactionInput::new_with_compute_budget(outpoint, script, 0, 65_535)
}
fn out(c: &CompiledContract<'_>, value: u64, auth: u16, id: Hash) -> TransactionOutput {
    TransactionOutput {
        value,
        script_public_key: pay_to_script_hash_script(&c.bytecode),
        covenant: Some(CovenantBinding {
            authorizing_input: auth,
            covenant_id: id,
        }),
    }
}
fn entry(o: &TransactionOutput, id: Hash) -> UtxoEntry {
    UtxoEntry::new(o.value, o.script_public_key.clone(), 0, false, Some(id))
}
fn execute(tx: &Transaction, entries: Vec<UtxoEntry>, idx: usize) -> Result<(), String> {
    let reused = SigHashReusedValuesUnsync::new();
    let cache = Cache::new(10_000);
    let p = PopulatedTransaction::new(tx, entries);
    let cov = CovenantsContext::from_tx(&p).map_err(|e| e.to_string())?;
    let mut vm = TxScriptEngine::from_transaction_input(
        &p,
        &tx.inputs[idx],
        idx,
        p.utxo(idx).unwrap(),
        EngineCtx::new(&cache)
            .with_reused(&reused)
            .with_covenants_ctx(&cov),
        EngineFlags {
            covenants_enabled: true,
            sigop_script_units: 0.into(),
        },
    );
    vm.execute().map_err(|e| format!("{e:?}"))
}
fn sign(tx: Transaction, entries: Vec<UtxoEntry>, idx: usize, key: &Keypair) -> Vec<u8> {
    let m = MutableTransaction::with_entries(tx, entries);
    let reused = SigHashReusedValuesUnsync::new();
    let h = calc_schnorr_signature_hash(&m.as_verifiable(), idx, SIG_HASH_ALL, &reused);
    let msg = secp256k1::Message::from_digest_slice(h.as_bytes().as_slice()).unwrap();
    let mut s = key.sign_schnorr(msg).as_ref().to_vec();
    s.push(SIG_HASH_ALL.to_u8());
    s
}

struct Opened<'a> {
    tx: Transaction,
    entries: Vec<UtxoEntry>,
    position: CompiledContract<'a>,
    zero: CompiledContract<'a>,
    minter: CompiledContract<'a>,
    user: CompiledContract<'a>,
    assigned: CompiledContract<'a>,
    pk: Vec<u8>,
    key: Keypair,
}

fn build_open<'a>(kcc_src: &'a str, pos_src: &'a str, module_src: &'a str) -> Opened<'a> {
    build_open_at(kcc_src, pos_src, module_src, 0)
}

fn build_open_at<'a>(
    kcc_src: &'a str,
    pos_src: &'a str,
    module_src: &'a str,
    current_daa: i64,
) -> Opened<'a> {
    build_open_at_price(kcc_src, pos_src, module_src, current_daa, 3_500_000)
}

fn build_open_at_price<'a>(
    kcc_src: &'a str,
    pos_src: &'a str,
    module_src: &'a str,
    current_daa: i64,
    liquidation_price: i64,
) -> Opened<'a> {
    let key = owner();
    let pk = key.x_only_public_key().0.serialize().to_vec();
    let current_fee_ppm = 20_000 * (YEAR_DAA - current_daa) / YEAR_DAA;
    let fee_amount = DEBT * current_fee_ppm / 1_000_000;
    let usable_amount = DEBT - ASSIGNED - fee_amount;
    let probe = kcc(kcc_src, vec![0; 32], 0, 2, true);
    let (kp, ks, kh) = parts(&probe);
    let position_contract = position_with_price(
        pos_src,
        pk.clone(),
        Hash::from_bytes([0; 32]),
        DEBT,
        ASSIGNED,
        &kp,
        &ks,
        &kh,
        liquidation_price,
    );
    let (pp, ps, ph) = parts(&position_contract);
    let old_module = module_with_price(
        module_src,
        3_000_000_000,
        0,
        &kp,
        &ks,
        &kh,
        &ph,
        liquidation_price,
    );
    let next_module = module_with_price(
        module_src,
        1_500_000_000,
        1,
        &kp,
        &ks,
        &kh,
        &ph,
        liquidation_price,
    );
    let old_minter = kcc(kcc_src, MODULE.as_bytes().to_vec(), 0, 2, true);
    let funding = TransactionOutpoint {
        transaction_id: TransactionId::from_bytes([90; 32]),
        index: 0,
    };
    let unbound = TransactionOutput {
        value: COLLATERAL,
        script_public_key: pay_to_script_hash_script(&position_contract.bytecode),
        covenant: None,
    };
    let pid = hashing::covenant_id::covenant_id(funding, std::iter::once((6, &unbound)));
    let minter = kcc(kcc_src, pid.as_bytes().to_vec(), 0, 2, true);
    let user = kcc(kcc_src, pk.clone(), usable_amount, 0, false);
    let assigned = kcc(kcc_src, pid.as_bytes().to_vec(), ASSIGNED, 2, false);
    let fee = kcc(kcc_src, RESERVE.as_bytes().to_vec(), fee_amount, 2, false);
    let module_minter = kcc(kcc_src, MODULE.as_bytes().to_vec(), 0, 2, true);
    let zero = position(
        pos_src,
        pk.clone(),
        Hash::from_bytes([0; 32]),
        0,
        0,
        &kp,
        &ks,
        &kh,
    );
    let outputs = vec![
        out(&module_minter, 1000, 0, ASSET),
        out(&minter, 1000, 0, ASSET),
        out(&user, 1000, 0, ASSET),
        out(&assigned, 1000, 0, ASSET),
        out(&fee, 1000, 0, ASSET),
        out(&next_module, 1000, 1, MODULE),
        TransactionOutput {
            covenant: Some(CovenantBinding {
                authorizing_input: 2,
                covenant_id: pid,
            }),
            ..unbound
        },
    ];
    let entries = vec![
        UtxoEntry::new(
            1000,
            pay_to_script_hash_script(&old_minter.bytecode),
            0,
            false,
            Some(ASSET),
        ),
        UtxoEntry::new(
            1000,
            pay_to_script_hash_script(&old_module.bytecode),
            0,
            false,
            Some(MODULE),
        ),
        UtxoEntry::new(
            COLLATERAL,
            kaspa_consensus_core::tx::ScriptPublicKey::new(0, vec![OpTrue].into()),
            0,
            false,
            None,
        ),
    ];
    let tx = Transaction::new(
        1,
        vec![
            inp(
                TransactionOutpoint {
                    transaction_id: TransactionId::from_bytes([88; 32]),
                    index: 0,
                },
                call(
                    &old_minter,
                    "transferPolicy",
                    vec![
                        tokens(vec![
                            (MODULE.as_bytes().to_vec(), 2, 0, true),
                            (pid.as_bytes().to_vec(), 2, 0, true),
                            (pk.clone(), 0, usable_amount, false),
                            (pid.as_bytes().to_vec(), 2, ASSIGNED, false),
                            (RESERVE.as_bytes().to_vec(), 2, fee_amount, false),
                        ]),
                        Expr::bytes(vec![0; 65]),
                        Expr::byte(0),
                    ],
                    true,
                ),
            ),
            inp(
                TransactionOutpoint {
                    transaction_id: TransactionId::from_bytes([89; 32]),
                    index: 0,
                },
                call(
                    &old_module,
                    "openPolicy",
                    vec![
                        mod_state_with_price(1_500_000_000, 1, liquidation_price),
                        Expr::byte(6),
                        Expr::dynamic_bytes(pp.clone()),
                        Expr::dynamic_bytes(ps.clone()),
                        pos_state_with_template_and_price(
                            "PositionState",
                            pk.clone(),
                            DEBT,
                            ASSIGNED,
                            &kp,
                            &ks,
                            &kh,
                            &pp,
                            &ps,
                            &ph,
                            liquidation_price,
                        ),
                        token("KCC20State", MODULE.as_bytes().to_vec(), 2, 0, true),
                        token("KCC20State", pid.as_bytes().to_vec(), 2, 0, true),
                        token("KCC20State", pk.clone(), 0, usable_amount, false),
                        token("KCC20State", pid.as_bytes().to_vec(), 2, ASSIGNED, false),
                        token(
                            "KCC20State",
                            RESERVE.as_bytes().to_vec(),
                            2,
                            fee_amount,
                            false,
                        ),
                    ],
                    true,
                ),
            ),
            inp(funding, vec![]),
        ],
        outputs,
        current_daa as u64,
        Default::default(),
        0,
        vec![],
    );
    Opened {
        tx,
        entries,
        position: position_contract,
        zero,
        minter,
        user,
        assigned,
        pk,
        key,
    }
}

#[test]
fn opens_with_a_high_uncapped_price_without_intermediate_overflow() {
    let ks = std::fs::read_to_string("contracts/kcc20.sil").unwrap();
    let ps = std::fs::read_to_string("contracts/position.sil").unwrap();
    let ms = std::fs::read_to_string("contracts/minting-module.sil").unwrap();

    // 100 KUSD/KAS is above the removed governance fixture ceiling. With
    // 1,000 KAS, the raw product exceeds i64 even though the scaled reference
    // value (100,000 KUSD) remains representable.
    let opened = build_open_at_price(&ks, &ps, &ms, 0, 10_000_000_000);
    execute(&opened.tx, opened.entries.clone(), 0).expect("KUSD mint at uncapped price");
    execute(&opened.tx, opened.entries, 1).expect("overflow-safe collateral check");
}

#[test]
fn chained_open_repay_close_and_consensus_fraud_cases() {
    let ks = std::fs::read_to_string("contracts/kcc20.sil").unwrap();
    let ps = std::fs::read_to_string("contracts/position.sil").unwrap();
    let ms = std::fs::read_to_string("contracts/minting-module.sil").unwrap();
    let o = build_open(&ks, &ps, &ms);
    execute(&o.tx, o.entries.clone(), 0).expect("KUSD mint");
    execute(&o.tx, o.entries.clone(), 1).expect("base protocol open");

    let mut below_minimum = o.tx.clone();
    below_minimum.outputs[6].value = 999_999_999;
    assert!(execute(&below_minimum, o.entries.clone(), 1).is_err());

    let half_life = build_open_at(&ks, &ps, &ms, YEAR_DAA / 2);
    execute(&half_life.tx, half_life.entries.clone(), 0)
        .expect("KUSD mint with half-life risk premium");
    execute(&half_life.tx, half_life.entries.clone(), 1).expect("open with half-life risk premium");
    let mut stale_annual_fee = o.tx.clone();
    stale_annual_fee.lock_time = (YEAR_DAA / 2) as u64;
    assert!(execute(&stale_annual_fee, o.entries.clone(), 1).is_err());
    let pid = o.tx.outputs[6].covenant.as_ref().unwrap().covenant_id;
    let op = |i| TransactionOutpoint {
        transaction_id: o.tx.id(),
        index: i,
    };
    assert_eq!(USABLE + FEE, USER_BURN);
    // Invalid open paths: amount, owner, mandatory output, and Position singleton.
    let mut excessive = o.tx.clone();
    excessive.outputs[2].script_public_key =
        pay_to_script_hash_script(&kcc(&ks, o.pk.clone(), USABLE + 1, 0, false).bytecode);
    assert!(execute(&excessive, o.entries.clone(), 1).is_err());
    let mut wrong_assignee = o.tx.clone();
    wrong_assignee.outputs[3].script_public_key = pay_to_script_hash_script(
        &kcc(&ks, RESERVE.as_bytes().to_vec(), ASSIGNED, 2, false).bytecode,
    );
    assert!(execute(&wrong_assignee, o.entries.clone(), 1).is_err());
    let mut missing_fee = o.tx.clone();
    missing_fee.outputs.remove(4);
    assert!(execute(&missing_fee, o.entries.clone(), 1).is_err());
    let mut duplicate_position = o.tx.clone();
    duplicate_position.outputs.push(o.tx.outputs[6].clone());
    assert!(execute(&duplicate_position, o.entries.clone(), 1).is_err());
    // The wallet consolidates usable mint and the fee top-up before repay.
    let repayment_contract = kcc(&ks, o.pk.clone(), USER_BURN, 0, false);
    let repayment_out = TransactionOutput {
        value: 1000,
        script_public_key: pay_to_script_hash_script(&repayment_contract.bytecode),
        covenant: Some(CovenantBinding {
            authorizing_input: 2,
            covenant_id: ASSET,
        }),
    };
    let repay_outputs = vec![
        out(&o.minter, 1000, 1, ASSET),
        out(&o.zero, COLLATERAL, 0, pid),
    ];
    let repay_entries = vec![
        entry(&o.tx.outputs[6], pid),
        entry(&o.tx.outputs[1], ASSET),
        entry(&repayment_out, ASSET),
        entry(&o.tx.outputs[3], ASSET),
    ];
    let repayment_op = TransactionOutpoint {
        transaction_id: TransactionId::from_bytes([91; 32]),
        index: 0,
    };
    let unsigned = Transaction::new(
        1,
        vec![
            inp(op(6), vec![]),
            inp(op(1), vec![]),
            inp(repayment_op, vec![]),
            inp(op(3), vec![]),
        ],
        repay_outputs.clone(),
        0,
        Default::default(),
        0,
        vec![],
    );
    let repayment_signature = sign(unsigned, repay_entries.clone(), 2, &o.key);
    let repay = Transaction::new(
        1,
        vec![
            inp(
                op(6),
                call(
                    &o.position,
                    "repayPolicy",
                    vec![
                        pos_state_with_template(
                            "State",
                            o.pk.clone(),
                            0,
                            0,
                            &parts(&o.user).0,
                            &parts(&o.user).1,
                            &parts(&o.user).2,
                            &parts(&o.position).0,
                            &parts(&o.position).1,
                            &parts(&o.position).2,
                        ),
                        Expr::int(DEBT),
                        token("KCC20State", pid.as_bytes().to_vec(), 2, 0, true),
                        token("KCC20State", pid.as_bytes().to_vec(), 2, 0, false),
                    ],
                    true,
                ),
            ),
            inp(
                op(1),
                call(
                    &o.minter,
                    "transferPolicy",
                    vec![
                        tokens(vec![(pid.as_bytes().to_vec(), 2, 0, true)]),
                        Expr::bytes(vec![0; 65]),
                        Expr::byte(0),
                    ],
                    true,
                ),
            ),
            inp(
                repayment_op,
                call(
                    &repayment_contract,
                    "transferPolicy",
                    vec![Expr::bytes(repayment_signature), Expr::byte(0)],
                    false,
                ),
            ),
            inp(
                op(3),
                call(
                    &o.assigned,
                    "transferPolicy",
                    vec![Expr::bytes(vec![0; 65]), Expr::byte(0)],
                    false,
                ),
            ),
        ],
        repay_outputs,
        0,
        Default::default(),
        0,
        vec![],
    );
    for i in 0..4 {
        execute(&repay, repay_entries.clone(), i)
            .unwrap_or_else(|e| panic!("repay input {i}: {e}"));
    }
    // Underpayment/forged state: a smaller payment cannot clear all debt.
    let mut under_entries = repay_entries.clone();
    under_entries[2] = UtxoEntry::new(
        1000,
        pay_to_script_hash_script(&kcc(&ks, o.pk.clone(), USER_BURN - 1, 0, false).bytecode),
        0,
        false,
        Some(ASSET),
    );
    assert!(execute(&repay, under_entries, 0).is_err());
    let mut stolen_assigned = repay_entries.clone();
    stolen_assigned[3] = UtxoEntry::new(
        1000,
        pay_to_script_hash_script(
            &kcc(&ks, RESERVE.as_bytes().to_vec(), ASSIGNED, 2, false).bytecode,
        ),
        0,
        false,
        Some(ASSET),
    );
    assert!(execute(&repay, stolen_assigned, 0).is_err());
    let mut duplicate_repay = repay.clone();
    duplicate_repay.outputs.push(repay.outputs[1].clone());
    assert!(execute(&duplicate_repay, repay_entries.clone(), 0).is_err());
    // Direct withdrawal and wrong KAS value are rejected while debt exists.
    let owner_spk = pay_to_address_script(&Address::new(Prefix::Testnet, Version::PubKey, &o.pk));
    let bad_close_outputs = vec![TransactionOutput {
        value: COLLATERAL - 1,
        script_public_key: owner_spk.clone(),
        covenant: None,
    }];
    let bad_entries = vec![entry(&o.tx.outputs[6], pid), entry(&o.tx.outputs[1], ASSET)];
    let unsigned_bad = Transaction::new(
        1,
        vec![inp(op(6), vec![]), inp(op(1), vec![])],
        bad_close_outputs.clone(),
        0,
        Default::default(),
        0,
        vec![],
    );
    let sig = sign(unsigned_bad, bad_entries.clone(), 0, &o.key);
    let bad = Transaction::new(
        1,
        vec![
            inp(
                op(6),
                call(
                    &o.position,
                    "closePolicy",
                    vec![
                        Expr::array(parse_type_ref("State[]").unwrap(), vec![]),
                        Expr::bytes(sig),
                    ],
                    true,
                ),
            ),
            inp(
                op(1),
                call(
                    &o.minter,
                    "transferPolicy",
                    vec![tokens(vec![]), Expr::bytes(vec![0; 65]), Expr::byte(0)],
                    true,
                ),
            ),
        ],
        bad_close_outputs,
        0,
        Default::default(),
        0,
        vec![],
    );
    assert!(execute(&bad, bad_entries, 0).is_err());
    // Close chains from the outpoints actually produced by repay.
    let rp = |i| TransactionOutpoint {
        transaction_id: repay.id(),
        index: i,
    };
    let close_outputs = vec![TransactionOutput {
        value: COLLATERAL,
        script_public_key: owner_spk,
        covenant: None,
    }];
    let close_entries = vec![
        entry(&repay.outputs[1], pid),
        entry(&repay.outputs[0], ASSET),
    ];
    let unsigned_close = Transaction::new(
        1,
        vec![inp(rp(1), vec![]), inp(rp(0), vec![])],
        close_outputs.clone(),
        0,
        Default::default(),
        0,
        vec![],
    );
    let cs = sign(unsigned_close, close_entries.clone(), 0, &o.key);
    let close = Transaction::new(
        1,
        vec![
            inp(
                rp(1),
                call(
                    &o.zero,
                    "closePolicy",
                    vec![
                        Expr::array(parse_type_ref("State[]").unwrap(), vec![]),
                        Expr::bytes(cs),
                    ],
                    true,
                ),
            ),
            inp(
                rp(0),
                call(
                    &o.minter,
                    "transferPolicy",
                    vec![tokens(vec![]), Expr::bytes(vec![0; 65]), Expr::byte(0)],
                    true,
                ),
            ),
        ],
        close_outputs,
        0,
        Default::default(),
        0,
        vec![],
    );
    execute(&close, close_entries.clone(), 0).expect("close");
    execute(&close, close_entries, 1).expect("minter termination");
    // Even at zero debt, beneficiary and KAS value remain strictly bound.
    for fraudulent_output in [
        TransactionOutput {
            value: COLLATERAL - 1,
            script_public_key: close.outputs[0].script_public_key.clone(),
            covenant: None,
        },
        TransactionOutput {
            value: COLLATERAL,
            script_public_key: pay_to_address_script(&Address::new(
                Prefix::Testnet,
                Version::PubKey,
                &[7; 32],
            )),
            covenant: None,
        },
    ] {
        let outs = vec![fraudulent_output];
        let entries = vec![
            entry(&repay.outputs[1], pid),
            entry(&repay.outputs[0], ASSET),
        ];
        let unsigned = Transaction::new(
            1,
            vec![inp(rp(1), vec![]), inp(rp(0), vec![])],
            outs.clone(),
            0,
            Default::default(),
            0,
            vec![],
        );
        let sig = sign(unsigned, entries.clone(), 0, &o.key);
        let tx = Transaction::new(
            1,
            vec![
                inp(
                    rp(1),
                    call(
                        &o.zero,
                        "closePolicy",
                        vec![
                            Expr::array(parse_type_ref("State[]").unwrap(), vec![]),
                            Expr::bytes(sig),
                        ],
                        true,
                    ),
                ),
                inp(
                    rp(0),
                    call(
                        &o.minter,
                        "transferPolicy",
                        vec![tokens(vec![]), Expr::bytes(vec![0; 65]), Expr::byte(0)],
                        true,
                    ),
                ),
            ],
            outs,
            0,
            Default::default(),
            0,
            vec![],
        );
        assert!(execute(&tx, entries, 0).is_err());
    }
}
