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
const KPS: Hash = Hash::from_bytes(*b"KKKKKKKKKKKKKKKKKKKKKKKKKKKKKKKK");
const RESERVE: Hash = Hash::from_bytes(*b"RRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRR");
const GOVERNOR: Hash = Hash::from_bytes(*b"GGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGG");
const CONTROLLER: Hash = Hash::from_bytes(*b"CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC");
const REGISTRY: Hash = Hash::from_bytes(*b"YYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYY");
const WRONG_RESERVE: Hash = Hash::from_bytes(*b"XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX");
const ZERO: Hash = Hash::from_bytes([0; 32]);

const CONTRACT_KAS: u64 = 1_000_000;
const ACCOUNT_KAS: u64 = 100_000_000;
const DEPOSIT: i64 = 10_000_000_000;
const RESERVE_BALANCE: i64 = 100_000_000_000;
const TOTAL_KPS: i64 = 50_000_000_000;
const RATE_PPM: i64 = 20_000;
const REFERRAL_PPM: i64 = 100_000;
const VOTING_DELAY: u64 = 100;
const EXECUTION_WINDOW: i64 = 1_000;
const INTEREST_DELAY: i64 = 50;
const MAX_ACCRUAL: i64 = 10_000;
const DAA_PER_YEAR: i64 = 10_000;
const ACCRUAL: i64 = 1_000;
const ANNUAL_INTEREST: i64 = 200_000_000;
const GROSS_INTEREST: i64 = 20_000_000;
const REFERRAL_FEE: i64 = 2_000_000;
const NET_INTEREST: i64 = 18_000_000;
const PROPOSAL_DEPOSIT: u64 = 100_000_000;

fn source(path: &str) -> String {
    std::fs::read_to_string(path).unwrap()
}

fn compile<'a>(source: &'a str, args: &[Expr<'a>]) -> CompiledContract<'a> {
    compile_contract(source, args, CompileOptions::default()).unwrap()
}

fn parts(c: &CompiledContract<'_>) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let l = c.state_layout;
    (
        c.bytecode[..l.start].to_vec(),
        c.bytecode[l.start + l.len..].to_vec(),
        c.template_hash().to_vec(),
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

fn outpoint(tx: &Transaction, index: u32) -> TransactionOutpoint {
    TransactionOutpoint {
        transaction_id: tx.id(),
        index,
    }
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

fn sign(tx: Transaction, entries: Vec<UtxoEntry>, index: usize, key: &Keypair) -> Vec<u8> {
    let mutable = MutableTransaction::with_entries(tx, entries);
    let reused = SigHashReusedValuesUnsync::new();
    let hash = calc_schnorr_signature_hash(&mutable.as_verifiable(), index, SIG_HASH_ALL, &reused);
    let message = secp256k1::Message::from_digest_slice(hash.as_bytes().as_slice()).unwrap();
    let mut signature = key.sign_schnorr(message).as_ref().to_vec();
    signature.push(SIG_HASH_ALL.to_u8());
    signature
}

fn fixture(n: u8, index: u32) -> TransactionOutpoint {
    TransactionOutpoint {
        transaction_id: TransactionId::from_bytes([n; 32]),
        index,
    }
}

fn token(name: &'static str, owner: Vec<u8>, kind: u8, amount: i64) -> Expr<'static> {
    struct_object(
        name,
        vec![
            ("ownerIdentifier", Expr::bytes(owner)),
            ("identifierType", Expr::byte(kind)),
            ("amount", Expr::int(amount)),
            ("isMinter", Expr::bool(false)),
        ],
    )
}

fn token_states(values: Vec<(Vec<u8>, u8, i64)>) -> Expr<'static> {
    Expr::array(
        parse_type_ref("State[]").unwrap(),
        values
            .into_iter()
            .map(|(o, k, a)| token("State", o, k, a))
            .collect(),
    )
}

fn proposal_state(name: &'static str, proposer: Vec<u8>, activated: bool) -> Expr<'static> {
    struct_object(
        name,
        vec![
            ("proposer", Expr::bytes(proposer)),
            ("governanceId", Expr::bytes(GOVERNOR.as_bytes().to_vec())),
            ("controllerId", Expr::bytes(CONTROLLER.as_bytes().to_vec())),
            ("assetId", Expr::bytes(ASSET.as_bytes().to_vec())),
            ("proposalNonce", Expr::int(1)),
            ("activated", Expr::bool(activated)),
            ("ratePpm", Expr::int(RATE_PPM)),
            ("interestDelayDaa", Expr::int(INTEREST_DELAY)),
            ("maxAccrualDaa", Expr::int(MAX_ACCRUAL)),
            ("daaPerYear", Expr::int(DAA_PER_YEAR)),
            ("votingDelayDaa", Expr::int(VOTING_DELAY as i64)),
            ("executionWindowDaa", Expr::int(EXECUTION_WINDOW)),
            ("depositSompi", Expr::int(PROPOSAL_DEPOSIT as i64)),
        ],
    )
}

#[derive(Clone)]
struct Routes {
    controller: (Vec<u8>, Vec<u8>, Vec<u8>),
    reserve: (Vec<u8>, Vec<u8>, Vec<u8>),
}

fn governor_state(
    name: &'static str,
    active: Hash,
    proposal_nonce: i64,
    execution_nonce: i64,
    routes: &Routes,
) -> Expr<'static> {
    struct_object(
        name,
        vec![
            ("assetId", Expr::bytes(ASSET.as_bytes().to_vec())),
            ("kpsId", Expr::bytes(KPS.as_bytes().to_vec())),
            ("reserveId", Expr::bytes(RESERVE.as_bytes().to_vec())),
            ("controllerId", Expr::bytes(CONTROLLER.as_bytes().to_vec())),
            ("proposalNonce", Expr::int(proposal_nonce)),
            ("executionNonce", Expr::int(execution_nonce)),
            ("activeProposalId", Expr::bytes(active.as_bytes().to_vec())),
            ("votingDelayDaa", Expr::int(VOTING_DELAY as i64)),
            ("executionWindowDaa", Expr::int(EXECUTION_WINDOW)),
            ("vetoThresholdPpm", Expr::int(200_000)),
            ("proposalDepositSompi", Expr::int(PROPOSAL_DEPOSIT as i64)),
            (
                "reservePrefixLenState",
                Expr::int(routes.reserve.0.len() as i64),
            ),
            (
                "reserveSuffixLenState",
                Expr::int(routes.reserve.1.len() as i64),
            ),
            (
                "reserveTemplateHashState",
                Expr::bytes(routes.reserve.2.clone()),
            ),
        ],
    )
}

fn controller_state(
    name: &'static str,
    enabled: bool,
    series_nonce: i64,
    account_nonce: i64,
    total_saved: i64,
    routes: &Routes,
) -> Expr<'static> {
    struct_object(
        name,
        vec![
            ("assetId", Expr::bytes(ASSET.as_bytes().to_vec())),
            ("reserveId", Expr::bytes(RESERVE.as_bytes().to_vec())),
            ("governanceId", Expr::bytes(GOVERNOR.as_bytes().to_vec())),
            ("enabled", Expr::bool(enabled)),
            ("seriesNonce", Expr::int(series_nonce)),
            ("accountNonce", Expr::int(account_nonce)),
            ("totalSaved", Expr::int(total_saved)),
            (
                "currentRatePpm",
                Expr::int(if enabled { RATE_PPM } else { 0 }),
            ),
            ("interestDelayDaa", Expr::int(INTEREST_DELAY)),
            ("maxAccrualDaa", Expr::int(MAX_ACCRUAL)),
            ("daaPerYear", Expr::int(DAA_PER_YEAR)),
            ("accountValueSompi", Expr::int(ACCOUNT_KAS as i64)),
            (
                "controllerPrefixLenState",
                Expr::int(routes.controller.0.len() as i64),
            ),
            (
                "controllerSuffixLenState",
                Expr::int(routes.controller.1.len() as i64),
            ),
            (
                "controllerTemplateHashState",
                Expr::bytes(routes.controller.2.clone()),
            ),
            (
                "reservePrefixLenState",
                Expr::int(routes.reserve.0.len() as i64),
            ),
            (
                "reserveSuffixLenState",
                Expr::int(routes.reserve.1.len() as i64),
            ),
            (
                "reserveTemplateHashState",
                Expr::bytes(routes.reserve.2.clone()),
            ),
        ],
    )
}

fn account_state(
    name: &'static str,
    owner: Vec<u8>,
    referrer: Vec<u8>,
    saved: i64,
    delay: i64,
    remaining: i64,
    routes: &Routes,
) -> Expr<'static> {
    struct_object(
        name,
        vec![
            ("owner", Expr::bytes(owner)),
            ("controllerId", Expr::bytes(CONTROLLER.as_bytes().to_vec())),
            ("reserveId", Expr::bytes(RESERVE.as_bytes().to_vec())),
            ("assetId", Expr::bytes(ASSET.as_bytes().to_vec())),
            ("saved", Expr::int(saved)),
            ("ratePpm", Expr::int(RATE_PPM)),
            ("delayRemainingDaa", Expr::int(delay)),
            ("remainingAccrualDaa", Expr::int(remaining)),
            ("daaPerYear", Expr::int(DAA_PER_YEAR)),
            ("referrer", Expr::bytes(referrer)),
            ("referralFeePpm", Expr::int(REFERRAL_PPM)),
            (
                "controllerPrefixLen",
                Expr::int(routes.controller.0.len() as i64),
            ),
            (
                "controllerSuffixLen",
                Expr::int(routes.controller.1.len() as i64),
            ),
            (
                "controllerTemplateHash",
                Expr::bytes(routes.controller.2.clone()),
            ),
            ("reservePrefixLen", Expr::int(routes.reserve.0.len() as i64)),
            ("reserveSuffixLen", Expr::int(routes.reserve.1.len() as i64)),
            ("reserveTemplateHash", Expr::bytes(routes.reserve.2.clone())),
        ],
    )
}

fn reserve_state(name: &'static str, balance: i64) -> Expr<'static> {
    struct_object(
        name,
        vec![
            ("kusdAssetId", Expr::bytes(ASSET.as_bytes().to_vec())),
            ("kpsAssetId", Expr::bytes(KPS.as_bytes().to_vec())),
            ("reserveKusd", Expr::int(balance)),
            ("totalKps", Expr::int(TOTAL_KPS)),
            ("collateralSompi", Expr::int(0)),
            ("auctionPrefixLenState", Expr::int(1)),
            ("auctionSuffixLenState", Expr::int(1)),
            ("auctionTemplateHashState", Expr::bytes(vec![0x77; 32])),
        ],
    )
}

fn kcc_args(owner: Vec<u8>, amount: i64, kind: u8) -> Vec<Expr<'static>> {
    vec![
        Expr::bytes(owner),
        Expr::int(amount),
        Expr::byte(kind),
        Expr::bool(false),
        Expr::int(4),
        Expr::int(4),
    ]
}

fn account_args(
    owner: Vec<u8>,
    referrer: Vec<u8>,
    saved: i64,
    delay: i64,
    remaining: i64,
    routes: &Routes,
    kcc: &(Vec<u8>, Vec<u8>, Vec<u8>),
) -> Vec<Expr<'static>> {
    vec![
        Expr::bytes(owner),
        Expr::bytes(CONTROLLER.as_bytes().to_vec()),
        Expr::bytes(RESERVE.as_bytes().to_vec()),
        Expr::bytes(ASSET.as_bytes().to_vec()),
        Expr::int(saved),
        Expr::int(RATE_PPM),
        Expr::int(delay),
        Expr::int(remaining),
        Expr::int(DAA_PER_YEAR),
        Expr::bytes(referrer),
        Expr::int(REFERRAL_PPM),
        Expr::int(kcc.0.len() as i64),
        Expr::int(kcc.1.len() as i64),
        Expr::bytes(kcc.2.clone()),
        Expr::int(routes.controller.0.len() as i64),
        Expr::int(routes.controller.1.len() as i64),
        Expr::bytes(routes.controller.2.clone()),
        Expr::int(routes.reserve.0.len() as i64),
        Expr::int(routes.reserve.1.len() as i64),
        Expr::bytes(routes.reserve.2.clone()),
    ]
}

fn proposal_args(proposer: Vec<u8>, activated: bool) -> Vec<Expr<'static>> {
    vec![
        Expr::bytes(proposer),
        Expr::bytes(GOVERNOR.as_bytes().to_vec()),
        Expr::bytes(CONTROLLER.as_bytes().to_vec()),
        Expr::bytes(ASSET.as_bytes().to_vec()),
        Expr::int(1),
        Expr::bool(activated),
        Expr::int(RATE_PPM),
        Expr::int(INTEREST_DELAY),
        Expr::int(MAX_ACCRUAL),
        Expr::int(DAA_PER_YEAR),
        Expr::int(VOTING_DELAY as i64),
        Expr::int(EXECUTION_WINDOW),
        Expr::int(PROPOSAL_DEPOSIT as i64),
    ]
}

fn governor_args(
    active: Hash,
    proposal_nonce: i64,
    execution_nonce: i64,
    proposal_route: &(Vec<u8>, Vec<u8>, Vec<u8>),
    reserve_route: &(Vec<u8>, Vec<u8>, Vec<u8>),
) -> Vec<Expr<'static>> {
    vec![
        Expr::bytes(ASSET.as_bytes().to_vec()),
        Expr::bytes(KPS.as_bytes().to_vec()),
        Expr::bytes(RESERVE.as_bytes().to_vec()),
        Expr::bytes(CONTROLLER.as_bytes().to_vec()),
        Expr::int(proposal_nonce),
        Expr::int(execution_nonce),
        Expr::bytes(active.as_bytes().to_vec()),
        Expr::int(VOTING_DELAY as i64),
        Expr::int(EXECUTION_WINDOW),
        Expr::int(200_000),
        Expr::int(PROPOSAL_DEPOSIT as i64),
        Expr::dynamic_bytes(proposal_route.0.clone()),
        Expr::dynamic_bytes(proposal_route.1.clone()),
        Expr::bytes(proposal_route.2.clone()),
        Expr::int(1),
        Expr::int(1),
        Expr::bytes(vec![0x41; 32]),
        Expr::int(1),
        Expr::int(1),
        Expr::bytes(vec![0x42; 32]),
        Expr::int(reserve_route.0.len() as i64),
        Expr::int(reserve_route.1.len() as i64),
        Expr::bytes(reserve_route.2.clone()),
        Expr::int(4),
    ]
}

fn controller_args(
    enabled: bool,
    series_nonce: i64,
    account_nonce: i64,
    total_saved: i64,
    routes: &Routes,
    kcc: &(Vec<u8>, Vec<u8>, Vec<u8>),
    account: &(Vec<u8>, Vec<u8>, Vec<u8>),
    governor: &(Vec<u8>, Vec<u8>, Vec<u8>),
    proposal: &(Vec<u8>, Vec<u8>, Vec<u8>),
) -> Vec<Expr<'static>> {
    vec![
        Expr::bytes(vec![0; 32]),
        Expr::bytes(ASSET.as_bytes().to_vec()),
        Expr::bytes(RESERVE.as_bytes().to_vec()),
        Expr::bytes(GOVERNOR.as_bytes().to_vec()),
        Expr::bool(enabled),
        Expr::int(series_nonce),
        Expr::int(account_nonce),
        Expr::int(total_saved),
        Expr::int(if enabled { RATE_PPM } else { 0 }),
        Expr::int(INTEREST_DELAY),
        Expr::int(MAX_ACCRUAL),
        Expr::int(DAA_PER_YEAR),
        Expr::int(ACCOUNT_KAS as i64),
        Expr::int(routes.controller.0.len() as i64),
        Expr::int(routes.controller.1.len() as i64),
        Expr::bytes(routes.controller.2.clone()),
        Expr::int(routes.reserve.0.len() as i64),
        Expr::int(routes.reserve.1.len() as i64),
        Expr::bytes(routes.reserve.2.clone()),
        Expr::int(kcc.0.len() as i64),
        Expr::int(kcc.1.len() as i64),
        Expr::bytes(kcc.2.clone()),
        Expr::int(account.0.len() as i64),
        Expr::int(account.1.len() as i64),
        Expr::bytes(account.2.clone()),
        Expr::int(governor.0.len() as i64),
        Expr::int(governor.1.len() as i64),
        Expr::bytes(governor.2.clone()),
        Expr::int(proposal.0.len() as i64),
        Expr::int(proposal.1.len() as i64),
        Expr::bytes(proposal.2.clone()),
    ]
}

fn reserve_args(
    balance: i64,
    kcc: &(Vec<u8>, Vec<u8>, Vec<u8>),
    account: &(Vec<u8>, Vec<u8>, Vec<u8>),
    controller: &(Vec<u8>, Vec<u8>, Vec<u8>),
    registry: &(Vec<u8>, Vec<u8>, Vec<u8>),
) -> Vec<Expr<'static>> {
    vec![
        Expr::bytes(ASSET.as_bytes().to_vec()),
        Expr::bytes(KPS.as_bytes().to_vec()),
        Expr::int(balance),
        Expr::int(TOTAL_KPS),
        Expr::int(0),
        Expr::int(kcc.0.len() as i64),
        Expr::int(kcc.1.len() as i64),
        Expr::bytes(kcc.2.clone()),
        Expr::int(1),
        Expr::int(1),
        Expr::bytes(vec![0x61; 32]),
        Expr::int(1),
        Expr::int(1),
        Expr::bytes(vec![0x77; 32]),
        Expr::bytes(REGISTRY.as_bytes().to_vec()),
        Expr::int(registry.0.len() as i64),
        Expr::int(registry.1.len() as i64),
        Expr::bytes(registry.2.clone()),
        Expr::int(account.0.len() as i64),
        Expr::int(account.1.len() as i64),
        Expr::bytes(account.2.clone()),
        Expr::int(controller.0.len() as i64),
        Expr::int(controller.1.len() as i64),
        Expr::bytes(controller.2.clone()),
    ]
}

#[test]
fn chained_governance_savings_refresh_and_withdraw_rejects_fraud() {
    assert_eq!(ANNUAL_INTEREST, DEPOSIT * RATE_PPM / 1_000_000);
    assert_eq!(GROSS_INTEREST, ANNUAL_INTEREST * ACCRUAL / DAA_PER_YEAR);
    assert_eq!(REFERRAL_FEE, GROSS_INTEREST * REFERRAL_PPM / 1_000_000);
    assert_eq!(NET_INTEREST, GROSS_INTEREST - REFERRAL_FEE);

    let secp = Secp256k1::new();
    let owner_key = Keypair::from_secret_key(&secp, &SecretKey::from_slice(&[21; 32]).unwrap());
    let proposer_key = Keypair::from_secret_key(&secp, &SecretKey::from_slice(&[22; 32]).unwrap());
    let referral_key = Keypair::from_secret_key(&secp, &SecretKey::from_slice(&[23; 32]).unwrap());
    let wrong_key = Keypair::from_secret_key(&secp, &SecretKey::from_slice(&[24; 32]).unwrap());
    let owner = owner_key.x_only_public_key().0.serialize().to_vec();
    let proposer = proposer_key.x_only_public_key().0.serialize().to_vec();
    let referral = referral_key.x_only_public_key().0.serialize().to_vec();
    let wrong_referral = wrong_key.x_only_public_key().0.serialize().to_vec();

    let kcc_src = source("contracts/kcc20.sil");
    let account_src = source("contracts/savings-account.sil");
    let controller_src = source("contracts/savings-controller.sil");
    let proposal_src = source("contracts/savings-proposal.sil");
    let governor_src = source("contracts/savings-governor.sil");
    let reserve_src = source("contracts/equity-reserve.sil");
    let registry_src = source("contracts/savings-registry.sil");
    let registry = compile(
        &registry_src,
        &[
            Expr::bytes(owner.clone()),
            Expr::bytes(CONTROLLER.as_bytes().to_vec()),
            Expr::bool(true),
        ],
    );
    let registry_route = parts(&registry);

    let kcc_probe = compile(&kcc_src, &kcc_args(vec![0; 32], 0, 2));
    let kcc_route = parts(&kcc_probe);
    let proposal_probe = compile(&proposal_src, &proposal_args(proposer.clone(), false));
    let proposal_route = parts(&proposal_probe);
    let dummy_route = (vec![0], vec![0], vec![0; 32]);
    let governor_probe = compile(
        &governor_src,
        &governor_args(ZERO, 0, 0, &proposal_route, &dummy_route),
    );
    let governor_route = parts(&governor_probe);
    let dummy_routes = Routes {
        controller: dummy_route.clone(),
        reserve: dummy_route.clone(),
    };
    let account_probe = compile(
        &account_src,
        &account_args(
            owner.clone(),
            referral.clone(),
            DEPOSIT,
            INTEREST_DELAY,
            MAX_ACCRUAL,
            &dummy_routes,
            &kcc_route,
        ),
    );
    let account_route = parts(&account_probe);
    let controller_probe = compile(
        &controller_src,
        &controller_args(
            false,
            0,
            0,
            0,
            &dummy_routes,
            &kcc_route,
            &account_route,
            &governor_route,
            &proposal_route,
        ),
    );
    let controller_route = parts(&controller_probe);
    let reserve_probe = compile(
        &reserve_src,
        &reserve_args(
            RESERVE_BALANCE,
            &kcc_route,
            &account_route,
            &controller_route,
            &registry_route,
        ),
    );
    let reserve_route = parts(&reserve_probe);
    let routes = Routes {
        controller: controller_route,
        reserve: reserve_route,
    };

    let proposal_pending = compile(&proposal_src, &proposal_args(proposer.clone(), false));
    let proposal_active = compile(&proposal_src, &proposal_args(proposer.clone(), true));
    let proposal_unbound = TransactionOutput {
        value: PROPOSAL_DEPOSIT,
        script_public_key: pay_to_script_hash_script(&proposal_pending.bytecode),
        covenant: None,
    };
    let proposal_id =
        hashing::covenant_id::covenant_id(fixture(1, 0), std::iter::once((1, &proposal_unbound)));
    let governor_initial = compile(
        &governor_src,
        &governor_args(ZERO, 0, 0, &proposal_route, &routes.reserve),
    );
    let governor_pending = compile(
        &governor_src,
        &governor_args(proposal_id, 1, 0, &proposal_route, &routes.reserve),
    );
    let governor_executed = compile(
        &governor_src,
        &governor_args(ZERO, 1, 1, &proposal_route, &routes.reserve),
    );
    let controller_disabled = compile(
        &controller_src,
        &controller_args(
            false,
            0,
            0,
            0,
            &routes,
            &kcc_route,
            &account_route,
            &governor_route,
            &proposal_route,
        ),
    );
    let controller_enabled = compile(
        &controller_src,
        &controller_args(
            true,
            1,
            0,
            0,
            &routes,
            &kcc_route,
            &account_route,
            &governor_route,
            &proposal_route,
        ),
    );
    assert_eq!(parts(&governor_initial), governor_route);
    assert_eq!(parts(&controller_disabled), routes.controller);
    assert_eq!(parts(&reserve_probe), routes.reserve);

    // 1. Proposal: Governor + KAS deposit -> Governor' + Proposal.
    let propose_entries = vec![
        UtxoEntry::new(
            CONTRACT_KAS,
            pay_to_script_hash_script(&governor_initial.bytecode),
            0,
            false,
            Some(GOVERNOR),
        ),
        UtxoEntry::new(
            PROPOSAL_DEPOSIT,
            kaspa_consensus_core::tx::ScriptPublicKey::new(0, vec![OpTrue].into()),
            0,
            false,
            None,
        ),
    ];
    let propose = Transaction::new(
        1,
        vec![
            input(
                fixture(1, 0),
                covenant_call(
                    &governor_initial,
                    "proposeSeriesPolicy",
                    vec![
                        governor_state("State", proposal_id, 1, 0, &routes),
                        Expr::byte(1),
                        proposal_state("ProposalState", proposer.clone(), false),
                    ],
                    true,
                ),
                0,
            ),
            input(fixture(2, 0), vec![], 0),
        ],
        vec![
            output(&governor_pending, CONTRACT_KAS, 0, GOVERNOR),
            output(&proposal_pending, PROPOSAL_DEPOSIT, 0, proposal_id),
        ],
        0,
        Default::default(),
        0,
        vec![],
    );
    execute(&propose, propose_entries, 0).expect("Savings proposal");

    // 2. Permissionless activation: the same outpoint fails immediately before maturity.
    let build_activation = |age: u64| {
        Transaction::new(
            1,
            vec![input(
                outpoint(&propose, 1),
                covenant_call(
                    &proposal_pending,
                    "activatePolicy",
                    vec![proposal_state("State", proposer.clone(), true)],
                    true,
                ),
                age,
            )],
            vec![output(&proposal_active, PROPOSAL_DEPOSIT, 0, proposal_id)],
            0,
            Default::default(),
            0,
            vec![],
        )
    };
    let proposal_entry = vec![entry(&propose.outputs[1], proposal_id)];
    assert!(
        execute(
            &build_activation(VOTING_DELAY - 1),
            proposal_entry.clone(),
            0
        )
        .is_err()
    );
    let activation = build_activation(VOTING_DELAY);
    execute(&activation, proposal_entry, 0).expect("Savings activation mature");

    // 3. Execution: produced Governor and Proposal outpoints enable Controller.
    let proposer_spk =
        pay_to_address_script(&Address::new(Prefix::Testnet, Version::PubKey, &proposer));
    let execute_entries = vec![
        entry(&propose.outputs[0], GOVERNOR),
        entry(&activation.outputs[0], proposal_id),
        UtxoEntry::new(
            CONTRACT_KAS,
            pay_to_script_hash_script(&controller_disabled.bytecode),
            0,
            false,
            Some(CONTROLLER),
        ),
    ];
    let execute_series = Transaction::new(
        1,
        vec![
            input(
                outpoint(&propose, 0),
                covenant_call(
                    &governor_pending,
                    "executeSeriesPolicy",
                    vec![
                        governor_state("State", ZERO, 1, 1, &routes),
                        Expr::byte(1),
                        proposal_state("ProposalState", proposer.clone(), true),
                    ],
                    true,
                ),
                0,
            ),
            input(
                outpoint(&activation, 0),
                entry_call(&proposal_active, "executePolicy", vec![Expr::byte(2)]),
                0,
            ),
            input(
                fixture(3, 0),
                covenant_call(
                    &controller_disabled,
                    "executeSeriesPolicy",
                    vec![
                        controller_state("State", true, 1, 0, 0, &routes),
                        Expr::byte(0),
                        Expr::byte(1),
                    ],
                    true,
                ),
                0,
            ),
        ],
        vec![
            output(&governor_executed, CONTRACT_KAS, 0, GOVERNOR),
            output(&controller_enabled, CONTRACT_KAS, 2, CONTROLLER),
            TransactionOutput {
                value: PROPOSAL_DEPOSIT,
                script_public_key: proposer_spk,
                covenant: None,
            },
        ],
        0,
        Default::default(),
        0,
        vec![],
    );
    for index in 0..3 {
        execute(&execute_series, execute_entries.clone(), index)
            .unwrap_or_else(|e| panic!("execute series input {index}: {e}"));
    }

    // 4. Open: Controller creates the account and KCC20 bound to its Covenant ID.
    let account_open = compile(
        &account_src,
        &account_args(
            owner.clone(),
            referral.clone(),
            DEPOSIT,
            INTEREST_DELAY,
            MAX_ACCRUAL,
            &routes,
            &kcc_route,
        ),
    );
    let account_unbound = TransactionOutput {
        value: ACCOUNT_KAS,
        script_public_key: pay_to_script_hash_script(&account_open.bytecode),
        covenant: None,
    };
    let account_id = hashing::covenant_id::covenant_id(
        outpoint(&execute_series, 1),
        std::iter::once((1, &account_unbound)),
    );
    let controller_open = compile(
        &controller_src,
        &controller_args(
            true,
            1,
            1,
            DEPOSIT,
            &routes,
            &kcc_route,
            &account_route,
            &governor_route,
            &proposal_route,
        ),
    );
    let user_token = compile(&kcc_src, &kcc_args(owner.clone(), DEPOSIT, 0));
    let account_token = compile(
        &kcc_src,
        &kcc_args(account_id.as_bytes().to_vec(), DEPOSIT, 2),
    );
    let open_entries = vec![
        entry(&execute_series.outputs[1], CONTROLLER),
        UtxoEntry::new(
            CONTRACT_KAS,
            pay_to_script_hash_script(&user_token.bytecode),
            0,
            false,
            Some(ASSET),
        ),
    ];
    let open_outputs = vec![
        output(&controller_open, CONTRACT_KAS, 0, CONTROLLER),
        output(&account_open, ACCOUNT_KAS, 0, account_id),
        output(&account_token, CONTRACT_KAS, 1, ASSET),
    ];
    let dummy_sig = vec![0; 65];
    let build_open = |token_sig: Vec<u8>| {
        Transaction::new(
            1,
            vec![
                input(
                    outpoint(&execute_series, 1),
                    covenant_call(
                        &controller_enabled,
                        "openAccountPolicy",
                        vec![
                            controller_state("State", true, 1, 1, DEPOSIT, &routes),
                            Expr::byte(1),
                            Expr::dynamic_bytes(account_route.0.clone()),
                            Expr::dynamic_bytes(account_route.1.clone()),
                            account_state(
                                "AccountState",
                                owner.clone(),
                                referral.clone(),
                                DEPOSIT,
                                INTEREST_DELAY,
                                MAX_ACCRUAL,
                                &routes,
                            ),
                            Expr::bytes(owner.clone()),
                            Expr::int(DEPOSIT),
                            token("TokenState", account_id.as_bytes().to_vec(), 2, DEPOSIT),
                        ],
                        true,
                    ),
                    0,
                ),
                input(
                    fixture(4, 0),
                    covenant_call(
                        &user_token,
                        "transferPolicy",
                        vec![
                            token_states(vec![(account_id.as_bytes().to_vec(), 2, DEPOSIT)]),
                            Expr::bytes(token_sig),
                            Expr::byte(0),
                        ],
                        true,
                    ),
                    0,
                ),
            ],
            open_outputs.clone(),
            0,
            Default::default(),
            0,
            vec![],
        )
    };
    let unsigned_open = build_open(dummy_sig.clone());
    let open_sig = sign(unsigned_open, open_entries.clone(), 1, &owner_key);
    let open = build_open(open_sig);
    for index in 0..2 {
        execute(&open, open_entries.clone(), index)
            .unwrap_or_else(|e| panic!("open Savings input {index}: {e}"));
    }

    // 5. Refresh: the account claims a signed duration, Reserve pays gross interest,
    // the account receives net interest and the referrer receives the exact fee.
    let account_refreshed = compile(
        &account_src,
        &account_args(
            owner.clone(),
            referral.clone(),
            DEPOSIT + NET_INTEREST,
            0,
            MAX_ACCRUAL - ACCRUAL,
            &routes,
            &kcc_route,
        ),
    );
    let controller_refreshed = compile(
        &controller_src,
        &controller_args(
            true,
            1,
            1,
            DEPOSIT + NET_INTEREST,
            &routes,
            &kcc_route,
            &account_route,
            &governor_route,
            &proposal_route,
        ),
    );
    let reserve_old = compile(
        &reserve_src,
        &reserve_args(
            RESERVE_BALANCE,
            &kcc_route,
            &account_route,
            &routes.controller,
            &registry_route,
        ),
    );
    let reserve_refreshed = compile(
        &reserve_src,
        &reserve_args(
            RESERVE_BALANCE - GROSS_INTEREST,
            &kcc_route,
            &account_route,
            &routes.controller,
            &registry_route,
        ),
    );
    let reserve_token = compile(
        &kcc_src,
        &kcc_args(RESERVE.as_bytes().to_vec(), RESERVE_BALANCE, 2),
    );
    let refreshed_account_token = compile(
        &kcc_src,
        &kcc_args(account_id.as_bytes().to_vec(), DEPOSIT + NET_INTEREST, 2),
    );
    let refreshed_reserve_token = compile(
        &kcc_src,
        &kcc_args(
            RESERVE.as_bytes().to_vec(),
            RESERVE_BALANCE - GROSS_INTEREST,
            2,
        ),
    );
    let refresh_entries_for = |reserve_id: Hash| {
        vec![
            entry(&open.outputs[0], CONTROLLER),
            entry(&open.outputs[1], account_id),
            UtxoEntry::new(
                CONTRACT_KAS,
                pay_to_script_hash_script(&reserve_old.bytecode),
                0,
                false,
                Some(reserve_id),
            ),
            entry(&open.outputs[2], ASSET),
            UtxoEntry::new(
                CONTRACT_KAS,
                pay_to_script_hash_script(&reserve_token.bytecode),
                0,
                false,
                Some(ASSET),
            ),
            UtxoEntry::new(
                CONTRACT_KAS,
                pay_to_script_hash_script(&registry.bytecode),
                0,
                false,
                Some(REGISTRY),
            ),
        ]
    };
    let build_refresh =
        |age: u64, bonus: i64, reserve_id: Hash, referral_owner: Vec<u8>, owner_sig: Vec<u8>| {
            let next_saved = DEPOSIT + NET_INTEREST + bonus;
            let next_controller = compile(
                &controller_src,
                &controller_args(
                    true,
                    1,
                    1,
                    next_saved,
                    &routes,
                    &kcc_route,
                    &account_route,
                    &governor_route,
                    &proposal_route,
                ),
            );
            let next_account = compile(
                &account_src,
                &account_args(
                    owner.clone(),
                    referral.clone(),
                    next_saved,
                    0,
                    MAX_ACCRUAL - ACCRUAL,
                    &routes,
                    &kcc_route,
                ),
            );
            let next_account_token = compile(
                &kcc_src,
                &kcc_args(account_id.as_bytes().to_vec(), next_saved, 2),
            );
            let next_referral_token =
                compile(&kcc_src, &kcc_args(referral_owner.clone(), REFERRAL_FEE, 0));
            let next_reserve_id = reserve_id;
            let account_expr = || {
                account_state(
                    "AccountState",
                    owner.clone(),
                    referral.clone(),
                    next_saved,
                    0,
                    MAX_ACCRUAL - ACCRUAL,
                    &routes,
                )
            };
            let account_token_expr =
                || token("TokenState", account_id.as_bytes().to_vec(), 2, next_saved);
            let reserve_token_expr = || {
                token(
                    "TokenState",
                    RESERVE.as_bytes().to_vec(),
                    2,
                    RESERVE_BALANCE - GROSS_INTEREST,
                )
            };
            let referral_expr = || token("TokenState", referral_owner.clone(), 0, REFERRAL_FEE);
            let outputs = vec![
                output(&next_controller, CONTRACT_KAS, 0, CONTROLLER),
                output(&next_account, ACCOUNT_KAS, 1, account_id),
                output(&reserve_refreshed, CONTRACT_KAS, 2, next_reserve_id),
                output(&next_account_token, CONTRACT_KAS, 3, ASSET),
                output(&refreshed_reserve_token, CONTRACT_KAS, 3, ASSET),
                output(&next_referral_token, CONTRACT_KAS, 3, ASSET),
                output(&registry, CONTRACT_KAS, 5, REGISTRY),
            ];
            Transaction::new(
                1,
                vec![
                    input(
                        outpoint(&open, 0),
                        covenant_call(
                            &controller_open,
                            "accountRefreshPolicy",
                            vec![
                                controller_state("State", true, 1, 1, next_saved, &routes),
                                Expr::byte(1),
                                account_expr(),
                            ],
                            true,
                        ),
                        0,
                    ),
                    input(
                        outpoint(&open, 1),
                        covenant_call(
                            &account_open,
                            "refreshPolicy",
                            vec![
                                account_state(
                                    "State",
                                    owner.clone(),
                                    referral.clone(),
                                    next_saved,
                                    0,
                                    MAX_ACCRUAL - ACCRUAL,
                                    &routes,
                                ),
                                Expr::bytes(owner_sig),
                                Expr::int(ACCRUAL),
                                Expr::byte(0),
                                Expr::byte(2),
                                Expr::byte(3),
                                Expr::byte(4),
                                account_token_expr(),
                                referral_expr(),
                            ],
                            true,
                        ),
                        age,
                    ),
                    input(
                        fixture(5, 0),
                        covenant_call(
                            &reserve_old,
                            "paySavingsPolicy",
                            vec![
                                reserve_state("State", RESERVE_BALANCE - GROSS_INTEREST),
                                Expr::byte(5),
                                Expr::byte(0),
                                Expr::byte(1),
                                account_expr(),
                                Expr::int(ACCRUAL),
                                Expr::byte(3),
                                Expr::byte(4),
                                reserve_token_expr(),
                            ],
                            true,
                        ),
                        0,
                    ),
                    input(
                        outpoint(&open, 2),
                        covenant_call(
                            &account_token,
                            "transferPolicy",
                            vec![
                                token_states(vec![
                                    (account_id.as_bytes().to_vec(), 2, next_saved),
                                    (
                                        RESERVE.as_bytes().to_vec(),
                                        2,
                                        RESERVE_BALANCE - GROSS_INTEREST,
                                    ),
                                    (referral_owner.clone(), 0, REFERRAL_FEE),
                                ]),
                                Expr::bytes(vec![0; 65]),
                                Expr::byte(0),
                            ],
                            true,
                        ),
                        0,
                    ),
                    input(
                        fixture(6, 0),
                        covenant_call(
                            &reserve_token,
                            "transferPolicy",
                            vec![Expr::bytes(vec![0; 65]), Expr::byte(0)],
                            false,
                        ),
                        0,
                    ),
                    input(
                        fixture(7, 0),
                        covenant_call(
                            &registry,
                            "preservePolicy",
                            vec![struct_object(
                                "State",
                                vec![
                                    ("bootstrapOwner", Expr::bytes(owner.clone())),
                                    ("controllerId", Expr::bytes(CONTROLLER.as_bytes().to_vec())),
                                    ("initialized", Expr::bool(true)),
                                ],
                            )],
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
            )
        };
    let signed_refresh = |age: u64, bonus: i64, reserve_id: Hash, referral_owner: Vec<u8>| {
        let entries = refresh_entries_for(reserve_id);
        let unsigned = build_refresh(age, bonus, reserve_id, referral_owner.clone(), vec![0; 65]);
        let signature = sign(unsigned, entries, 1, &owner_key);
        build_refresh(age, bonus, reserve_id, referral_owner, signature)
    };

    let valid_age = (INTEREST_DELAY + ACCRUAL) as u64;
    let insufficient_daa = signed_refresh(valid_age - 1, 0, RESERVE, referral.clone());
    assert!(execute(&insufficient_daa, refresh_entries_for(RESERVE), 1).is_err());

    let overpayment = signed_refresh(valid_age, 1, RESERVE, referral.clone());
    assert!(execute(&overpayment, refresh_entries_for(RESERVE), 1).is_err());
    assert!(execute(&overpayment, refresh_entries_for(RESERVE), 2).is_err());

    let wrong_reserve_tx = signed_refresh(valid_age, 0, WRONG_RESERVE, referral.clone());
    assert!(execute(&wrong_reserve_tx, refresh_entries_for(WRONG_RESERVE), 1).is_err());
    assert!(execute(&wrong_reserve_tx, refresh_entries_for(WRONG_RESERVE), 2).is_err());

    let wrong_referral_tx = signed_refresh(valid_age, 0, RESERVE, wrong_referral.clone());
    assert!(execute(&wrong_referral_tx, refresh_entries_for(RESERVE), 1).is_err());

    let refresh = signed_refresh(valid_age, 0, RESERVE, referral.clone());
    for index in 0..6 {
        execute(&refresh, refresh_entries_for(RESERVE), index)
            .unwrap_or_else(|e| panic!("refresh Savings input {index}: {e}"));
    }
    let mut wrong_registry_entries = refresh_entries_for(RESERVE);
    wrong_registry_entries[5] = UtxoEntry::new(
        CONTRACT_KAS,
        pay_to_script_hash_script(&registry.bytecode),
        0,
        false,
        Some(WRONG_RESERVE),
    );
    assert!(execute(&refresh, wrong_registry_entries, 2).is_err());

    let mut duplicated_registry = refresh.clone();
    duplicated_registry
        .outputs
        .push(output(&registry, CONTRACT_KAS, 5, REGISTRY));
    assert!(execute(&duplicated_registry, refresh_entries_for(RESERVE), 5).is_err());

    // 6. Withdraw: close without Reserve, pay the exact full balance, and
    // return account sompi to the owner.
    let controller_closed = compile(
        &controller_src,
        &controller_args(
            true,
            1,
            1,
            0,
            &routes,
            &kcc_route,
            &account_route,
            &governor_route,
            &proposal_route,
        ),
    );
    let owner_payout = compile(
        &kcc_src,
        &kcc_args(owner.clone(), DEPOSIT + NET_INTEREST, 0),
    );
    let owner_spk = pay_to_address_script(&Address::new(Prefix::Testnet, Version::PubKey, &owner));
    let withdraw_entries = vec![
        entry(&refresh.outputs[0], CONTROLLER),
        entry(&refresh.outputs[1], account_id),
        entry(&refresh.outputs[3], ASSET),
    ];
    let build_withdraw = |amount: i64, account_sig: Vec<u8>| {
        let payout = compile(&kcc_src, &kcc_args(owner.clone(), amount, 0));
        Transaction::new(
            1,
            vec![
                input(
                    outpoint(&refresh, 0),
                    covenant_call(
                        &controller_refreshed,
                        "accountClosePolicy",
                        vec![
                            controller_state("State", true, 1, 1, 0, &routes),
                            Expr::byte(1),
                        ],
                        true,
                    ),
                    0,
                ),
                input(
                    outpoint(&refresh, 1),
                    covenant_call(
                        &account_refreshed,
                        "closePolicy",
                        vec![
                            Expr::array(parse_type_ref("State[]").unwrap(), vec![]),
                            Expr::bytes(account_sig),
                            Expr::byte(0),
                            controller_state("ControllerState", true, 1, 1, 0, &routes),
                            Expr::byte(2),
                            token("TokenState", owner.clone(), 0, amount),
                            Expr::byte(2),
                        ],
                        true,
                    ),
                    0,
                ),
                input(
                    outpoint(&refresh, 3),
                    covenant_call(
                        &refreshed_account_token,
                        "transferPolicy",
                        vec![
                            token_states(vec![(owner.clone(), 0, amount)]),
                            Expr::bytes(vec![0; 65]),
                            Expr::byte(0),
                        ],
                        true,
                    ),
                    0,
                ),
            ],
            vec![
                output(&controller_closed, CONTRACT_KAS, 0, CONTROLLER),
                output(&payout, CONTRACT_KAS, 2, ASSET),
                TransactionOutput {
                    value: ACCOUNT_KAS,
                    script_public_key: owner_spk.clone(),
                    covenant: None,
                },
            ],
            0,
            Default::default(),
            0,
            vec![],
        )
    };
    let unsigned_withdraw = build_withdraw(DEPOSIT + NET_INTEREST, vec![0; 65]);
    let withdraw_sig = sign(unsigned_withdraw, withdraw_entries.clone(), 1, &owner_key);
    let withdraw = build_withdraw(DEPOSIT + NET_INTEREST, withdraw_sig);
    for index in 0..3 {
        execute(&withdraw, withdraw_entries.clone(), index)
            .unwrap_or_else(|e| panic!("withdraw Savings input {index}: {e}"));
    }

    let unsigned_theft = build_withdraw(DEPOSIT + NET_INTEREST + 1, vec![0; 65]);
    let theft_sig = sign(unsigned_theft, withdraw_entries.clone(), 1, &owner_key);
    let theft = build_withdraw(DEPOSIT + NET_INTEREST + 1, theft_sig);
    assert!(execute(&theft, withdraw_entries.clone(), 1).is_err());

    let unsigned_wrong_signer = build_withdraw(DEPOSIT + NET_INTEREST, vec![0; 65]);
    let wrong_sig = sign(
        unsigned_wrong_signer,
        withdraw_entries.clone(),
        1,
        &wrong_key,
    );
    let wrong_signer = build_withdraw(DEPOSIT + NET_INTEREST, wrong_sig);
    assert!(execute(&wrong_signer, withdraw_entries, 1).is_err());

    assert_eq!(
        withdraw.outputs[1].script_public_key,
        pay_to_script_hash_script(&owner_payout.bytecode),
    );
}

#[test]
fn savings_registry_initializes_once_and_preserves_exact_successor() {
    let secp = Secp256k1::new();
    let owner_key = Keypair::from_secret_key(&secp, &SecretKey::from_slice(&[31; 32]).unwrap());
    let wrong_key = Keypair::from_secret_key(&secp, &SecretKey::from_slice(&[32; 32]).unwrap());
    let owner = owner_key.x_only_public_key().0.serialize().to_vec();
    let source = source("contracts/savings-registry.sil");
    let previous = compile(
        &source,
        &[
            Expr::bytes(owner.clone()),
            Expr::bytes(vec![0; 32]),
            Expr::bool(false),
        ],
    );
    let next = compile(
        &source,
        &[
            Expr::bytes(owner.clone()),
            Expr::bytes(CONTROLLER.as_bytes().to_vec()),
            Expr::bool(true),
        ],
    );
    assert_eq!(previous.template_hash(), next.template_hash());
    let previous_output = output(&previous, CONTRACT_KAS, 0, REGISTRY);
    let entries = vec![entry(&previous_output, REGISTRY)];
    let next_state = || {
        struct_object(
            "State",
            vec![
                ("bootstrapOwner", Expr::bytes(owner.clone())),
                ("controllerId", Expr::bytes(CONTROLLER.as_bytes().to_vec())),
                ("initialized", Expr::bool(true)),
            ],
        )
    };
    let build = |value: u64, sig: Vec<u8>| {
        Transaction::new(
            0,
            vec![input(
                fixture(90, 0),
                covenant_call(
                    &previous,
                    "initializePolicy",
                    vec![next_state(), Expr::bytes(sig)],
                    true,
                ),
                0,
            )],
            vec![output(&next, value, 0, REGISTRY)],
            0,
            Default::default(),
            0,
            vec![],
        )
    };
    let unsigned = build(CONTRACT_KAS, vec![0; 65]);
    let signature = sign(unsigned, entries.clone(), 0, &owner_key);
    let initialized = build(CONTRACT_KAS, signature);
    execute(&initialized, entries.clone(), 0).expect("signed Registry initialization");

    let wrong_signature = sign(
        build(CONTRACT_KAS, vec![0; 65]),
        entries.clone(),
        0,
        &wrong_key,
    );
    assert!(execute(&build(CONTRACT_KAS, wrong_signature), entries.clone(), 0).is_err());
    let valid_signature = sign(
        build(CONTRACT_KAS - 1, vec![0; 65]),
        entries.clone(),
        0,
        &owner_key,
    );
    assert!(execute(&build(CONTRACT_KAS - 1, valid_signature), entries, 0).is_err());

    let initialized_entry = vec![entry(&initialized.outputs[0], REGISTRY)];
    let preserve_state = next_state();
    let preserve = Transaction::new(
        0,
        vec![input(
            outpoint(&initialized, 0),
            covenant_call(&next, "preservePolicy", vec![preserve_state], true),
            0,
        )],
        vec![output(&next, CONTRACT_KAS, 0, REGISTRY)],
        0,
        Default::default(),
        0,
        vec![],
    );
    execute(&preserve, initialized_entry.clone(), 0).expect("Registry preserved");

    let second_init = Transaction::new(
        0,
        vec![input(
            outpoint(&initialized, 0),
            covenant_call(
                &next,
                "initializePolicy",
                vec![next_state(), Expr::bytes(vec![0; 65])],
                true,
            ),
            0,
        )],
        vec![output(&next, CONTRACT_KAS, 0, REGISTRY)],
        0,
        Default::default(),
        0,
        vec![],
    );
    assert!(execute(&second_init, initialized_entry, 0).is_err());
}
