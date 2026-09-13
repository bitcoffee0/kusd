use kaspa_addresses::{Address, Prefix, Version};
use kaspa_consensus_core::Hash;
use kaspa_consensus_core::hashing::sighash::SigHashReusedValuesUnsync;
use kaspa_consensus_core::tx::{
    CovenantBinding, PopulatedTransaction, Transaction, TransactionId, TransactionInput,
    TransactionOutpoint, TransactionOutput, UtxoEntry, VerifiableTransaction,
};
use kaspa_kusd::silverscript::{
    CompileOptions, CompiledContract, CovenantDeclCallOptions, compile_contract, struct_object,
};
use kaspa_txscript::caches::Cache;
use kaspa_txscript::covenants::CovenantsContext;
use kaspa_txscript::{
    EngineCtx, EngineFlags, TxScriptEngine, pay_to_address_script, pay_to_script_hash_script,
    script_builder::ScriptBuilder,
};
use silverscript_lang::ast::{Expr, parse_type_ref};

fn execute_case(a: i64, b: i64, denominator: i64, expected: i64) -> Result<(), String> {
    let source = std::fs::read_to_string("contracts/muldiv-harness.sil")
        .map_err(|error| error.to_string())?;
    let contract = compile_contract(
        &source,
        &[
            Expr::int(a),
            Expr::int(b),
            Expr::int(denominator),
            Expr::int(expected),
        ],
        CompileOptions::default(),
    )
    .map_err(|error| error.to_string())?;
    let mut signature_script = contract
        .build_sig_script("verify", vec![])
        .map_err(|error| error.to_string())?;
    let flags = EngineFlags {
        covenants_enabled: true,
        sigop_script_units: 0.into(),
    };
    signature_script.extend(
        ScriptBuilder::with_flags(flags)
            .add_data(&contract.bytecode)
            .map_err(|error| error.to_string())?
            .drain(),
    );
    let input = TransactionInput::new_with_compute_budget(
        TransactionOutpoint::default(),
        signature_script,
        0,
        65_535,
    );
    let tx = Transaction::new(
        0,
        vec![input],
        vec![TransactionOutput::new(
            1,
            pay_to_script_hash_script(&contract.bytecode),
        )],
        0,
        Default::default(),
        0,
        vec![],
    );
    let populated = PopulatedTransaction::new(
        &tx,
        vec![UtxoEntry::new(
            1,
            pay_to_script_hash_script(&contract.bytecode),
            0,
            false,
            None,
        )],
    );
    let cache = Cache::new(10_000);
    let reused = SigHashReusedValuesUnsync::new();
    let covenants = CovenantsContext::from_tx(&populated).map_err(|error| error.to_string())?;
    let mut engine = TxScriptEngine::from_transaction_input(
        &populated,
        &tx.inputs[0],
        0,
        populated.utxo(0).unwrap(),
        EngineCtx::new(&cache)
            .with_reused(&reused)
            .with_covenants_ctx(&covenants),
        flags,
    );
    engine.execute().map_err(|error| format!("{error:?}"))
}

#[test]
fn full_kps_redemption_no_longer_overflows() {
    execute_case(
        1_515_000_000,
        100_000_000_000,
        1_515_000_000,
        100_000_000_000,
    )
    .unwrap();
}

#[test]
fn long_division_matches_i128_reference_at_boundaries() {
    let cases = [
        (i64::MAX - 1, i64::MAX - 2, i64::MAX),
        (4_611_686_018_427_387_904, 2, 3),
        (7_575_000_001, 10_000_000_003, 1_515_000_007),
        (1, i64::MAX, i64::MAX),
    ];
    for (a, b, denominator) in cases {
        let expected = ((a as i128) * (b as i128) / denominator as i128) as i64;
        execute_case(a, b, denominator, expected).unwrap();
    }
}

#[test]
fn deterministic_property_cases_match_the_i128_reference() {
    // Deterministic LCG: reproducible without fuzzing or network dependencies.
    let mut seed = 0x6a09_e667_f3bc_c909_u64;
    for _ in 0..32 {
        seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        let a = (seed & i64::MAX as u64) as i64;
        seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        let b = (seed & i64::MAX as u64) as i64;
        seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        // d >= a ensures the final quotient fits in i64 even when
        // the intermediate a*b product greatly exceeds i64.
        let denominator = a.max((seed & i64::MAX as u64) as i64).max(1);
        let expected = ((a as i128) * (b as i128) / denominator as i128) as i64;
        execute_case(a, b, denominator, expected).unwrap();
    }
}

#[test]
fn a_false_quoted_result_is_rejected() {
    assert!(execute_case(i64::MAX - 1, i64::MAX - 2, i64::MAX, 1).is_err());
}

fn template_parts(c: &CompiledContract<'_>) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let l = c.state_layout;
    (
        c.bytecode[..l.start].to_vec(),
        c.bytecode[l.start + l.len..].to_vec(),
        c.template_hash().to_vec(),
    )
}

fn token_state(owner: Vec<u8>, kind: u8, amount: i64, minter: bool) -> Expr<'static> {
    struct_object(
        "TokenState",
        vec![
            ("ownerIdentifier", Expr::bytes(owner)),
            ("identifierType", Expr::byte(kind)),
            ("amount", Expr::int(amount)),
            ("isMinter", Expr::bool(minter)),
        ],
    )
}

#[test]
fn reserve_redeem_uses_muldiv_for_the_previous_overflow_case() {
    const KUSD: Hash = Hash::from_bytes([0x61; 32]);
    const KPS: Hash = Hash::from_bytes([0x62; 32]);
    const RESERVE: Hash = Hash::from_bytes([0x63; 32]);
    const SHARES: i64 = 1_515_000_000;
    const KAS: u64 = 100_000_000_000;
    let pk = secp256k1::Keypair::from_secret_key(
        &secp256k1::SECP256K1,
        &secp256k1::SecretKey::from_slice(&[31; 32]).unwrap(),
    )
    .x_only_public_key()
    .0
    .serialize()
    .to_vec();
    let kcc_src = std::fs::read_to_string("contracts/kcc20.sil").unwrap();
    let kps_src = std::fs::read_to_string("contracts/kps.sil").unwrap();
    let reserve_src = std::fs::read_to_string("contracts/equity-reserve-base.sil").unwrap();
    let compile_kcc = |owner: Vec<u8>, amount: i64, kind: u8, minter: bool| {
        compile_contract(
            &kcc_src,
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
    };
    let compile_kps = |owner: Vec<u8>, amount: i64, kind: u8, minter: bool| {
        compile_contract(
            &kps_src,
            &[
                Expr::bytes(owner),
                Expr::int(amount),
                Expr::byte(kind),
                Expr::bool(minter),
                Expr::int(8),
                Expr::int(8),
                Expr::int(1),
                Expr::dynamic_bytes(vec![]),
                Expr::dynamic_bytes(vec![]),
                Expr::bytes(vec![0; 32]),
            ],
            CompileOptions::default(),
        )
        .unwrap()
    };
    let kcc_probe = compile_kcc(vec![0; 32], 0, 2, true);
    let kps_probe = compile_kps(vec![0; 32], 0, 2, true);
    let (cp, cs, ch) = template_parts(&kcc_probe);
    let (sp, ss, sh) = template_parts(&kps_probe);
    let reserve_args = |reserve: i64, total: i64, kas: i64| {
        vec![
            Expr::bytes(KUSD.as_bytes().to_vec()),
            Expr::bytes(KPS.as_bytes().to_vec()),
            Expr::int(reserve),
            Expr::int(total),
            Expr::int(kas),
            Expr::int(cp.len() as i64),
            Expr::int(cs.len() as i64),
            Expr::bytes(ch.clone()),
            Expr::int(sp.len() as i64),
            Expr::int(ss.len() as i64),
            Expr::bytes(sh.clone()),
            Expr::int(0),
            Expr::int(0),
            Expr::bytes(vec![0; 32]),
        ]
    };
    let reserve = compile_contract(
        &reserve_src,
        &reserve_args(SHARES, SHARES, KAS as i64),
        CompileOptions::default(),
    )
    .unwrap();
    let next = compile_contract(
        &reserve_src,
        &reserve_args(0, 0, 0),
        CompileOptions::default(),
    )
    .unwrap();
    let reserve_kusd = compile_kcc(RESERVE.as_bytes().to_vec(), SHARES, 2, false);
    let reserve_kusd_next = compile_kcc(RESERVE.as_bytes().to_vec(), 0, 2, false);
    let payout = compile_kcc(pk.clone(), SHARES, 0, false);
    let minter = compile_kps(RESERVE.as_bytes().to_vec(), 0, 2, true);
    let shares = compile_kps(pk.clone(), SHARES, 0, false);
    let output = |c: &CompiledContract<'_>, value, auth, id| TransactionOutput {
        value,
        script_public_key: pay_to_script_hash_script(&c.bytecode),
        covenant: Some(CovenantBinding {
            authorizing_input: auth,
            covenant_id: id,
        }),
    };
    let outputs = vec![
        output(&next, 1_000, 0, RESERVE),
        output(&reserve_kusd_next, 1_000, 1, KUSD),
        output(&payout, 1_000, 1, KUSD),
        output(&minter, 1_000, 2, KPS),
        TransactionOutput {
            value: KAS,
            script_public_key: pay_to_address_script(&Address::new(
                Prefix::Testnet,
                Version::PubKey,
                &pk,
            )),
            covenant: None,
        },
    ];
    let entries = vec![
        UtxoEntry::new(
            KAS + 1_000,
            pay_to_script_hash_script(&reserve.bytecode),
            0,
            false,
            Some(RESERVE),
        ),
        UtxoEntry::new(
            1_000,
            pay_to_script_hash_script(&reserve_kusd.bytecode),
            0,
            false,
            Some(KUSD),
        ),
        UtxoEntry::new(
            1_000,
            pay_to_script_hash_script(&minter.bytecode),
            0,
            false,
            Some(KPS),
        ),
        UtxoEntry::new(
            1_000,
            pay_to_script_hash_script(&shares.bytecode),
            0,
            false,
            Some(KPS),
        ),
    ];
    let mut script = reserve
        .build_sig_script_for_covenant_decl(
            "redeemPolicy",
            vec![
                struct_object(
                    "State",
                    vec![
                        ("kusdAssetId", Expr::bytes(KUSD.as_bytes().to_vec())),
                        ("kpsAssetId", Expr::bytes(KPS.as_bytes().to_vec())),
                        ("reserveKusd", Expr::int(0)),
                        ("totalKps", Expr::int(0)),
                        ("collateralSompi", Expr::int(0)),
                        ("auctionPrefixLenState", Expr::int(0)),
                        ("auctionSuffixLenState", Expr::int(0)),
                        ("auctionTemplateHashState", Expr::bytes(vec![0; 32])),
                    ],
                ),
                Expr::bytes(pk.clone()),
                Expr::int(SHARES),
                token_state(RESERVE.as_bytes().to_vec(), 2, 0, false),
                token_state(pk.clone(), 0, SHARES, false),
                token_state(RESERVE.as_bytes().to_vec(), 2, 0, true),
                Expr::byte(4),
            ],
            CovenantDeclCallOptions { is_leader: true },
        )
        .unwrap();
    let flags = EngineFlags {
        covenants_enabled: true,
        sigop_script_units: 0.into(),
    };
    script.extend_from_slice(
        &ScriptBuilder::with_flags(flags)
            .add_data(&reserve.bytecode)
            .unwrap()
            .drain(),
    );
    let state = |owner: Vec<u8>, kind, amount, minter| {
        struct_object(
            "State",
            vec![
                ("ownerIdentifier", Expr::bytes(owner)),
                ("identifierType", Expr::byte(kind)),
                ("amount", Expr::int(amount)),
                ("isMinter", Expr::bool(minter)),
            ],
        )
    };
    let call = |c: &CompiledContract<'_>, args: Vec<Expr<'_>>, leader| {
        let mut s = c
            .build_sig_script_for_covenant_decl(
                "transferPolicy",
                args,
                CovenantDeclCallOptions { is_leader: leader },
            )
            .unwrap();
        s.extend_from_slice(
            &ScriptBuilder::with_flags(flags)
                .add_data(&c.bytecode)
                .unwrap()
                .drain(),
        );
        s
    };
    let kusd_call = call(
        &reserve_kusd,
        vec![
            Expr::array(
                parse_type_ref("State[]").unwrap(),
                vec![
                    state(RESERVE.as_bytes().to_vec(), 2, 0, false),
                    state(pk.clone(), 0, SHARES, false),
                ],
            ),
            Expr::bytes(vec![0; 65]),
            Expr::byte(0),
        ],
        true,
    );
    let minter_call = call(
        &minter,
        vec![
            Expr::array(
                parse_type_ref("State[]").unwrap(),
                vec![state(RESERVE.as_bytes().to_vec(), 2, 0, true)],
            ),
            Expr::bytes(vec![0; 65]),
            Expr::byte(0),
        ],
        true,
    );
    let shares_call = call(
        &shares,
        vec![Expr::bytes(vec![0; 65]), Expr::byte(0)],
        false,
    );
    let scripts = vec![script, kusd_call, minter_call, shares_call];
    let tx = Transaction::new(
        1,
        scripts
            .into_iter()
            .enumerate()
            .map(|(i, s)| {
                TransactionInput::new_with_compute_budget(
                    TransactionOutpoint {
                        transaction_id: TransactionId::from_bytes([100 + i as u8; 32]),
                        index: 0,
                    },
                    s,
                    0,
                    65_535,
                )
            })
            .collect(),
        outputs,
        0,
        Default::default(),
        0,
        vec![],
    );
    let reused = SigHashReusedValuesUnsync::new();
    let cache = Cache::new(10_000);
    let populated = PopulatedTransaction::new(&tx, entries);
    let cov = CovenantsContext::from_tx(&populated).unwrap();
    let mut vm = TxScriptEngine::from_transaction_input(
        &populated,
        &tx.inputs[0],
        0,
        populated.utxo(0).unwrap(),
        EngineCtx::new(&cache)
            .with_reused(&reused)
            .with_covenants_ctx(&cov),
        flags,
    );
    vm.execute()
        .expect("full Reserve redemption must no longer overflow");
}
