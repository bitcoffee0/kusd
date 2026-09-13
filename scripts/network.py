"""Shared network, transaction, mass, fee, and RPC helpers for KUSD."""

import asyncio
import math
import os
from pathlib import Path

from kaspa import (
    CovenantBinding, Hash, PrivateKey, ScriptBuilder, Transaction,
    TransactionInput, TransactionOutpoint, UtxoEntryReference,
    address_from_script_public_key, calculate_transaction_mass,
)

NETWORK_ID = "testnet-10"
NETWORK_TYPE = "testnet"
SUBNETWORK_ID = bytes(20)
VERSION = 1
CB = 500
DEPOSIT = 100_000_000
def load_env():
    for line in Path(".env").read_text().splitlines():
        if line and not line.lstrip().startswith("#") and "=" in line:
            key, value = line.split("=", 1)
            os.environ.setdefault(key.strip(), value.strip())
    if os.environ.get("KASPA_NETWORK") != NETWORK_ID:
        raise SystemExit("Only testnet-10 is supported by this deployment branch")
    return os.environ["KASPA_RPC_URL"], PrivateKey(os.environ["KASPA_PRIVATE_KEY"])


def spk(artifact):
    return ScriptBuilder.from_script(bytes.fromhex(artifact["bytecode"]), covenants_enabled=True).create_pay_to_script_hash_script()


def inp(entry, script=b"", budget=CB, sequence=0):
    return TransactionInput(
        TransactionOutpoint(Hash(entry["outpoint"]["transactionId"]), entry["outpoint"]["index"]),
        script, sequence=sequence, sig_op_count=0, compute_budget=budget,
        utxo=UtxoEntryReference.from_dict(entry),
    )


def entry(txid, index, value, script, covenant_id=None):
    address = address_from_script_public_key(script, NETWORK_TYPE).to_string()
    return {"address": address, "outpoint": {"transactionId": txid, "index": index}, "utxoEntry": {
        "amount": value, "scriptPublicKey": {"version": script.version, "script": script.script},
        "blockDaaScore": 0, "isCoinbase": False, "covenantId": covenant_id,
    }}


def binding(authorizer, covenant_id):
    return CovenantBinding(authorizing_input=authorizer, covenant_id=Hash(covenant_id))


async def fee_rate(client):
    estimate = await client.get_fee_estimate()
    return math.ceil(estimate["estimate"]["priorityBucket"]["feerate"])


def make_tx(inputs, outputs, mass=0, lock_time=0):
    """Build a transaction, optionally committing to a DAA score as lock time."""
    return Transaction(VERSION, inputs, outputs, lock_time, SUBNETWORK_ID, 0, b"", mass)


def estimated_serialized_size(tx):
    """Mirror Kaspa consensus transaction_estimated_serialized_size."""
    value = tx.to_dict()
    size = 2 + 8 + 8 + 8 + 20 + 8 + 32 + 8
    for transaction_input in value["inputs"]:
        signature = transaction_input.get("signatureScript") or ""
        size += 32 + 4 + 8 + len(signature) // 2 + 8
        if tx.version >= 1:
            size += 2
    for output in value["outputs"]:
        script = output["scriptPublicKey"]["script"]
        size += 8 + 2 + 8 + len(script) // 2
        if output.get("covenant") is not None:
            size += 2 + 32
    payload = value.get("payload") or ""
    size += len(payload) // 2
    return size


def fee_mass_components(tx):
    """Return the mass components used by post-Toccata relay policy."""
    storage = tx.storage_mass
    value = tx.to_dict()
    # kaspa-python 2.0.2rc1 omits computeBudget from to_dict; the native
    # TransactionInput property is authoritative here.
    covenant_compute = 100 * sum(
        transaction_input.compute_budget for transaction_input in tx.inputs
    )
    serialized = estimated_serialized_size(tx)
    script_public_key_mass = 10 * sum(
        2 + len(output["scriptPublicKey"]["script"]) // 2
        for output in value["outputs"]
    )
    compute = serialized + script_public_key_mass + covenant_compute
    normalized_transient = 2 * serialized
    return {
        "storage_mass": storage,
        "covenant_compute_budget_mass": covenant_compute,
        "script_public_key_mass": script_public_key_mass,
        "compute_mass": compute,
        "serialized_size": serialized,
        "normalized_transient_mass": normalized_transient,
        "fee_mass_budgeted": max(compute, normalized_transient),
    }


async def sized(client, build, total_in, fixed_out):
    """Solve the post-Toccata change/storage-mass/fee fixed point."""
    rate, fee = await fee_rate(client), 0
    for _ in range(10):
        change = total_in - fixed_out - fee
        if change <= 0:
            raise RuntimeError("insufficient funds")
        draft = build(change, 0)
        mass = calculate_transaction_mass(NETWORK_ID, draft)
        # Relay also charges two grams per estimated byte. This term dominates
        # transactions with long SilverScript unlocking scripts.
        draft.storage_mass = mass
        fee2 = fee_mass_components(draft)["fee_mass_budgeted"] * rate
        if fee2 == fee:
            break
        fee = fee2
    final = build(total_in - fixed_out - fee, mass)
    # SDK setters do not automatically refresh the cached transaction ID.
    final.finalize()
    return final, fee


async def wait_utxo(client, address, txid, index=None, timeout=180):
    for _ in range(timeout):
        result = await client.get_utxos_by_addresses({"addresses": [address]})
        for candidate in result["entries"]:
            if candidate["outpoint"]["transactionId"] == txid and (
                index is None or candidate["outpoint"]["index"] == index
            ):
                return candidate
        await asyncio.sleep(1)
    raise TimeoutError(f"confirmation not observed: {txid}")


async def wallet_utxos(client, address):
    result = await client.get_utxos_by_addresses({"addresses": [address]})
    return sorted(result["entries"], key=lambda x: x["utxoEntry"]["amount"], reverse=True)
