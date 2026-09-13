use kaspa_consensus_core::Hash;
use kaspa_consensus_core::hashing::sighash::{
    SigHashReusedValuesUnsync, calc_schnorr_signature_hash,
};
use kaspa_consensus_core::hashing::sighash_type::SIG_HASH_ALL;
use kaspa_consensus_core::tx::{
    CovenantBinding, MutableTransaction, PopulatedTransaction, Transaction, TransactionId,
    TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry, VerifiableTransaction,
};
use kaspa_kusd::silverscript::{
    CompileOptions, CompiledContract, CovenantDeclCallOptions, compile_contract, struct_object,
};
use kaspa_txscript::caches::Cache;
use kaspa_txscript::covenants::CovenantsContext;
use kaspa_txscript::script_builder::ScriptBuilder;
use kaspa_txscript::{EngineCtx, EngineFlags, TxScriptEngine, pay_to_script_hash_script};
use secp256k1::Keypair;
use silverscript_lang::ast::{Expr, parse_type_ref};

// Test policy value. Deployment must calibrate it from the
// observed DAA rate because a DAA unit is not a second.
const HOLDING_DAA: i64 = 7_776_000;
const KPS_ID: Hash = Hash::from_bytes([0x31; 32]);
const RESERVE_ID: Hash = Hash::from_bytes([0x41; 32]);

fn compile_kps<'a>(
    source: &'a str,
    owner: Vec<u8>,
    amount: i64,
    minter: bool,
) -> CompiledContract<'a> {
    compile_contract(
        source,
        &[
            Expr::bytes(owner),
            Expr::int(amount),
            Expr::byte(if minter { 2 } else { 0 }),
            Expr::bool(minter),
            Expr::int(8),
            Expr::int(8),
            Expr::int(HOLDING_DAA),
            Expr::dynamic_bytes(vec![]),
            Expr::dynamic_bytes(vec![]),
            Expr::bytes(vec![0; 32]),
        ],
        CompileOptions::default(),
    )
    .unwrap()
}

fn call(contract: &CompiledContract<'_>, args: Vec<Expr<'_>>, leader: bool) -> Vec<u8> {
    let flags = EngineFlags {
        covenants_enabled: true,
        sigop_script_units: 0.into(),
    };
    let mut script = contract
        .build_sig_script_for_covenant_decl(
            "transferPolicy",
            args,
            CovenantDeclCallOptions { is_leader: leader },
        )
        .unwrap();
    script.extend_from_slice(
        &ScriptBuilder::with_flags(flags)
            .add_data(&contract.bytecode)
            .unwrap()
            .drain(),
    );
    script
}

fn minter_outputs() -> Expr<'static> {
    Expr::array(
        parse_type_ref("State[]").unwrap(),
        vec![struct_object(
            "State",
            vec![
                (
                    "ownerIdentifier",
                    Expr::bytes(RESERVE_ID.as_bytes().to_vec()),
                ),
                ("identifierType", Expr::byte(2)),
                ("amount", Expr::int(0)),
                ("isMinter", Expr::bool(true)),
            ],
        )],
    )
}

fn input(tag: u8, script: Vec<u8>, sequence: u64) -> TransactionInput {
    TransactionInput::new_with_compute_budget(
        TransactionOutpoint {
            transaction_id: TransactionId::from_bytes([tag; 32]),
            index: 0,
        },
        script,
        sequence,
        65_535,
    )
}

fn execute(tx: Transaction, entries: Vec<UtxoEntry>, idx: usize) -> Result<(), String> {
    let reused = SigHashReusedValuesUnsync::new();
    let cache = Cache::new(10_000);
    let populated = PopulatedTransaction::new(&tx, entries);
    let covenants = CovenantsContext::from_tx(&populated).map_err(|error| error.to_string())?;
    let mut vm = TxScriptEngine::from_transaction_input(
        &populated,
        &tx.inputs[idx],
        idx,
        populated.utxo(idx).unwrap(),
        EngineCtx::new(&cache)
            .with_reused(&reused)
            .with_covenants_ctx(&covenants),
        EngineFlags {
            covenants_enabled: true,
            sigop_script_units: 0.into(),
        },
    );
    vm.execute().map_err(|error| format!("{error:?}"))
}

fn build(sequence: u64) -> (Transaction, Vec<UtxoEntry>) {
    let source = std::fs::read_to_string("contracts/kps.sil").unwrap();
    let owner = Keypair::new(&secp256k1::SECP256K1, &mut secp256k1::rand::thread_rng());
    let owner_pk = owner.x_only_public_key().0.serialize().to_vec();
    let minter = compile_kps(&source, RESERVE_ID.as_bytes().to_vec(), 0, true);
    let shares = compile_kps(&source, owner_pk, 1_000_000_000, false);
    let output = TransactionOutput {
        value: 1_000,
        script_public_key: pay_to_script_hash_script(&minter.bytecode),
        covenant: Some(CovenantBinding {
            authorizing_input: 0,
            covenant_id: KPS_ID,
        }),
    };
    let entries = vec![
        UtxoEntry::new(
            1_000,
            pay_to_script_hash_script(&minter.bytecode),
            0,
            false,
            Some(KPS_ID),
        ),
        UtxoEntry::new(
            1_000,
            pay_to_script_hash_script(&shares.bytecode),
            0,
            false,
            Some(KPS_ID),
        ),
        UtxoEntry::new(
            1_000,
            pay_to_script_hash_script(&[0x51]),
            0,
            false,
            Some(RESERVE_ID),
        ),
    ];
    let unsigned = Transaction::new(
        1,
        vec![
            input(1, vec![], 0),
            input(2, vec![], sequence),
            input(3, vec![], 0),
        ],
        vec![output.clone()],
        0,
        Default::default(),
        0,
        vec![],
    );
    let mutable = MutableTransaction::with_entries(unsigned, entries.clone());
    let reused = SigHashReusedValuesUnsync::new();
    let hash = calc_schnorr_signature_hash(&mutable.as_verifiable(), 1, SIG_HASH_ALL, &reused);
    let message = secp256k1::Message::from_digest_slice(hash.as_bytes().as_slice()).unwrap();
    let mut signature = owner.sign_schnorr(message).as_ref().to_vec();
    signature.push(SIG_HASH_ALL.to_u8());
    let tx = Transaction::new(
        1,
        vec![
            input(
                1,
                call(
                    &minter,
                    vec![minter_outputs(), Expr::bytes(vec![0; 65]), Expr::byte(2)],
                    true,
                ),
                0,
            ),
            input(
                2,
                call(&shares, vec![Expr::bytes(signature), Expr::byte(2)], false),
                sequence,
            ),
            input(3, vec![], 0),
        ],
        vec![output],
        0,
        Default::default(),
        0,
        vec![],
    );
    (tx, entries)
}

#[test]
fn delegated_kps_redemption_requires_the_full_holding_period() {
    let (early, entries) = build((HOLDING_DAA - 1) as u64);
    assert!(execute(early, entries, 1).is_err());

    let (mature, entries) = build(HOLDING_DAA as u64);
    execute(mature, entries, 1).expect("mature KPS share must authorize Reserve redemption");
}
