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
use kaspa_kusd::protocol::{
    GovernanceParams, ProtocolParams, compile_governance_stack, delegation_args, governance_args,
    kcc_args, kps_args, module_args, proposal_args_with_activation, reserve_args, root_args,
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
const GOVERNANCE: Hash = Hash::from_bytes(*b"GGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGG");
const ROOT: Hash = Hash::from_bytes(*b"RRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRR");
const DELEGATION: Hash = Hash::from_bytes(*b"DDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDD");
const ALLOCATION: i64 = 100_000_000_000;
const ROOT_ALLOCATION: i64 = 100_000_000_000;
const TOTAL_KPS: i64 = 2_000_000_000;
const VETO: i64 = 40_000_000;
const DELAY: u64 = 100;
const WINDOW: i64 = 200;
const DEPOSIT: u64 = 100_000_000;

fn setup() -> (ProtocolParams, GovernanceParams, Keypair) {
    let proposer = Keypair::from_secret_key(
        &Secp256k1::new(),
        &SecretKey::from_slice(&[41; 32]).unwrap(),
    );
    let proposer_pk = proposer.x_only_public_key().0.serialize().to_vec();
    (
        ProtocolParams {
            owner: vec![1; 32],
            challenger: vec![2; 32],
            asset_id: ASSET.as_bytes().to_vec(),
            kps_id: KPS.as_bytes().to_vec(),
            reserve_id: RESERVE.as_bytes().to_vec(),
            module_id: vec![6; 32],
            position_id: vec![7; 32],
            debt: 1_500_000_000,
            assigned_reserve: 150_000_000,
            reserve_contribution_ppm: 100_000,
            risk_premium_ppm: 20_000,
            daa_per_year: 31_536_000,
            liquidation_price: 3_500_000,
            challenge_period_daa: 3_600,
            auction_duration_daa: 3_600,
            challenge_reward_ppm: 10_000,
            collateral_sompi: 100_000_000_000,
            minimum_collateral_sompi: 1_000_000_000,
            current_daa: 399_000_000,
            reserve_kusd: 2_000_000_000,
            total_kps: TOTAL_KPS,
            reserve_collateral_sompi: 0,
            minimum_kps_holding_daa: 90,
            max_kps_vote_weight: 4,
            module_remaining_mint: ALLOCATION,
            max_debt_per_position: 2_000_000_000,
            module_expiration_daa: 400_000_000,
            position_nonce: 0,
        },
        GovernanceParams {
            proposer: proposer_pk,
            governance_id: GOVERNANCE.as_bytes().to_vec(),
            root_id: ROOT.as_bytes().to_vec(),
            proposal_nonce: 0,
            execution_nonce: 0,
            voting_delay_daa: DELAY as i64,
            execution_window_daa: WINDOW,
            veto_threshold_ppm: 20_000,
            proposal_deposit_sompi: DEPOSIT as i64,
            proposal_fee_kusd: 100_000_000,
            min_module_allocation: 1_000_000,
            max_module_allocation: 1_000_000_000_000,
            max_debt_per_position: 100_000_000_000,
            min_collateral_sompi: 100_000_000,
            max_collateral_sompi: 100_000_000_000_000,
            min_module_duration_daa: 100,
            max_module_duration_daa: 100_000_000,
            min_challenge_period_daa: 10,
            max_challenge_period_daa: 1_000_000,
            min_auction_duration_daa: 10,
            max_auction_duration_daa: 1_000_000,
            max_risk_premium_ppm: 200_000,
            min_reserve_contribution_ppm: 10_000,
            max_reserve_contribution_ppm: 500_000,
            root_remaining_allocation: ROOT_ALLOCATION,
        },
        proposer,
    )
}

fn compile<'a>(source: &'a str, args: &[Expr<'a>]) -> CompiledContract<'a> {
    compile_contract(source, args, CompileOptions::default()).unwrap()
}

fn state(name: &'static str, fields: Vec<(&'static str, Expr<'static>)>) -> Expr<'static> {
    struct_object(name, fields)
}

fn proposal_state(
    p: &ProtocolParams,
    g: &GovernanceParams,
    activated: bool,
    name: &'static str,
) -> Expr<'static> {
    state(
        name,
        vec![
            ("proposer", Expr::bytes(g.proposer.clone())),
            ("governanceId", Expr::bytes(g.governance_id.clone())),
            ("proposalNonce", Expr::int(g.proposal_nonce)),
            ("activated", Expr::bool(activated)),
            ("votingDelayDaa", Expr::int(g.voting_delay_daa)),
            ("executionWindowDaa", Expr::int(g.execution_window_daa)),
            ("vetoThresholdPpm", Expr::int(g.veto_threshold_ppm)),
            ("depositSompi", Expr::int(g.proposal_deposit_sompi)),
            ("assetId", Expr::bytes(p.asset_id.clone())),
            ("allocation", Expr::int(p.module_remaining_mint)),
            ("maxDebtPerPosition", Expr::int(p.max_debt_per_position)),
            (
                "minimumCollateralSompi",
                Expr::int(p.minimum_collateral_sompi),
            ),
            ("liquidationPrice", Expr::int(p.liquidation_price)),
            ("expirationDaa", Expr::int(p.module_expiration_daa)),
            ("challengePeriodDaa", Expr::int(p.challenge_period_daa)),
            ("auctionDurationDaa", Expr::int(p.auction_duration_daa)),
            ("challengeRewardPpm", Expr::int(p.challenge_reward_ppm)),
            (
                "reserveContributionPpm",
                Expr::int(p.reserve_contribution_ppm),
            ),
            ("riskPremiumPpm", Expr::int(p.risk_premium_ppm)),
        ],
    )
}

fn governance_state(
    p: &ProtocolParams,
    g: &GovernanceParams,
    active: Hash,
    name: &'static str,
) -> Expr<'static> {
    state(
        name,
        vec![
            ("rootId", Expr::bytes(g.root_id.clone())),
            ("assetId", Expr::bytes(p.asset_id.clone())),
            ("reserveId", Expr::bytes(p.reserve_id.clone())),
            ("proposalNonce", Expr::int(g.proposal_nonce)),
            ("executionNonce", Expr::int(g.execution_nonce)),
            ("activeProposalId", Expr::bytes(active.as_bytes().to_vec())),
        ],
    )
}

fn module_state(p: &ProtocolParams, name: &'static str) -> Expr<'static> {
    state(
        name,
        vec![
            ("assetId", Expr::bytes(p.asset_id.clone())),
            ("remainingMint", Expr::int(p.module_remaining_mint)),
            ("maxDebtPerPosition", Expr::int(p.max_debt_per_position)),
            (
                "minimumCollateralSompi",
                Expr::int(p.minimum_collateral_sompi),
            ),
            ("liquidationPrice", Expr::int(p.liquidation_price)),
            ("expirationDaa", Expr::int(p.module_expiration_daa)),
            ("challengePeriodDaa", Expr::int(p.challenge_period_daa)),
            ("auctionDurationDaa", Expr::int(p.auction_duration_daa)),
            ("challengeRewardPpm", Expr::int(p.challenge_reward_ppm)),
            ("positionNonce", Expr::int(0)),
            ("reserveId", Expr::bytes(p.reserve_id.clone())),
            (
                "reserveContributionPpm",
                Expr::int(p.reserve_contribution_ppm),
            ),
            ("riskPremiumPpm", Expr::int(p.risk_premium_ppm)),
        ],
    )
}

fn root_state(nonce: i64, remaining: i64, name: &'static str) -> Expr<'static> {
    state(
        name,
        vec![
            ("assetId", Expr::bytes(ASSET.as_bytes().to_vec())),
            ("initialized", Expr::bool(true)),
            ("moduleNonce", Expr::int(nonce)),
            ("remainingAllocation", Expr::int(remaining)),
            (
                "authorityIdentifier",
                Expr::bytes(GOVERNANCE.as_bytes().to_vec()),
            ),
            ("authorityType", Expr::byte(4)),
        ],
    )
}

fn token(name: &'static str, owner: Vec<u8>, amount: i64, kind: u8, minter: bool) -> Expr<'static> {
    state(
        name,
        vec![
            ("ownerIdentifier", Expr::bytes(owner)),
            ("identifierType", Expr::byte(kind)),
            ("amount", Expr::int(amount)),
            ("isMinter", Expr::bool(minter)),
        ],
    )
}

fn delegation_state(
    owner: Vec<u8>,
    delegate: Vec<u8>,
    amount: i64,
    weight: i64,
    name: &'static str,
) -> Expr<'static> {
    state(
        name,
        vec![
            ("owner", Expr::bytes(owner)),
            ("delegate", Expr::bytes(delegate)),
            ("kpsId", Expr::bytes(KPS.as_bytes().to_vec())),
            ("amount", Expr::int(amount)),
            ("weight", Expr::int(weight)),
        ],
    )
}

fn reserve_state(
    p: &ProtocolParams,
    ap: usize,
    as_: usize,
    ah: Vec<u8>,
    name: &'static str,
) -> Expr<'static> {
    state(
        name,
        vec![
            ("kusdAssetId", Expr::bytes(p.asset_id.clone())),
            ("kpsAssetId", Expr::bytes(p.kps_id.clone())),
            ("reserveKusd", Expr::int(p.reserve_kusd)),
            ("totalKps", Expr::int(p.total_kps)),
            ("collateralSompi", Expr::int(0)),
            ("auctionPrefixLenState", Expr::int(ap as i64)),
            ("auctionSuffixLenState", Expr::int(as_ as i64)),
            ("auctionTemplateHashState", Expr::bytes(ah)),
        ],
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

fn output(c: &CompiledContract<'_>, value: u64, auth: u16, id: Hash) -> TransactionOutput {
    TransactionOutput {
        value,
        script_public_key: pay_to_script_hash_script(&c.bytecode),
        covenant: Some(CovenantBinding {
            authorizing_input: auth,
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
    let mut opcode_log = Vec::new();
    let mut vm = TxScriptEngine::from_transaction_input(
        &populated,
        &tx.inputs[index],
        index,
        populated.utxo(index).unwrap(),
        EngineCtx::new(&cache)
            .with_reused(&reused)
            .with_covenants_ctx(&covenants),
        flags(),
    )
    .with_opcode_execution_log_buffer(&mut opcode_log);
    vm.execute().map_err(|e| {
        let trace = String::from_utf8_lossy(&opcode_log);
        let tail = trace
            .lines()
            .rev()
            .take(3)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n");
        format!("{e:?}\n{tail}")
    })
}

fn sign(tx: Transaction, entries: Vec<UtxoEntry>, index: usize, key: &Keypair) -> Vec<u8> {
    let mutable = MutableTransaction::with_entries(tx, entries);
    let reused = SigHashReusedValuesUnsync::new();
    let hash = calc_schnorr_signature_hash(&mutable.as_verifiable(), index, SIG_HASH_ALL, &reused);
    let msg = secp256k1::Message::from_digest_slice(hash.as_bytes().as_slice()).unwrap();
    let mut signature = key.sign_schnorr(msg).as_ref().to_vec();
    signature.push(SIG_HASH_ALL.to_u8());
    signature
}

fn outpoint(tx: &Transaction, index: u32) -> TransactionOutpoint {
    TransactionOutpoint {
        transaction_id: tx.id(),
        index,
    }
}

struct Proposed<'a> {
    p: ProtocolParams,
    credited_reserve_p: ProtocolParams,
    g1: GovernanceParams,
    proposal_id: Hash,
    tx: Transaction,
    governance_initial: CompiledContract<'a>,
    governance_pending: CompiledContract<'a>,
    proposal_pending: CompiledContract<'a>,
    proposal_active: CompiledContract<'a>,
    reserve_token: CompiledContract<'a>,
    fee_escrow: CompiledContract<'a>,
    stack: kaspa_kusd::protocol::GovernanceStack,
    governance_source: &'a str,
    root_source: &'a str,
    module_source: &'a str,
    kcc_source: &'a str,
}

fn build_proposal<'a>(sources: (&'a str, &'a str, &'a str, &'a str, &'a str)) -> Proposed<'a> {
    let (governance_source, proposal_source, root_source, module_source, kcc_source) = sources;
    let (p, g0, proposer_key) = setup();
    let stack = compile_governance_stack(&p, &g0).unwrap();
    let governance_initial = compile(
        governance_source,
        &governance_args(&p, &g0, &stack.proposal, vec![0; 32], &stack.base),
    );
    let mut g1 = g0.clone();
    g1.proposal_nonce = 1;
    let proposal_pending = compile(
        proposal_source,
        &proposal_args_with_activation(&p, &g1, &stack.base, false),
    );
    let proposal_active = compile(
        proposal_source,
        &proposal_args_with_activation(&p, &g1, &stack.base, true),
    );
    let fee_outpoint = TransactionOutpoint {
        transaction_id: TransactionId::from_bytes([222; 32]),
        index: 0,
    };
    let unbound = TransactionOutput {
        value: DEPOSIT,
        script_public_key: pay_to_script_hash_script(&proposal_pending.bytecode),
        covenant: None,
    };
    let proposal_id =
        hashing::covenant_id::covenant_id(fee_outpoint, std::iter::once((1, &unbound)));
    let governance_pending = compile(
        governance_source,
        &governance_args(
            &p,
            &g1,
            &stack.proposal,
            proposal_id.as_bytes().to_vec(),
            &stack.base,
        ),
    );
    let mut credited = p.clone();
    credited.reserve_kusd += g0.proposal_fee_kusd;
    let reserve_token = compile(
        kcc_source,
        &kcc_args(p.reserve_id.clone(), p.reserve_kusd, 2, false),
    );
    let fee_token = compile(
        kcc_source,
        &kcc_args(g0.proposer.clone(), g0.proposal_fee_kusd, 0, false),
    );
    let fee_escrow = compile(
        kcc_source,
        &kcc_args(
            proposal_id.as_bytes().to_vec(),
            g0.proposal_fee_kusd,
            2,
            false,
        ),
    );
    let outputs = vec![
        output(&governance_pending, 1_000, 0, GOVERNANCE),
        TransactionOutput {
            covenant: Some(CovenantBinding {
                authorizing_input: 1,
                covenant_id: proposal_id,
            }),
            ..unbound
        },
        output(&fee_escrow, 1_000, 1, ASSET),
    ];
    let tx = Transaction::new(
        1,
        vec![
            input(
                TransactionOutpoint {
                    transaction_id: TransactionId::from_bytes([210; 32]),
                    index: 0,
                },
                covenant_call(
                    &governance_initial,
                    "proposeModulePolicy",
                    vec![
                        governance_state(&p, &g1, proposal_id, "State"),
                        Expr::byte(1),
                        proposal_state(&p, &g1, false, "ProposalState"),
                        token(
                            "TokenState",
                            proposal_id.as_bytes().to_vec(),
                            g0.proposal_fee_kusd,
                            2,
                            false,
                        ),
                    ],
                    true,
                ),
                0,
            ),
            input(
                TransactionOutpoint {
                    transaction_id: fee_outpoint.transaction_id,
                    index: fee_outpoint.index,
                },
                covenant_call(
                    &fee_token,
                    "transferPolicy",
                    vec![Expr::bytes(vec![0; 65]), Expr::byte(0)],
                    false,
                ),
                0,
            ),
        ],
        outputs,
        p.current_daa as u64,
        Default::default(),
        0,
        vec![],
    );
    let entries = vec![
        UtxoEntry::new(
            1_000,
            pay_to_script_hash_script(&governance_initial.bytecode),
            0,
            false,
            Some(GOVERNANCE),
        ),
        UtxoEntry::new(
            1_000,
            pay_to_script_hash_script(&fee_token.bytecode),
            0,
            false,
            Some(ASSET),
        ),
    ];
    let mut tx = tx;
    let fee_signature = sign(tx.clone(), entries.clone(), 1, &proposer_key);
    tx.inputs[1].signature_script = covenant_call(
        &fee_token,
        "transferPolicy",
        vec![
            Expr::array(
                parse_type_ref("State[]").unwrap(),
                vec![token(
                    "State",
                    proposal_id.as_bytes().to_vec(),
                    g0.proposal_fee_kusd,
                    2,
                    false,
                )],
            ),
            Expr::bytes(fee_signature),
            Expr::byte(0),
        ],
        true,
    );
    let underpaid_fee = compile(
        kcc_source,
        &kcc_args(g0.proposer.clone(), g0.proposal_fee_kusd - 1, 0, false),
    );
    let mut underpaid_entries = entries.clone();
    underpaid_entries[1].script_public_key = pay_to_script_hash_script(&underpaid_fee.bytecode);
    assert!(execute(&tx, underpaid_entries, 0).is_err());

    let execute_all = |candidate: &Transaction, candidate_entries: Vec<UtxoEntry>| {
        (0..candidate.inputs.len()).try_for_each(|index| {
            execute(candidate, candidate_entries.clone(), index)
                .map_err(|error| format!("input {index}: {error}"))
        })
    };
    let mut diverted_fee = tx.clone();
    diverted_fee.outputs[2].script_public_key = pay_to_script_hash_script(&fee_token.bytecode);
    assert!(execute_all(&diverted_fee, entries.clone()).is_err());
    execute_all(&tx, entries).expect("permissionless module proposal with escrowed KUSD fee");
    Proposed {
        p,
        credited_reserve_p: credited,
        g1,
        proposal_id,
        tx,
        governance_initial,
        governance_pending,
        proposal_pending,
        proposal_active,
        reserve_token,
        fee_escrow,
        stack,
        governance_source,
        root_source,
        module_source,
        kcc_source,
    }
}

#[test]
fn consensus_rejects_module_terms_outside_immutable_bounds() {
    let gs = std::fs::read_to_string("contracts/governance.sil").unwrap();
    let ps = std::fs::read_to_string("contracts/module-proposal.sil").unwrap();
    let rs = std::fs::read_to_string("contracts/root-issuance.sil").unwrap();
    let ms = std::fs::read_to_string("contracts/minting-module.sil").unwrap();
    let ks = std::fs::read_to_string("contracts/kcc20.sil").unwrap();
    let proposed = build_proposal((&gs, &ps, &rs, &ms, &ks));

    let rejects = |malicious: ProtocolParams| {
        let malicious_proposal = compile(
            &ps,
            &proposal_args_with_activation(&malicious, &proposed.g1, &proposed.stack.base, false),
        );
        let fee_outpoint = TransactionOutpoint {
            transaction_id: TransactionId::from_bytes([222; 32]),
            index: 0,
        };
        let unbound = TransactionOutput {
            value: DEPOSIT,
            script_public_key: pay_to_script_hash_script(&malicious_proposal.bytecode),
            covenant: None,
        };
        let malicious_id =
            hashing::covenant_id::covenant_id(fee_outpoint, std::iter::once((1, &unbound)));
        let malicious_governance = compile(
            &gs,
            &governance_args(
                &proposed.p,
                &proposed.g1,
                &proposed.stack.proposal,
                malicious_id.as_bytes().to_vec(),
                &proposed.stack.base,
            ),
        );
        let mut tx = proposed.tx.clone();
        tx.outputs[0].script_public_key = pay_to_script_hash_script(&malicious_governance.bytecode);
        tx.outputs[1] = TransactionOutput {
            covenant: Some(CovenantBinding {
                authorizing_input: 1,
                covenant_id: malicious_id,
            }),
            ..unbound
        };
        tx.inputs[0].signature_script = covenant_call(
            &proposed.governance_initial,
            "proposeModulePolicy",
            vec![
                governance_state(&proposed.p, &proposed.g1, malicious_id, "State"),
                Expr::byte(1),
                proposal_state(&malicious, &proposed.g1, false, "ProposalState"),
                token(
                    "TokenState",
                    malicious_id.as_bytes().to_vec(),
                    proposed.g1.proposal_fee_kusd,
                    2,
                    false,
                ),
            ],
            true,
        );
        let malicious_escrow = compile(
            proposed.kcc_source,
            &kcc_args(
                malicious_id.as_bytes().to_vec(),
                proposed.g1.proposal_fee_kusd,
                2,
                false,
            ),
        );
        tx.outputs[2].script_public_key = pay_to_script_hash_script(&malicious_escrow.bytecode);
        let entries = vec![
            UtxoEntry::new(
                1_000,
                pay_to_script_hash_script(&proposed.governance_initial.bytecode),
                0,
                false,
                Some(GOVERNANCE),
            ),
            UtxoEntry::new(
                1_000,
                pay_to_script_hash_script(
                    &compile(
                        proposed.kcc_source,
                        &kcc_args(
                            proposed.g1.proposer.clone(),
                            proposed.g1.proposal_fee_kusd,
                            0,
                            false,
                        ),
                    )
                    .bytecode,
                ),
                0,
                false,
                Some(ASSET),
            ),
        ];
        assert!(execute(&tx, entries, 0).is_err());
    };

    let mut below_allocation = proposed.p.clone();
    below_allocation.module_remaining_mint = proposed.g1.min_module_allocation - 1;
    rejects(below_allocation);
    let mut above_allocation = proposed.p.clone();
    above_allocation.module_remaining_mint = proposed.g1.max_module_allocation + 1;
    rejects(above_allocation);
    let mut zero_debt_limit = proposed.p.clone();
    zero_debt_limit.max_debt_per_position = 0;
    rejects(zero_debt_limit);
    let mut excessive_debt_limit = proposed.p.clone();
    excessive_debt_limit.max_debt_per_position = proposed.g1.max_debt_per_position + 1;
    rejects(excessive_debt_limit);
    let mut below_collateral = proposed.p.clone();
    below_collateral.minimum_collateral_sompi = proposed.g1.min_collateral_sompi - 1;
    rejects(below_collateral);
    let mut above_collateral = proposed.p.clone();
    above_collateral.minimum_collateral_sompi = proposed.g1.max_collateral_sompi + 1;
    rejects(above_collateral);
    let mut non_positive_price = proposed.p.clone();
    non_positive_price.liquidation_price = 0;
    rejects(non_positive_price);
    let mut short_module = proposed.p.clone();
    short_module.module_expiration_daa =
        proposed.p.current_daa + proposed.g1.min_module_duration_daa - 1;
    rejects(short_module);
    let mut long_module = proposed.p.clone();
    long_module.module_expiration_daa =
        proposed.p.current_daa + proposed.g1.max_module_duration_daa + 1;
    rejects(long_module);
    let mut short_challenge = proposed.p.clone();
    short_challenge.challenge_period_daa = proposed.g1.min_challenge_period_daa - 1;
    rejects(short_challenge);
    let mut long_challenge = proposed.p.clone();
    long_challenge.challenge_period_daa = proposed.g1.max_challenge_period_daa + 1;
    rejects(long_challenge);
    let mut short_auction = proposed.p.clone();
    short_auction.auction_duration_daa = proposed.g1.min_auction_duration_daa - 1;
    rejects(short_auction);
    let mut long_auction = proposed.p.clone();
    long_auction.auction_duration_daa = proposed.g1.max_auction_duration_daa + 1;
    rejects(long_auction);
    let mut zero_reward = proposed.p.clone();
    zero_reward.challenge_reward_ppm = 0;
    rejects(zero_reward);
    let mut excessive_reward = proposed.p.clone();
    excessive_reward.challenge_reward_ppm = 100_001;
    rejects(excessive_reward);
    let mut low_reserve = proposed.p.clone();
    low_reserve.reserve_contribution_ppm = proposed.g1.min_reserve_contribution_ppm - 1;
    rejects(low_reserve);
    let mut high_reserve = proposed.p.clone();
    high_reserve.reserve_contribution_ppm = proposed.g1.max_reserve_contribution_ppm + 1;
    rejects(high_reserve);
    let mut negative_risk = proposed.p.clone();
    negative_risk.risk_premium_ppm = -1;
    rejects(negative_risk);
    let mut excessive_risk = proposed.p.clone();
    excessive_risk.risk_premium_ppm = proposed.g1.max_risk_premium_ppm + 1;
    rejects(excessive_risk);
}

#[test]
fn proposal_activation_and_root_module_execution_are_chained_and_authenticated() {
    let gs = std::fs::read_to_string("contracts/governance.sil").unwrap();
    let ps = std::fs::read_to_string("contracts/module-proposal.sil").unwrap();
    let rs = std::fs::read_to_string("contracts/root-issuance.sil").unwrap();
    let ms = std::fs::read_to_string("contracts/minting-module.sil").unwrap();
    let ks = std::fs::read_to_string("contracts/kcc20.sil").unwrap();
    let proposed = build_proposal((&gs, &ps, &rs, &ms, &ks));

    let activate = |age| {
        Transaction::new(
            1,
            vec![input(
                outpoint(&proposed.tx, 1),
                covenant_call(
                    &proposed.proposal_pending,
                    "activatePolicy",
                    vec![proposal_state(&proposed.p, &proposed.g1, true, "State")],
                    true,
                ),
                age,
            )],
            vec![output(
                &proposed.proposal_active,
                DEPOSIT,
                0,
                proposed.proposal_id,
            )],
            0,
            Default::default(),
            0,
            vec![],
        )
    };
    let proposal_entry = vec![entry(&proposed.tx.outputs[1], proposed.proposal_id)];
    assert!(execute(&activate(DELAY - 1), proposal_entry.clone(), 0).is_err());
    let activated = activate(DELAY);
    execute(&activated, proposal_entry, 0).expect("proposal activation");

    // Alternative branch of the activated UTXO: after the window, anyone
    // can release Governance and return the deposit. It fails before maturity.
    let governance_cleared = compile(
        proposed.governance_source,
        &governance_args(
            &proposed.p,
            &proposed.g1,
            &proposed.stack.proposal,
            vec![0; 32],
            &proposed.stack.base,
        ),
    );
    let build_cancel = |age| {
        let fee_refund = compile(
            proposed.kcc_source,
            &kcc_args(
                proposed.g1.proposer.clone(),
                proposed.g1.proposal_fee_kusd,
                0,
                false,
            ),
        );
        Transaction::new(
            1,
            vec![
                input(
                    outpoint(&proposed.tx, 0),
                    covenant_call(
                        &proposed.governance_pending,
                        "cancelExpiredPolicy",
                        vec![governance_state(
                            &proposed.p,
                            &proposed.g1,
                            Hash::from_bytes([0; 32]),
                            "State",
                        )],
                        true,
                    ),
                    0,
                ),
                input(
                    outpoint(&activated, 0),
                    entry_call(
                        &proposed.proposal_active,
                        "cancelExpiredPolicy",
                        vec![
                            Expr::byte(1),
                            token(
                                "TokenState",
                                proposed.g1.proposer.clone(),
                                proposed.g1.proposal_fee_kusd,
                                0,
                                false,
                            ),
                            Expr::byte(2),
                            Expr::byte(2),
                        ],
                    ),
                    age,
                ),
                input(
                    outpoint(&proposed.tx, 2),
                    covenant_call(
                        &proposed.fee_escrow,
                        "transferPolicy",
                        vec![
                            Expr::array(
                                parse_type_ref("State[]").unwrap(),
                                vec![token(
                                    "State",
                                    proposed.g1.proposer.clone(),
                                    proposed.g1.proposal_fee_kusd,
                                    0,
                                    false,
                                )],
                            ),
                            Expr::bytes(vec![0; 65]),
                            Expr::byte(0),
                        ],
                        true,
                    ),
                    0,
                ),
            ],
            vec![
                output(&governance_cleared, 1_000, 0, GOVERNANCE),
                TransactionOutput {
                    value: DEPOSIT,
                    script_public_key: pay_to_address_script(&Address::new(
                        Prefix::Testnet,
                        Version::PubKey,
                        &proposed.g1.proposer,
                    )),
                    covenant: None,
                },
                output(&fee_refund, 1_000, 2, ASSET),
            ],
            0,
            Default::default(),
            0,
            vec![],
        )
    };
    let cancel_entries = vec![
        entry(&proposed.tx.outputs[0], GOVERNANCE),
        entry(&activated.outputs[0], proposed.proposal_id),
        entry(&proposed.tx.outputs[2], ASSET),
    ];
    assert!(execute(&build_cancel(WINDOW as u64 - 1), cancel_entries.clone(), 1).is_err());
    let cancel = build_cancel(WINDOW as u64);
    execute(&cancel, cancel_entries.clone(), 0).expect("governance expiry cleanup");
    execute(&cancel, cancel_entries.clone(), 1).expect("proposal expiry cleanup");
    execute(&cancel, cancel_entries, 2).expect("proposal-fee refund");

    let mut executed_g = proposed.g1.clone();
    executed_g.execution_nonce = 1;
    let governance_final = compile(
        proposed.governance_source,
        &governance_args(
            &proposed.p,
            &executed_g,
            &proposed.stack.proposal,
            vec![0; 32],
            &proposed.stack.base,
        ),
    );
    let root_old = compile(
        proposed.root_source,
        &root_args(
            &proposed.p,
            &proposed.stack,
            0,
            ROOT_ALLOCATION,
            GOVERNANCE.as_bytes().to_vec(),
            4,
        ),
    );
    let root_next = compile(
        proposed.root_source,
        &root_args(
            &proposed.p,
            &proposed.stack,
            1,
            ROOT_ALLOCATION - ALLOCATION,
            GOVERNANCE.as_bytes().to_vec(),
            4,
        ),
    );
    let module = compile(
        proposed.module_source,
        &module_args(&proposed.p, &proposed.stack.base),
    );
    let root_minter = compile(
        proposed.kcc_source,
        &kcc_args(ROOT.as_bytes().to_vec(), 0, 2, true),
    );
    let funding = TransactionOutpoint {
        transaction_id: TransactionId::from_bytes([212; 32]),
        index: 0,
    };
    let module_unbound = TransactionOutput {
        value: 1_000,
        script_public_key: pay_to_script_hash_script(&module.bytecode),
        covenant: None,
    };
    let module_id =
        hashing::covenant_id::covenant_id(funding, std::iter::once((4, &module_unbound)));
    let module_minter = compile(
        proposed.kcc_source,
        &kcc_args(module_id.as_bytes().to_vec(), 0, 2, true),
    );
    let refund = TransactionOutput {
        value: DEPOSIT,
        script_public_key: pay_to_address_script(&Address::new(
            Prefix::Testnet,
            Version::PubKey,
            &proposed.g1.proposer,
        )),
        covenant: None,
    };
    let fee_refund = compile(
        proposed.kcc_source,
        &kcc_args(
            proposed.g1.proposer.clone(),
            proposed.g1.proposal_fee_kusd,
            0,
            false,
        ),
    );
    let outputs = vec![
        output(&governance_final, 1_000, 0, GOVERNANCE),
        output(&root_next, 1_000, 2, ROOT),
        output(&root_minter, 1_000, 3, ASSET),
        output(&module_minter, 1_000, 3, ASSET),
        TransactionOutput {
            covenant: Some(CovenantBinding {
                authorizing_input: 5,
                covenant_id: module_id,
            }),
            ..module_unbound
        },
        refund,
        output(&fee_refund, 1_000, 4, ASSET),
    ];
    let entries = vec![
        entry(&proposed.tx.outputs[0], GOVERNANCE),
        entry(&activated.outputs[0], proposed.proposal_id),
        UtxoEntry::new(
            1_000,
            pay_to_script_hash_script(&root_old.bytecode),
            0,
            false,
            Some(ROOT),
        ),
        UtxoEntry::new(
            1_000,
            pay_to_script_hash_script(&root_minter.bytecode),
            0,
            false,
            Some(ASSET),
        ),
        UtxoEntry::new(
            1_000,
            pay_to_script_hash_script(&proposed.fee_escrow.bytecode),
            0,
            false,
            Some(ASSET),
        ),
        UtxoEntry::new(
            1_000,
            kaspa_consensus_core::tx::ScriptPublicKey::new(0, vec![OpTrue].into()),
            0,
            false,
            None,
        ),
    ];
    let transaction = Transaction::new(
        1,
        vec![
            input(
                outpoint(&proposed.tx, 0),
                covenant_call(
                    &proposed.governance_pending,
                    "executeModulePolicy",
                    vec![governance_state(
                        &proposed.p,
                        &executed_g,
                        Hash::from_bytes([0; 32]),
                        "State",
                    )],
                    true,
                ),
                0,
            ),
            input(
                outpoint(&activated, 0),
                entry_call(
                    &proposed.proposal_active,
                    "executePolicy",
                    vec![
                        Expr::byte(5),
                        token(
                            "TokenState",
                            proposed.g1.proposer.clone(),
                            proposed.g1.proposal_fee_kusd,
                            0,
                            false,
                        ),
                        Expr::byte(4),
                        Expr::byte(6),
                    ],
                ),
                0,
            ),
            input(
                TransactionOutpoint {
                    transaction_id: TransactionId::from_bytes([213; 32]),
                    index: 0,
                },
                covenant_call(
                    &root_old,
                    "createModulePolicy",
                    vec![
                        root_state(1, ROOT_ALLOCATION - ALLOCATION, "State"),
                        Expr::bytes(vec![0; 65]),
                        Expr::byte(4),
                        Expr::dynamic_bytes(proposed.stack.base.module.prefix.clone()),
                        Expr::dynamic_bytes(proposed.stack.base.module.suffix.clone()),
                        module_state(&proposed.p, "ModuleState"),
                        governance_state(
                            &proposed.p,
                            &executed_g,
                            Hash::from_bytes([0; 32]),
                            "GovernanceState",
                        ),
                        token("KCC20State", ROOT.as_bytes().to_vec(), 0, 2, true),
                        token("KCC20State", module_id.as_bytes().to_vec(), 0, 2, true),
                    ],
                    true,
                ),
                0,
            ),
            input(
                TransactionOutpoint {
                    transaction_id: TransactionId::from_bytes([214; 32]),
                    index: 0,
                },
                covenant_call(
                    &root_minter,
                    "transferPolicy",
                    vec![
                        Expr::array(
                            parse_type_ref("State[]").unwrap(),
                            vec![
                                token("State", ROOT.as_bytes().to_vec(), 0, 2, true),
                                token("State", module_id.as_bytes().to_vec(), 0, 2, true),
                                token(
                                    "State",
                                    proposed.g1.proposer.clone(),
                                    proposed.g1.proposal_fee_kusd,
                                    0,
                                    false,
                                ),
                            ],
                        ),
                        Expr::bytes(vec![0; 65]),
                        Expr::byte(0),
                    ],
                    true,
                ),
                0,
            ),
            input(
                outpoint(&proposed.tx, 2),
                covenant_call(
                    &proposed.fee_escrow,
                    "transferPolicy",
                    vec![Expr::bytes(vec![0; 65]), Expr::byte(0)],
                    false,
                ),
                0,
            ),
            input(funding, vec![], 0),
        ],
        outputs,
        0,
        Default::default(),
        0,
        vec![],
    );
    for index in 0..5 {
        execute(&transaction, entries.clone(), index)
            .unwrap_or_else(|e| panic!("governance execution input {index}: {e}"));
    }
    assert_eq!(
        transaction.inputs[0].previous_outpoint,
        outpoint(&proposed.tx, 0)
    );
    assert_eq!(
        transaction.inputs[1].previous_outpoint,
        outpoint(&activated, 0)
    );

    // The terminal Proposal path cannot divert or omit the escrowed KUSD fee.
    let attacker_refund = compile(
        proposed.kcc_source,
        &kcc_args(vec![99; 32], proposed.g1.proposal_fee_kusd, 0, false),
    );
    let mut diverted_refund = transaction.clone();
    diverted_refund.outputs[6].script_public_key =
        pay_to_script_hash_script(&attacker_refund.bytecode);
    assert!(execute(&diverted_refund, entries.clone(), 1).is_err());

    let mut forged = transaction.clone();
    let mut malicious = proposed.p.clone();
    malicious.liquidation_price += 1;
    let forged_module = compile(
        proposed.module_source,
        &module_args(&malicious, &proposed.stack.base),
    );
    forged.outputs[4].script_public_key = pay_to_script_hash_script(&forged_module.bytecode);
    assert!(execute(&forged, entries.clone(), 2).is_err());
    let mut duplicate = transaction.clone();
    duplicate.outputs.push(transaction.outputs[4].clone());
    assert!(execute(&duplicate, entries, 2).is_err());
}

#[test]
fn delegated_weighted_kps_can_veto_without_double_counting() {
    let gs = std::fs::read_to_string("contracts/governance.sil").unwrap();
    let ps = std::fs::read_to_string("contracts/module-proposal.sil").unwrap();
    let rs = std::fs::read_to_string("contracts/root-issuance.sil").unwrap();
    let ms = std::fs::read_to_string("contracts/minting-module.sil").unwrap();
    let ks = std::fs::read_to_string("contracts/kcc20.sil").unwrap();
    let proposed = build_proposal((&gs, &ps, &rs, &ms, &ks));
    let (_, _, voter) = setup();
    let voter_pk = voter.x_only_public_key().0.serialize().to_vec();
    let reserve_source = std::fs::read_to_string("contracts/equity-reserve-base.sil").unwrap();
    let kps_source = std::fs::read_to_string("contracts/kps.sil").unwrap();

    let build = |amount: i64, weight: i64, signer: &Keypair| {
        let owner_pk = voter_pk.clone();
        let delegate_pk = voter_pk.clone();
        let kps = compile(
            &kps_source,
            &kps_args(
                &proposed.p,
                &proposed.stack.base.delegation,
                DELEGATION.as_bytes().to_vec(),
                amount,
                2,
                false,
            ),
        );
        let delegation_source = std::fs::read_to_string("contracts/kps-delegation.sil").unwrap();
        let delegation = compile(
            &delegation_source,
            &delegation_args(
                &proposed.p,
                owner_pk.clone(),
                delegate_pk.clone(),
                amount,
                weight,
            ),
        );
        let reserve = compile(
            &reserve_source,
            &reserve_args(
                &proposed.p,
                &proposed.stack.base,
                proposed.p.reserve_kusd,
                TOTAL_KPS,
                0,
            ),
        );
        let reserve_next = compile(
            &reserve_source,
            &reserve_args(
                &proposed.credited_reserve_p,
                &proposed.stack.base,
                proposed.credited_reserve_p.reserve_kusd,
                TOTAL_KPS,
                0,
            ),
        );
        let merged_reserve_token = compile(
            proposed.kcc_source,
            &kcc_args(
                proposed.p.reserve_id.clone(),
                proposed.credited_reserve_p.reserve_kusd,
                2,
                false,
            ),
        );
        let cleared = proposed.g1.clone();
        let governance_clear = compile(
            proposed.governance_source,
            &governance_args(
                &proposed.p,
                &cleared,
                &proposed.stack.proposal,
                vec![0; 32],
                &proposed.stack.base,
            ),
        );
        let delegation_output = output(&delegation, 1_000, 5, DELEGATION);
        let kps_output = output(&kps, 1_000, 6, KPS);
        let outputs = vec![
            output(&governance_clear, 1_000, 0, GOVERNANCE),
            output(&reserve_next, 1_000, 2, RESERVE),
            output(&merged_reserve_token, 1_000, 3, ASSET),
            delegation_output,
            kps_output,
            TransactionOutput {
                value: DEPOSIT,
                script_public_key: pay_to_address_script(&Address::new(
                    Prefix::Testnet,
                    Version::PubKey,
                    &proposed.g1.proposer,
                )),
                covenant: None,
            },
        ];
        let entries = vec![
            entry(&proposed.tx.outputs[0], GOVERNANCE),
            entry(&proposed.tx.outputs[1], proposed.proposal_id),
            UtxoEntry::new(
                1_000,
                pay_to_script_hash_script(&reserve.bytecode),
                0,
                false,
                Some(RESERVE),
            ),
            UtxoEntry::new(
                1_000,
                pay_to_script_hash_script(&proposed.reserve_token.bytecode),
                0,
                false,
                Some(ASSET),
            ),
            entry(&proposed.tx.outputs[2], ASSET),
            UtxoEntry::new(
                1_000,
                pay_to_script_hash_script(&delegation.bytecode),
                0,
                false,
                Some(DELEGATION),
            ),
            UtxoEntry::new(
                1_000,
                pay_to_script_hash_script(&kps.bytecode),
                0,
                false,
                Some(KPS),
            ),
        ];
        let unsigned = Transaction::new(
            1,
            vec![
                input(outpoint(&proposed.tx, 0), vec![], 0),
                input(outpoint(&proposed.tx, 1), vec![], 0),
                input(
                    TransactionOutpoint {
                        transaction_id: TransactionId::from_bytes([215; 32]),
                        index: 0,
                    },
                    vec![],
                    0,
                ),
                input(
                    TransactionOutpoint {
                        transaction_id: TransactionId::from_bytes([219; 32]),
                        index: 0,
                    },
                    vec![],
                    0,
                ),
                input(outpoint(&proposed.tx, 2), vec![], 0),
                input(
                    TransactionOutpoint {
                        transaction_id: TransactionId::from_bytes([216; 32]),
                        index: 0,
                    },
                    vec![],
                    0,
                ),
                input(
                    TransactionOutpoint {
                        transaction_id: TransactionId::from_bytes([217; 32]),
                        index: 0,
                    },
                    vec![],
                    0,
                ),
            ],
            outputs.clone(),
            0,
            Default::default(),
            0,
            vec![],
        );
        let signature = sign(unsigned, entries.clone(), 5, signer);
        let tx = Transaction::new(
            1,
            vec![
                input(
                    outpoint(&proposed.tx, 0),
                    covenant_call(
                        &proposed.governance_pending,
                        "vetoModulePolicy",
                        vec![governance_state(
                            &proposed.p,
                            &cleared,
                            Hash::from_bytes([0; 32]),
                            "State",
                        )],
                        true,
                    ),
                    0,
                ),
                input(
                    outpoint(&proposed.tx, 1),
                    entry_call(
                        &proposed.proposal_pending,
                        "vetoPolicy",
                        vec![
                            Expr::array(
                                parse_type_ref("DelegationState[]").unwrap(),
                                vec![delegation_state(
                                    owner_pk.clone(),
                                    delegate_pk.clone(),
                                    amount,
                                    weight,
                                    "DelegationState",
                                )],
                            ),
                            Expr::array(
                                parse_type_ref("TokenState[]").unwrap(),
                                vec![token(
                                    "TokenState",
                                    DELEGATION.as_bytes().to_vec(),
                                    amount,
                                    2,
                                    false,
                                )],
                            ),
                            Expr::dynamic_bytes(vec![5]),
                            Expr::byte(2),
                            reserve_state(
                                &proposed.credited_reserve_p,
                                proposed.stack.base.auction.prefix.len(),
                                proposed.stack.base.auction.suffix.len(),
                                proposed.stack.base.auction.template_hash.clone(),
                                "ReserveState",
                            ),
                            token(
                                "TokenState",
                                proposed.p.reserve_id.clone(),
                                proposed.credited_reserve_p.reserve_kusd,
                                2,
                                false,
                            ),
                            Expr::byte(5),
                        ],
                    ),
                    0,
                ),
                input(
                    TransactionOutpoint {
                        transaction_id: TransactionId::from_bytes([215; 32]),
                        index: 0,
                    },
                    covenant_call(
                        &reserve,
                        "collectProposalFeePolicy",
                        vec![
                            reserve_state(
                                &proposed.credited_reserve_p,
                                proposed.stack.base.auction.prefix.len(),
                                proposed.stack.base.auction.suffix.len(),
                                proposed.stack.base.auction.template_hash.clone(),
                                "State",
                            ),
                            Expr::bytes(proposed.proposal_id.as_bytes().to_vec()),
                            Expr::int(proposed.g1.proposal_fee_kusd),
                            token(
                                "TokenState",
                                proposed.p.reserve_id.clone(),
                                proposed.credited_reserve_p.reserve_kusd,
                                2,
                                false,
                            ),
                        ],
                        true,
                    ),
                    0,
                ),
                input(
                    TransactionOutpoint {
                        transaction_id: TransactionId::from_bytes([219; 32]),
                        index: 0,
                    },
                    covenant_call(
                        &proposed.reserve_token,
                        "transferPolicy",
                        vec![
                            Expr::array(
                                parse_type_ref("State[]").unwrap(),
                                vec![token(
                                    "State",
                                    proposed.p.reserve_id.clone(),
                                    proposed.credited_reserve_p.reserve_kusd,
                                    2,
                                    false,
                                )],
                            ),
                            Expr::bytes(vec![0; 65]),
                            Expr::byte(0),
                        ],
                        true,
                    ),
                    0,
                ),
                input(
                    outpoint(&proposed.tx, 2),
                    covenant_call(
                        &proposed.fee_escrow,
                        "transferPolicy",
                        vec![Expr::bytes(vec![0; 65]), Expr::byte(0)],
                        false,
                    ),
                    0,
                ),
                input(
                    TransactionOutpoint {
                        transaction_id: TransactionId::from_bytes([216; 32]),
                        index: 0,
                    },
                    entry_call(
                        &delegation,
                        "vetoPolicy",
                        vec![
                            Expr::bytes(signature),
                            Expr::bytes(proposed.proposal_id.as_bytes().to_vec()),
                            delegation_state(owner_pk, delegate_pk, amount, weight, "State"),
                            token(
                                "TokenState",
                                DELEGATION.as_bytes().to_vec(),
                                amount,
                                2,
                                false,
                            ),
                            Expr::byte(6),
                            Expr::byte(4),
                        ],
                    ),
                    0,
                ),
                input(
                    TransactionOutpoint {
                        transaction_id: TransactionId::from_bytes([217; 32]),
                        index: 0,
                    },
                    covenant_call(
                        &kps,
                        "transferPolicy",
                        vec![
                            Expr::array(
                                parse_type_ref("State[]").unwrap(),
                                vec![token(
                                    "State",
                                    DELEGATION.as_bytes().to_vec(),
                                    amount,
                                    2,
                                    false,
                                )],
                            ),
                            Expr::bytes(vec![0; 65]),
                            Expr::byte(0),
                        ],
                        true,
                    ),
                    0,
                ),
            ],
            outputs,
            0,
            Default::default(),
            0,
            vec![],
        );
        (tx, entries)
    };

    let (valid, entries) = build(VETO, 4, &voter);
    for index in 0..7 {
        execute(&valid, entries.clone(), index)
            .unwrap_or_else(|e| panic!("veto input {index}: {e}"));
    }
    let uncredited_reserve = compile(
        &reserve_source,
        &reserve_args(
            &proposed.p,
            &proposed.stack.base,
            proposed.p.reserve_kusd,
            TOTAL_KPS,
            0,
        ),
    );
    let mut refunded_on_veto = valid.clone();
    refunded_on_veto.outputs[1].script_public_key =
        pay_to_script_hash_script(&uncredited_reserve.bytecode);
    assert!(execute(&refunded_on_veto, entries.clone(), 1).is_err());
    let (short, short_entries) = build(VETO - 1, 4, &voter);
    assert!(execute(&short, short_entries, 1).is_err());
    let (underweighted, underweighted_entries) = build(VETO, 3, &voter);
    assert!(execute(&underweighted, underweighted_entries, 1).is_err());
    let attacker = Keypair::from_secret_key(
        &Secp256k1::new(),
        &SecretKey::from_slice(&[42; 32]).unwrap(),
    );
    let (wrong_delegate, wrong_delegate_entries) = build(VETO, 4, &attacker);
    assert!(execute(&wrong_delegate, wrong_delegate_entries, 5).is_err());

    let mut duplicated = valid.clone();
    duplicated.inputs.push(valid.inputs[5].clone());
    let mut duplicate_entries = entries.clone();
    duplicate_entries.push(entries[5].clone());
    assert!(execute(&duplicated, duplicate_entries, 1).is_err());
}

#[test]
fn kps_lock_authenticates_zero_weight_delegation_genesis() {
    let (p, g, owner) = setup();
    let stack = compile_governance_stack(&p, &g).unwrap();
    let owner_pk = owner.x_only_public_key().0.serialize().to_vec();
    let delegate = Keypair::from_secret_key(
        &Secp256k1::new(),
        &SecretKey::from_slice(&[45; 32]).unwrap(),
    );
    let delegate_pk = delegate.x_only_public_key().0.serialize().to_vec();
    let amount = 25_000_000;
    let delegation_source = std::fs::read_to_string("contracts/kps-delegation.sil").unwrap();
    let kps_source = std::fs::read_to_string("contracts/kps.sil").unwrap();
    let delegation0 = compile(
        &delegation_source,
        &delegation_args(&p, owner_pk.clone(), delegate_pk.clone(), amount, 0),
    );
    let funding = TransactionOutpoint {
        transaction_id: TransactionId::from_bytes([218; 32]),
        index: 0,
    };
    let delegation_unbound = TransactionOutput {
        value: 1_000,
        script_public_key: pay_to_script_hash_script(&delegation0.bytecode),
        covenant: None,
    };
    let delegation_id =
        hashing::covenant_id::covenant_id(funding, std::iter::once((1, &delegation_unbound)));
    let owner_kps = compile(
        &kps_source,
        &kps_args(
            &p,
            &stack.base.delegation,
            owner_pk.clone(),
            amount,
            0,
            false,
        ),
    );
    let locked_kps = compile(
        &kps_source,
        &kps_args(
            &p,
            &stack.base.delegation,
            delegation_id.as_bytes().to_vec(),
            amount,
            2,
            false,
        ),
    );
    let outputs = vec![
        output(&locked_kps, 1_000, 0, KPS),
        TransactionOutput {
            covenant: Some(CovenantBinding {
                authorizing_input: 1,
                covenant_id: delegation_id,
            }),
            ..delegation_unbound
        },
    ];
    let entries = vec![
        UtxoEntry::new(
            1_000,
            pay_to_script_hash_script(&owner_kps.bytecode),
            0,
            false,
            Some(KPS),
        ),
        UtxoEntry::new(
            1_000,
            kaspa_consensus_core::tx::ScriptPublicKey::new(0, vec![OpTrue].into()),
            0,
            false,
            None,
        ),
    ];
    let unsigned = Transaction::new(
        1,
        vec![
            input(
                TransactionOutpoint {
                    transaction_id: TransactionId::from_bytes([219; 32]),
                    index: 0,
                },
                vec![],
                0,
            ),
            input(funding, vec![], 0),
        ],
        outputs.clone(),
        0,
        Default::default(),
        0,
        vec![],
    );
    let signature = sign(unsigned, entries.clone(), 0, &owner);
    let lock = |weight: i64| {
        Transaction::new(
            1,
            vec![
                input(
                    TransactionOutpoint {
                        transaction_id: TransactionId::from_bytes([219; 32]),
                        index: 0,
                    },
                    entry_call(
                        &owner_kps,
                        "lockDelegationPolicy",
                        vec![
                            Expr::bytes(signature.clone()),
                            Expr::byte(0),
                            delegation_state(
                                owner_pk.clone(),
                                delegate_pk.clone(),
                                amount,
                                weight,
                                "DelegationState",
                            ),
                            token("State", delegation_id.as_bytes().to_vec(), amount, 2, false),
                            Expr::byte(1),
                            Expr::byte(0),
                        ],
                    ),
                    0,
                ),
                input(funding, vec![], 0),
            ],
            outputs.clone(),
            0,
            Default::default(),
            0,
            vec![],
        )
    };
    execute(&lock(0), entries.clone(), 0).expect("genese delegation poids zero");
    assert!(execute(&lock(4), entries.clone(), 0).is_err());

    let generic_outputs = vec![outputs[0].clone()];
    let generic_unsigned = Transaction::new(
        1,
        vec![input(
            TransactionOutpoint {
                transaction_id: TransactionId::from_bytes([219; 32]),
                index: 0,
            },
            vec![],
            0,
        )],
        generic_outputs.clone(),
        0,
        Default::default(),
        0,
        vec![],
    );
    let generic_signature = sign(generic_unsigned, vec![entries[0].clone()], 0, &owner);
    let generic = Transaction::new(
        1,
        vec![input(
            TransactionOutpoint {
                transaction_id: TransactionId::from_bytes([219; 32]),
                index: 0,
            },
            covenant_call(
                &owner_kps,
                "transferPolicy",
                vec![
                    Expr::array(
                        parse_type_ref("State[]").unwrap(),
                        vec![token(
                            "State",
                            delegation_id.as_bytes().to_vec(),
                            amount,
                            2,
                            false,
                        )],
                    ),
                    Expr::bytes(generic_signature),
                    Expr::byte(0),
                ],
                true,
            ),
            0,
        )],
        generic_outputs,
        0,
        Default::default(),
        0,
        vec![],
    );
    assert!(execute(&generic, vec![entries[0].clone()], 0).is_err());
}

#[test]
fn delegation_maturity_redelegation_and_owner_unlock_are_chained() {
    let (p, g, owner) = setup();
    let stack = compile_governance_stack(&p, &g).unwrap();
    let delegate = Keypair::from_secret_key(
        &Secp256k1::new(),
        &SecretKey::from_slice(&[43; 32]).unwrap(),
    );
    let next_delegate = Keypair::from_secret_key(
        &Secp256k1::new(),
        &SecretKey::from_slice(&[44; 32]).unwrap(),
    );
    let owner_pk = owner.x_only_public_key().0.serialize().to_vec();
    let delegate_pk = delegate.x_only_public_key().0.serialize().to_vec();
    let next_delegate_pk = next_delegate.x_only_public_key().0.serialize().to_vec();
    let amount = 25_000_000;
    let delegation_source = std::fs::read_to_string("contracts/kps-delegation.sil").unwrap();
    let kps_source = std::fs::read_to_string("contracts/kps.sil").unwrap();
    let delegation0 = compile(
        &delegation_source,
        &delegation_args(&p, owner_pk.clone(), delegate_pk.clone(), amount, 0),
    );
    let delegation1 = compile(
        &delegation_source,
        &delegation_args(&p, owner_pk.clone(), delegate_pk.clone(), amount, 1),
    );
    let delegation2 = compile(
        &delegation_source,
        &delegation_args(&p, owner_pk.clone(), delegate_pk.clone(), amount, 2),
    );
    let kps_locked = compile(
        &kps_source,
        &kps_args(
            &p,
            &stack.base.delegation,
            DELEGATION.as_bytes().to_vec(),
            amount,
            2,
            false,
        ),
    );
    let locked_token = || {
        token(
            "TokenState",
            DELEGATION.as_bytes().to_vec(),
            amount,
            2,
            false,
        )
    };
    let locked_state = || token("State", DELEGATION.as_bytes().to_vec(), amount, 2, false);
    let mature = |age: u64, next: &CompiledContract<'_>, next_weight: i64| {
        Transaction::new(
            1,
            vec![
                input(
                    TransactionOutpoint {
                        transaction_id: TransactionId::from_bytes([220; 32]),
                        index: 0,
                    },
                    covenant_call(
                        &delegation0,
                        "maturePolicy",
                        vec![
                            delegation_state(
                                owner_pk.clone(),
                                delegate_pk.clone(),
                                amount,
                                next_weight,
                                "State",
                            ),
                            locked_token(),
                            Expr::byte(1),
                            Expr::byte(1),
                        ],
                        true,
                    ),
                    age,
                ),
                input(
                    TransactionOutpoint {
                        transaction_id: TransactionId::from_bytes([221; 32]),
                        index: 0,
                    },
                    covenant_call(
                        &kps_locked,
                        "transferPolicy",
                        vec![
                            Expr::array(parse_type_ref("State[]").unwrap(), vec![locked_state()]),
                            Expr::bytes(vec![0; 65]),
                            Expr::byte(0),
                        ],
                        true,
                    ),
                    0,
                ),
            ],
            vec![
                output(next, 1_000, 0, DELEGATION),
                output(&kps_locked, 1_000, 1, KPS),
            ],
            0,
            Default::default(),
            0,
            vec![],
        )
    };
    let initial_entries = vec![
        UtxoEntry::new(
            1_000,
            pay_to_script_hash_script(&delegation0.bytecode),
            0,
            false,
            Some(DELEGATION),
        ),
        UtxoEntry::new(
            1_000,
            pay_to_script_hash_script(&kps_locked.bytecode),
            0,
            false,
            Some(KPS),
        ),
    ];
    assert!(execute(&mature(89, &delegation1, 1), initial_entries.clone(), 0).is_err());
    assert!(execute(&mature(90, &delegation2, 2), initial_entries.clone(), 0).is_err());
    let matured = mature(90, &delegation1, 1);
    execute(&matured, initial_entries.clone(), 0).expect("maturity delegation");
    execute(&matured, initial_entries, 1).expect("KPS reste lie pendant maturity");

    let redelegated = compile(
        &delegation_source,
        &delegation_args(&p, owner_pk.clone(), next_delegate_pk.clone(), amount, 1),
    );
    let redelegate_outputs = vec![
        output(&redelegated, 1_000, 0, DELEGATION),
        output(&kps_locked, 1_000, 1, KPS),
    ];
    let redelegate_entries = vec![
        entry(&matured.outputs[0], DELEGATION),
        entry(&matured.outputs[1], KPS),
    ];
    let redelegate_unsigned = Transaction::new(
        1,
        vec![
            input(outpoint(&matured, 0), vec![], 0),
            input(outpoint(&matured, 1), vec![], 0),
        ],
        redelegate_outputs.clone(),
        0,
        Default::default(),
        0,
        vec![],
    );
    let owner_sig = sign(redelegate_unsigned, redelegate_entries.clone(), 0, &owner);
    let redelegate_tx = Transaction::new(
        1,
        vec![
            input(
                outpoint(&matured, 0),
                covenant_call(
                    &delegation1,
                    "redelegatePolicy",
                    vec![
                        delegation_state(owner_pk.clone(), next_delegate_pk, amount, 1, "State"),
                        Expr::bytes(owner_sig),
                        locked_token(),
                        Expr::byte(1),
                        Expr::byte(1),
                    ],
                    true,
                ),
                0,
            ),
            input(
                outpoint(&matured, 1),
                covenant_call(
                    &kps_locked,
                    "transferPolicy",
                    vec![
                        Expr::array(parse_type_ref("State[]").unwrap(), vec![locked_state()]),
                        Expr::bytes(vec![0; 65]),
                        Expr::byte(0),
                    ],
                    true,
                ),
                0,
            ),
        ],
        redelegate_outputs,
        0,
        Default::default(),
        0,
        vec![],
    );
    execute(&redelegate_tx, redelegate_entries.clone(), 0).expect("owner redelegation");
    execute(&redelegate_tx, redelegate_entries, 1).expect("KPS reste lie apres redelegation");

    let owner_token = token("TokenState", owner_pk.clone(), amount, 0, false);
    let owner_kps = compile(
        &kps_source,
        &kps_args(
            &p,
            &stack.base.delegation,
            owner_pk.clone(),
            amount,
            0,
            false,
        ),
    );
    let unlock_outputs = vec![output(&owner_kps, 1_000, 1, KPS)];
    let unlock_entries = vec![
        entry(&redelegate_tx.outputs[0], DELEGATION),
        entry(&redelegate_tx.outputs[1], KPS),
    ];
    let unlock_unsigned = Transaction::new(
        1,
        vec![
            input(outpoint(&redelegate_tx, 0), vec![], 0),
            input(outpoint(&redelegate_tx, 1), vec![], 0),
        ],
        unlock_outputs.clone(),
        0,
        Default::default(),
        0,
        vec![],
    );
    let unlock_sig = sign(unlock_unsigned, unlock_entries.clone(), 0, &owner);
    let unlock = Transaction::new(
        1,
        vec![
            input(
                outpoint(&redelegate_tx, 0),
                entry_call(
                    &redelegated,
                    "unlockPolicy",
                    vec![
                        Expr::bytes(unlock_sig),
                        owner_token,
                        Expr::byte(1),
                        Expr::byte(0),
                    ],
                ),
                0,
            ),
            input(
                outpoint(&redelegate_tx, 1),
                entry_call(
                    &kps_locked,
                    "unlockDelegationPolicy",
                    vec![
                        delegation_state(
                            owner_pk.clone(),
                            next_delegate.x_only_public_key().0.serialize().to_vec(),
                            amount,
                            1,
                            "DelegationState",
                        ),
                        token("State", owner_pk, amount, 0, false),
                        Expr::byte(0),
                        Expr::byte(0),
                    ],
                ),
                0,
            ),
        ],
        unlock_outputs,
        0,
        Default::default(),
        0,
        vec![],
    );
    execute(&unlock, unlock_entries.clone(), 0).expect("owner unlock");
    execute(&unlock, unlock_entries, 1).expect("KPS rendu au owner");
}
