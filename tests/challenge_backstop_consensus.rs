use kaspa_addresses::{Address, Prefix, Version};
use kaspa_consensus_core::hashing::sighash::SigHashReusedValuesUnsync;
use kaspa_consensus_core::tx::{
    CovenantBinding, PopulatedTransaction, Transaction, TransactionId, TransactionInput,
    TransactionOutpoint, TransactionOutput, UtxoEntry, VerifiableTransaction,
};
use kaspa_consensus_core::{Hash, hashing};
use kaspa_kusd::protocol::{
    ProtocolParams, anchor_args, auction_args, challenge_args, challenged_position_args,
    compile_stack, kcc_args, position_args, reserve_args,
};
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
const KPS: Hash = Hash::from_bytes(*b"KKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKK");
const RESERVE: Hash = Hash::from_bytes(*b"EEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEE");
const MODULE: Hash = Hash::from_bytes(*b"MMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMM");
const POSITION: Hash = Hash::from_bytes(*b"PPPPPPPPPPPPPPPPPPPPPPPPPPPPPPPP");
const DEBT: i64 = 1_500_000_000;
const ASSIGNED: i64 = 150_000_000;
const REWARD: i64 = 15_000_000;
const PAYMENT: i64 = DEBT - ASSIGNED + REWARD;
const COLLATERAL: u64 = 100_000_000_000;
const PERIOD: u64 = 3_600;

fn params(owner: Vec<u8>, challenger: Vec<u8>) -> ProtocolParams {
    ProtocolParams {
        owner,
        challenger,
        asset_id: ASSET.as_bytes().to_vec(),
        kps_id: KPS.as_bytes().to_vec(),
        reserve_id: RESERVE.as_bytes().to_vec(),
        module_id: MODULE.as_bytes().to_vec(),
        position_id: POSITION.as_bytes().to_vec(),
        debt: DEBT,
        assigned_reserve: ASSIGNED,
        reserve_contribution_ppm: 100_000,
        risk_premium_ppm: 20_000,
        daa_per_year: 31_536_000,
        liquidation_price: 3_500_000,
        challenge_period_daa: PERIOD as i64,
        auction_duration_daa: PERIOD as i64,
        challenge_reward_ppm: 10_000,
        collateral_sompi: COLLATERAL as i64,
        minimum_collateral_sompi: 1_000_000_000,
        current_daa: 399_000_000,
        reserve_kusd: PAYMENT,
        total_kps: 2_000_000_000,
        reserve_collateral_sompi: 0,
        minimum_kps_holding_daa: 86_400,
        max_kps_vote_weight: 4,
        module_remaining_mint: 100_000_000_000,
        max_debt_per_position: 2_000_000_000,
        module_expiration_daa: 400_000_000,
        position_nonce: 0,
    }
}

fn source(path: &str) -> String {
    std::fs::read_to_string(path).unwrap()
}

fn compile<'a>(source: &'a str, args: &[Expr<'a>]) -> CompiledContract<'a> {
    compile_contract(source, args, CompileOptions::default()).unwrap()
}

fn challenge_state(
    name: &'static str,
    p: &ProtocolParams,
    kp: usize,
    ks: usize,
    kh: Vec<u8>,
) -> Expr<'static> {
    struct_object(
        name,
        vec![
            ("owner", Expr::bytes(p.owner.clone())),
            ("challenger", Expr::bytes(p.challenger.clone())),
            ("positionId", Expr::bytes(p.position_id.clone())),
            ("assetId", Expr::bytes(p.asset_id.clone())),
            ("debt", Expr::int(p.debt)),
            ("assignedReserve", Expr::int(p.assigned_reserve)),
            ("reserveId", Expr::bytes(p.reserve_id.clone())),
            ("liquidationPrice", Expr::int(p.liquidation_price)),
            ("rewardPpm", Expr::int(p.challenge_reward_ppm)),
            ("challengePeriodDaa", Expr::int(p.challenge_period_daa)),
            ("auctionDurationDaa", Expr::int(p.auction_duration_daa)),
            ("collateralSompi", Expr::int(p.collateral_sompi)),
            ("kccPrefixLen", Expr::int(kp as i64)),
            ("kccSuffixLen", Expr::int(ks as i64)),
            ("kccTemplateHash", Expr::bytes(kh)),
        ],
    )
}

fn auction_state(
    name: &'static str,
    p: &ProtocolParams,
    kp: usize,
    ks: usize,
    kh: Vec<u8>,
) -> Expr<'static> {
    struct_object(
        name,
        vec![
            ("positionOwner", Expr::bytes(p.owner.clone())),
            ("challenger", Expr::bytes(p.challenger.clone())),
            ("positionId", Expr::bytes(p.position_id.clone())),
            ("assetId", Expr::bytes(p.asset_id.clone())),
            ("debt", Expr::int(p.debt)),
            ("assignedReserve", Expr::int(p.assigned_reserve)),
            ("reserveId", Expr::bytes(p.reserve_id.clone())),
            ("liquidationPrice", Expr::int(p.liquidation_price)),
            ("rewardPpm", Expr::int(p.challenge_reward_ppm)),
            ("auctionDurationDaa", Expr::int(p.auction_duration_daa)),
            ("collateralSompi", Expr::int(p.collateral_sompi)),
            ("kccPrefixLen", Expr::int(kp as i64)),
            ("kccSuffixLen", Expr::int(ks as i64)),
            ("kccTemplateHash", Expr::bytes(kh)),
        ],
    )
}

fn position_state(
    p: &ProtocolParams,
    challenge_id: Hash,
    name: &'static str,
    kh: Vec<u8>,
    kp: usize,
    ks: usize,
    pp: usize,
    ps: usize,
    ph: Vec<u8>,
) -> Expr<'static> {
    struct_object(
        name,
        vec![
            ("owner", Expr::bytes(p.owner.clone())),
            ("assetId", Expr::bytes(p.asset_id.clone())),
            ("debt", Expr::int(p.debt)),
            ("assignedReserve", Expr::int(p.assigned_reserve)),
            ("reserveId", Expr::bytes(p.reserve_id.clone())),
            (
                "reserveContributionPpm",
                Expr::int(p.reserve_contribution_ppm),
            ),
            ("liquidationPrice", Expr::int(p.liquidation_price)),
            ("challengePeriodDaa", Expr::int(p.challenge_period_daa)),
            ("auctionDurationDaa", Expr::int(p.auction_duration_daa)),
            ("challengeRewardPpm", Expr::int(p.challenge_reward_ppm)),
            ("challengeId", Expr::bytes(challenge_id.as_bytes().to_vec())),
            ("kccPrefixLen", Expr::int(kp as i64)),
            ("kccSuffixLen", Expr::int(ks as i64)),
            ("kccTemplateHash", Expr::bytes(kh)),
            ("positionPrefixLen", Expr::int(pp as i64)),
            ("positionSuffixLen", Expr::int(ps as i64)),
            ("positionTemplateHash", Expr::bytes(ph)),
        ],
    )
}

fn reserve_state(
    name: &'static str,
    p: &ProtocolParams,
    reserve: i64,
    collateral: i64,
    ap: usize,
    as_: usize,
    ah: Vec<u8>,
) -> Expr<'static> {
    struct_object(
        name,
        vec![
            ("kusdAssetId", Expr::bytes(p.asset_id.clone())),
            ("kpsAssetId", Expr::bytes(p.kps_id.clone())),
            ("reserveKusd", Expr::int(reserve)),
            ("totalKps", Expr::int(p.total_kps)),
            ("collateralSompi", Expr::int(collateral)),
            ("auctionPrefixLenState", Expr::int(ap as i64)),
            ("auctionSuffixLenState", Expr::int(as_ as i64)),
            ("auctionTemplateHashState", Expr::bytes(ah)),
        ],
    )
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

fn token_states(values: Vec<(Vec<u8>, u8, i64, bool)>) -> Expr<'static> {
    Expr::array(
        parse_type_ref("State[]").unwrap(),
        values
            .into_iter()
            .map(|(owner, kind, amount, minter)| token("State", owner, kind, amount, minter))
            .collect(),
    )
}

fn flags() -> EngineFlags {
    EngineFlags {
        covenants_enabled: true,
        sigop_script_units: 0.into(),
    }
}

fn covenant_call(
    c: &CompiledContract<'_>,
    policy: &str,
    args: Vec<Expr<'_>>,
    leader: bool,
) -> Vec<u8> {
    let mut script = c
        .build_sig_script_for_covenant_decl(
            policy,
            args,
            CovenantDeclCallOptions { is_leader: leader },
        )
        .unwrap();
    script.extend_from_slice(
        &ScriptBuilder::with_flags(flags())
            .add_data(&c.bytecode)
            .unwrap()
            .drain(),
    );
    script
}

fn entry_call(c: &CompiledContract<'_>, policy: &str, args: Vec<Expr<'_>>) -> Vec<u8> {
    let mut script = c.build_sig_script(policy, args).unwrap();
    script.extend_from_slice(
        &ScriptBuilder::with_flags(flags())
            .add_data(&c.bytecode)
            .unwrap()
            .drain(),
    );
    script
}

fn input(outpoint: TransactionOutpoint, script: Vec<u8>, sequence: u64) -> TransactionInput {
    TransactionInput::new_with_compute_budget(outpoint, script, sequence, 65_535)
}

fn output(c: &CompiledContract<'_>, value: u64, authorizer: u16, id: Hash) -> TransactionOutput {
    TransactionOutput {
        value,
        script_public_key: pay_to_script_hash_script(&c.bytecode),
        covenant: Some(CovenantBinding {
            authorizing_input: authorizer,
            covenant_id: id,
        }),
    }
}

fn entry(output: &TransactionOutput, id: Hash) -> UtxoEntry {
    UtxoEntry::new(
        output.value,
        output.script_public_key.clone(),
        0,
        false,
        Some(id),
    )
}

fn execute(tx: &Transaction, entries: Vec<UtxoEntry>, index: usize) -> Result<(), String> {
    let cache = Cache::new(10_000);
    let reused = SigHashReusedValuesUnsync::new();
    let populated = PopulatedTransaction::new(tx, entries);
    let covenants = CovenantsContext::from_tx(&populated).map_err(|e| e.to_string())?;
    let mut vm = TxScriptEngine::from_transaction_input(
        &populated,
        &tx.inputs[index],
        index,
        populated.utxo(index).unwrap(),
        EngineCtx::new(&cache)
            .with_reused(&reused)
            .with_covenants_ctx(&covenants),
        flags(),
    );
    vm.execute().map_err(|e| format!("{e:?}"))
}

fn outpoint(tx: &Transaction, index: u32) -> TransactionOutpoint {
    TransactionOutpoint {
        transaction_id: tx.id(),
        index,
    }
}

#[test]
fn chained_challenge_to_auction_to_backstop_uses_produced_outpoints() {
    let owner = Keypair::from_secret_key(
        &Secp256k1::new(),
        &SecretKey::from_slice(&[31; 32]).unwrap(),
    );
    let challenger = Keypair::from_secret_key(
        &Secp256k1::new(),
        &SecretKey::from_slice(&[32; 32]).unwrap(),
    );
    let owner_pk = owner.x_only_public_key().0.serialize().to_vec();
    let challenger_pk = challenger.x_only_public_key().0.serialize().to_vec();
    let p = params(owner_pk, challenger_pk.clone());
    let stack = compile_stack(&p).expect("pile circulaire base protocol");
    for artifact in [
        &stack.kcc,
        &stack.kps,
        &stack.reserve,
        &stack.auction,
        &stack.challenge,
        &stack.anchor,
        &stack.challenged_position,
        &stack.position,
        &stack.module,
    ] {
        assert!(!artifact.bytecode.is_empty());
        assert_eq!(artifact.template_hash.len(), 32);
    }

    let kcc_source = source("contracts/kcc20.sil");
    let position_source = source("contracts/position.sil");
    let challenged_position_source = source("contracts/challenged-position.sil");
    let anchor_source = source("contracts/challenge-anchor.sil");
    let challenge_source = source("contracts/challenge.sil");
    let auction_source = source("contracts/auction.sil");
    let reserve_source = source("contracts/equity-reserve-base.sil");
    let position = compile(&position_source, &position_args(&p, &stack, vec![0; 32]));
    let anchor = compile(&anchor_source, &anchor_args(&p, &stack));
    let challenge = compile(&challenge_source, &challenge_args(&p, &stack));
    let auction = compile(&auction_source, &auction_args(&p, &stack));
    let old_reserve = compile(
        &reserve_source,
        &reserve_args(&p, &stack, PAYMENT, p.total_kps, 0),
    );
    let next_reserve = compile(
        &reserve_source,
        &reserve_args(&p, &stack, 0, p.total_kps, COLLATERAL as i64),
    );
    assert_eq!(stack.reserve.bytecode, old_reserve.bytecode);
    assert_eq!(stack.auction.bytecode, auction.bytecode);

    let funding = TransactionOutpoint {
        transaction_id: TransactionId::from_bytes([201; 32]),
        index: 0,
    };
    let anchor_unbound = TransactionOutput {
        value: COLLATERAL,
        script_public_key: pay_to_script_hash_script(&anchor.bytecode),
        covenant: None,
    };
    let challenge_id =
        hashing::covenant_id::covenant_id(funding, std::iter::once((1, &anchor_unbound)));
    let challenged_position = compile(
        &challenged_position_source,
        &challenged_position_args(&p, &stack, challenge_id.as_bytes().to_vec()),
    );
    let kp = stack.kcc.prefix.len();
    let ks = stack.kcc.suffix.len();
    let kh = stack.kcc.template_hash.clone();

    // 1. Position + challenger deposit -> Position' + Anchor.
    let start_outputs = vec![
        output(&challenged_position, COLLATERAL, 0, POSITION),
        TransactionOutput {
            covenant: Some(CovenantBinding {
                authorizing_input: 1,
                covenant_id: challenge_id,
            }),
            ..anchor_unbound
        },
    ];
    let position_fixture = TransactionOutpoint {
        transaction_id: TransactionId::from_bytes([200; 32]),
        index: 0,
    };
    let start = Transaction::new(
        1,
        vec![
            input(
                position_fixture,
                entry_call(
                    &position,
                    "startChallengePolicy",
                    vec![
                        Expr::byte(0),
                        position_state(
                            &p,
                            challenge_id,
                            "State",
                            kh.clone(),
                            kp,
                            ks,
                            stack.position.prefix.len(),
                            stack.position.suffix.len(),
                            stack.position.template_hash.clone(),
                        ),
                        Expr::byte(1),
                        challenge_state("ChallengeState", &p, kp, ks, kh.clone()),
                    ],
                ),
                0,
            ),
            input(funding, vec![], 0),
        ],
        start_outputs,
        0,
        Default::default(),
        0,
        vec![],
    );
    let start_entries = vec![
        UtxoEntry::new(
            COLLATERAL,
            pay_to_script_hash_script(&position.bytecode),
            0,
            false,
            Some(POSITION),
        ),
        UtxoEntry::new(
            COLLATERAL,
            kaspa_consensus_core::tx::ScriptPublicKey::new(0, vec![OpTrue].into()),
            0,
            false,
            None,
        ),
    ];
    execute(&start, start_entries, 0).expect("start_challenge");

    // 2. The Anchor consumes exactly start.txid:1 and becomes a Challenge.
    let init = Transaction::new(
        1,
        vec![input(
            outpoint(&start, 1),
            entry_call(
                &anchor,
                "initPolicy",
                vec![
                    Expr::byte(0),
                    challenge_state("ChallengeState", &p, kp, ks, kh.clone()),
                ],
            ),
            0,
        )],
        vec![output(&challenge, COLLATERAL, 0, challenge_id)],
        0,
        Default::default(),
        0,
        vec![],
    );
    execute(&init, vec![entry(&start.outputs[1], challenge_id)], 0).expect("anchor init");

    // 3. The Challenge consumes init.txid:0 after its DAA delay and becomes Auction.
    let build_activate = |age: u64| {
        Transaction::new(
            1,
            vec![input(
                outpoint(&init, 0),
                entry_call(
                    &challenge,
                    "activatePolicy",
                    vec![
                        Expr::byte(0),
                        auction_state("AuctionState", &p, kp, ks, kh.clone()),
                    ],
                ),
                age,
            )],
            vec![output(&auction, COLLATERAL, 0, challenge_id)],
            0,
            Default::default(),
            0,
            vec![],
        )
    };
    let challenge_entry = vec![entry(&init.outputs[0], challenge_id)];
    assert!(execute(&build_activate(PERIOD - 1), challenge_entry.clone(), 0).is_err());
    let activate = build_activate(PERIOD);
    execute(&activate, challenge_entry, 0).expect("permissionless activate");

    // 4. The flow-produced Auction and Position are settled by Reserve.
    let minter = compile(
        &kcc_source,
        &kcc_args(POSITION.as_bytes().to_vec(), 0, 2, true),
    );
    let assigned = compile(
        &kcc_source,
        &kcc_args(POSITION.as_bytes().to_vec(), ASSIGNED, 2, false),
    );
    let reserve_token = compile(
        &kcc_source,
        &kcc_args(RESERVE.as_bytes().to_vec(), PAYMENT, 2, false),
    );
    let empty_reserve_token = compile(
        &kcc_source,
        &kcc_args(RESERVE.as_bytes().to_vec(), 0, 2, false),
    );
    let reward_token = compile(
        &kcc_source,
        &kcc_args(challenger_pk.clone(), REWARD, 0, false),
    );
    let reserve_fixture = TransactionOutpoint {
        transaction_id: TransactionId::from_bytes([202; 32]),
        index: 0,
    };
    let minter_fixture = TransactionOutpoint {
        transaction_id: TransactionId::from_bytes([203; 32]),
        index: 0,
    };
    let assigned_fixture = TransactionOutpoint {
        transaction_id: TransactionId::from_bytes([204; 32]),
        index: 0,
    };
    let reserve_token_fixture = TransactionOutpoint {
        transaction_id: TransactionId::from_bytes([205; 32]),
        index: 0,
    };
    let challenger_spk = pay_to_address_script(&Address::new(
        Prefix::Testnet,
        Version::PubKey,
        &challenger_pk,
    ));
    let backstop_outputs = vec![
        output(&empty_reserve_token, 1_000, 2, ASSET),
        output(&reward_token, 1_000, 2, ASSET),
        output(&next_reserve, COLLATERAL + 1_000, 4, RESERVE),
        TransactionOutput {
            value: COLLATERAL,
            script_public_key: challenger_spk,
            covenant: None,
        },
    ];
    let backstop_entries = vec![
        entry(&activate.outputs[0], challenge_id),
        entry(&start.outputs[0], POSITION),
        UtxoEntry::new(
            1_000,
            pay_to_script_hash_script(&minter.bytecode),
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
        UtxoEntry::new(
            1_000,
            pay_to_script_hash_script(&old_reserve.bytecode),
            0,
            false,
            Some(RESERVE),
        ),
        UtxoEntry::new(
            1_000,
            pay_to_script_hash_script(&reserve_token.bytecode),
            0,
            false,
            Some(ASSET),
        ),
    ];
    let auction_expr = || auction_state("AuctionState", &p, kp, ks, kh.clone());
    let reserve_expr = || {
        reserve_state(
            "ReserveState",
            &p,
            0,
            COLLATERAL as i64,
            stack.auction.prefix.len(),
            stack.auction.suffix.len(),
            stack.auction.template_hash.clone(),
        )
    };
    let reserve_successor_expr = || {
        reserve_state(
            "State",
            &p,
            0,
            COLLATERAL as i64,
            stack.auction.prefix.len(),
            stack.auction.suffix.len(),
            stack.auction.template_hash.clone(),
        )
    };
    let reward_expr = || token("TokenState", challenger_pk.clone(), 0, REWARD, false);
    let backstop = Transaction::new(
        1,
        vec![
            input(
                outpoint(&activate, 0),
                covenant_call(
                    &auction,
                    "backstopPolicy",
                    vec![
                        Expr::array(parse_type_ref("State[]").unwrap(), vec![]),
                        Expr::byte(4),
                        reserve_expr(),
                        Expr::byte(3),
                        reward_expr(),
                    ],
                    true,
                ),
                PERIOD,
            ),
            input(
                outpoint(&start, 0),
                entry_call(&challenged_position, "settleAuctionPolicy", vec![]),
                0,
            ),
            input(
                minter_fixture,
                covenant_call(
                    &minter,
                    "transferPolicy",
                    vec![
                        token_states(vec![
                            (RESERVE.as_bytes().to_vec(), 2, 0, false),
                            (challenger_pk.clone(), 0, REWARD, false),
                        ]),
                        Expr::bytes(vec![0; 65]),
                        Expr::byte(0),
                    ],
                    true,
                ),
                0,
            ),
            input(
                assigned_fixture,
                covenant_call(
                    &assigned,
                    "transferPolicy",
                    vec![Expr::bytes(vec![0; 65]), Expr::byte(0)],
                    false,
                ),
                0,
            ),
            input(
                reserve_fixture,
                covenant_call(
                    &old_reserve,
                    "backstopPolicy",
                    vec![
                        reserve_successor_expr(),
                        Expr::byte(0),
                        auction_expr(),
                        token("TokenState", RESERVE.as_bytes().to_vec(), 2, 0, false),
                        reward_expr(),
                    ],
                    true,
                ),
                0,
            ),
            input(
                reserve_token_fixture,
                covenant_call(
                    &reserve_token,
                    "transferPolicy",
                    vec![Expr::bytes(vec![0; 65]), Expr::byte(0)],
                    false,
                ),
                0,
            ),
        ],
        backstop_outputs,
        0,
        Default::default(),
        0,
        vec![],
    );
    let mut premature_backstop = backstop.clone();
    premature_backstop.inputs[0].sequence = PERIOD - 1;
    assert!(execute(&premature_backstop, backstop_entries.clone(), 0).is_err());
    for index in 0..6 {
        execute(&backstop, backstop_entries.clone(), index)
            .unwrap_or_else(|e| panic!("backstop input {index}: {e}"));
    }
    assert_eq!(backstop.inputs[0].previous_outpoint, outpoint(&activate, 0));
    assert_eq!(backstop.inputs[1].previous_outpoint, outpoint(&start, 0));

    // The three critical forgeries are rejected by separate covenants
    // insufficient liquidity, forged Reserve state, and duplication.
    let mut one_short = backstop_entries.clone();
    let short_token = compile(
        &kcc_source,
        &kcc_args(RESERVE.as_bytes().to_vec(), PAYMENT - 1, 2, false),
    );
    one_short[5] = UtxoEntry::new(
        1_000,
        pay_to_script_hash_script(&short_token.bytecode),
        0,
        false,
        Some(ASSET),
    );
    assert!(execute(&backstop, one_short, 0).is_err());
    let mut wrong_reserve = backstop.clone();
    let forged = compile(
        &reserve_source,
        &reserve_args(&p, &stack, 1, p.total_kps, COLLATERAL as i64),
    );
    wrong_reserve.outputs[2].script_public_key = pay_to_script_hash_script(&forged.bytecode);
    assert!(execute(&wrong_reserve, backstop_entries.clone(), 4).is_err());
    let mut duplicate = backstop.clone();
    duplicate.outputs.push(backstop.outputs[2].clone());
    assert!(execute(&duplicate, backstop_entries, 0).is_err());
}
