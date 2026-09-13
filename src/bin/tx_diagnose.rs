use kaspa_consensus_core::Hash;
use kaspa_consensus_core::hashing::sighash::SigHashReusedValuesUnsync;
use kaspa_consensus_core::tx::{
    CovenantBinding, PopulatedTransaction, ScriptPublicKey, Transaction, TransactionInput,
    TransactionOutpoint, TransactionOutput, UtxoEntry, VerifiableTransaction,
};
use kaspa_txscript::caches::Cache;
use kaspa_txscript::covenants::CovenantsContext;
use kaspa_txscript::{EngineCtx, EngineFlags, TxScriptEngine};
use serde_json::Value;
use std::str::FromStr;

fn bytes(value: &Value) -> Vec<u8> {
    hex::decode(value.as_str().unwrap_or_default()).expect("invalid hex")
}

fn hash(value: &Value) -> Hash {
    Hash::from_str(value.as_str().expect("missing hash")).expect("invalid hash")
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let summary_only = args.iter().any(|arg| arg == "--summary");
    let compact = args.iter().any(|arg| arg == "--compact");
    let budget = args
        .iter()
        .position(|arg| arg == "--budget")
        .and_then(|index| args.get(index + 1))
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(500);
    let budgets = args
        .iter()
        .position(|arg| arg == "--budgets")
        .and_then(|index| args.get(index + 1))
        .map(|values| {
            values
                .split(',')
                .map(|value| value.parse::<u16>())
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?;
    let path = args
        .iter()
        .enumerate()
        .find(|(index, arg)| {
            arg.as_str() != "--summary"
                && arg.as_str() != "--compact"
                && arg.as_str() != "--budget"
                && arg.as_str() != "--budgets"
                && args.get(index.saturating_sub(1)).map(|v| v.as_str()) != Some("--budget")
                && args.get(index.saturating_sub(1)).map(|v| v.as_str()) != Some("--budgets")
        })
        .map(|(_, value)| value)
        .cloned()
        .unwrap_or_else(|| "kusd-transaction-probe.local.json".into());
    let value: Value = serde_json::from_slice(&std::fs::read(path)?)?;
    let raw = &value["tx"];
    let mut entries = vec![];
    let mut inputs = vec![];
    for (index, input) in raw["inputs"].as_array().expect("inputs").iter().enumerate() {
        let outpoint = &input["previousOutpoint"];
        inputs.push(TransactionInput::new_with_compute_budget(
            TransactionOutpoint::new(
                hash(&outpoint["transactionId"]),
                outpoint["index"].as_u64().unwrap() as u32,
            ),
            bytes(&input["signatureScript"]),
            input["sequence"].as_u64().unwrap(),
            budgets
                .as_ref()
                .and_then(|values| values.get(index))
                .copied()
                .unwrap_or(budget),
        ));
        let utxo = &input["utxo"];
        let spk = &utxo["scriptPublicKey"];
        let covenant_id = utxo["covenantId"]
            .as_str()
            .map(|id| Hash::from_str(id).unwrap());
        entries.push(UtxoEntry::new(
            utxo["amount"].as_u64().unwrap(),
            ScriptPublicKey::from_vec(
                spk["version"].as_u64().unwrap() as u16,
                bytes(&spk["script"]),
            ),
            utxo["blockDaaScore"].as_u64().unwrap(),
            utxo["isCoinbase"].as_bool().unwrap(),
            covenant_id,
        ));
    }
    let mut outputs = vec![];
    for output in raw["outputs"].as_array().expect("outputs") {
        let spk = &output["scriptPublicKey"];
        let covenant = output
            .get("covenant")
            .filter(|v| !v.is_null())
            .map(|value| CovenantBinding {
                authorizing_input: value["authorizingInput"].as_u64().unwrap() as u16,
                covenant_id: hash(&value["covenantId"]),
            });
        outputs.push(TransactionOutput::with_covenant(
            output["value"].as_u64().unwrap(),
            ScriptPublicKey::from_vec(
                spk["version"].as_u64().unwrap() as u16,
                bytes(&spk["script"]),
            ),
            covenant,
        ));
    }
    let tx = Transaction::new_with_mass(
        raw["version"].as_u64().unwrap() as u16,
        inputs,
        outputs,
        raw["lockTime"].as_u64().unwrap(),
        Default::default(),
        raw["gas"].as_u64().unwrap(),
        bytes(&raw["payload"]),
        raw["mass"].as_u64().unwrap(),
    );
    let populated = PopulatedTransaction::new(&tx, entries);
    let covenants = CovenantsContext::from_tx(&populated)?;
    let cache = Cache::new(10_000);
    let reused = SigHashReusedValuesUnsync::new();
    for index in 0..tx.inputs.len() {
        let mut opcode_log = Vec::new();
        let mut engine = TxScriptEngine::from_transaction_input_with_script_units_limit(
            &populated,
            &tx.inputs[index],
            index,
            populated.utxo(index).unwrap(),
            EngineCtx::new(&cache)
                .with_reused(&reused)
                .with_covenants_ctx(&covenants),
            EngineFlags {
                covenants_enabled: true,
                ..Default::default()
            },
            tx.inputs[index].compute_commit.allowed_script_units(),
        )
        .with_opcode_execution_log_buffer(&mut opcode_log);
        match engine.execute() {
            Ok(()) => println!("input {index}: OK"),
            Err(error) => {
                println!("input {index}: {error:?}");
                if summary_only {
                    continue;
                }
                let trace = String::from_utf8_lossy(&opcode_log);
                let lines = trace.lines().collect::<Vec<_>>();
                for line in lines.iter().skip(lines.len().saturating_sub(12)) {
                    if compact {
                        let prefix = line.chars().take(120).collect::<String>();
                        let suffix = line
                            .chars()
                            .rev()
                            .take(320)
                            .collect::<String>()
                            .chars()
                            .rev()
                            .collect::<String>();
                        println!("  {prefix} … {suffix}");
                    } else {
                        println!("  {line}");
                    }
                }
            }
        }
    }
    Ok(())
}
