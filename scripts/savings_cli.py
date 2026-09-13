"""Validate the KUSD Savings lifecycle on the configured Kaspa network."""
import argparse
import asyncio
import copy
import json
import os
from pathlib import Path

os.environ["KUSD_FULL"] = "1"

from kaspa import (
    GenesisCovenantGroup, RpcClient, Transaction, TransactionOutput,
    create_input_signature, pay_to_address_script,
)

import deployment_core as genesis
from network import (
    CB, DEPOSIT, NETWORK_ID, NETWORK_TYPE, SUBNETWORK_ID, binding, entry, inp, load_env,
    make_tx, sized, spk, wait_utxo,
)

TAG = os.environ.get("KUSD_TAG", "kusd")
PROTOCOL = "KUSD"
DEPLOYMENT = Path(f"{TAG}-deployment.local.json")
REMAINING = Path(f"{TAG}-remaining-validation.local.json")
STATE = Path(f"{TAG}-live-validation.local.json")
PROGRESS = Path(f"{TAG}-live-progress.local.json")
ERROR_LOG = Path(f"{TAG}-live-last-error.local.txt")
PROBE_PREFIX = f"{TAG}-live-probe"
SAVED = int(os.environ.get("KUSD_SAVINGS_AMOUNT", "220000000"))
ACCRUAL_DAA = 1_000
RATE_PPM = 20_000
REFERRAL_PPM = 100_000
DAA_PER_YEAR = 31_536_000
DELAY_DAA = 50
MAX_ACCRUAL_DAA = 10_000


def configure_ids(deployment):
    genesis.SAVINGS_REGISTRY_ID = deployment["savings_registry_id"]
    genesis.SAVINGS_CONTROLLER_ID = deployment["savings_controller_id"]
    genesis.SAVINGS_GOVERNOR_ID = deployment["savings_governor_id"]


def reserve_after_proposal(deployment):
    return int(deployment.get("economics", {}).get(
        "reserve_kusd_after_proposal_fee",
        genesis.RESERVE_DEPOSIT,
    ))


def owner_kusd_change(deployment):
    """Amount encoded in the single `kusd_change` UTXO.

    Deployments created before this field was made explicit recorded the
    combined owner balance, including the separate proposal-fee refund.
    """
    economics = deployment.get("economics", {})
    if "owner_kusd_change" in economics:
        return int(economics["owner_kusd_change"])
    combined = int(economics.get("owner_kusd_after_proposal_fee", SAVED))
    refund = int(economics.get("proposal_fee_kusd", 0))
    return combined - refund


def base_config(deployment, *, reserve_kusd=None, total_saved=0,
                account_nonce=0, saved=None, delay=DELAY_DAA,
                remaining=MAX_ACCRUAL_DAA):
    if reserve_kusd is None:
        reserve_kusd = reserve_after_proposal(deployment)
    if saved is None:
        saved = SAVED
    configure_ids(deployment)
    owner = deployment["owner"]
    values = {
        **genesis.economic(
            owner, deployment["asset_id"], deployment["kps_id"],
            deployment["reserve_id"], module=deployment["governed_module_id"],
            remaining=genesis.GOVERNED_CAP,
            expiration=deployment["expiration_daa"],
        ),
        **genesis.governance(
            owner, deployment["governance_id"], deployment["root_id"],
            proposal_nonce=1, execution_nonce=1,
            remaining=genesis.ROOT_CAP-genesis.BOOTSTRAP_CAP-genesis.GOVERNED_CAP,
        ),
        "state_reserve_kusd": reserve_kusd,
        "state_total_kps": genesis.RESERVE_DEPOSIT,
    }
    cfg = genesis._config(values)
    cfg["savings"].update({
        "enabled": True, "series_nonce": 1, "account_nonce": account_nonce,
        "total_saved": total_saved, "saved": saved,
        "current_rate_ppm": RATE_PPM, "rate_ppm": RATE_PPM,
        "delay_remaining_daa": delay,
        "remaining_accrual_daa": remaining,
        "interest_delay_daa": DELAY_DAA,
        "max_accrual_daa": MAX_ACCRUAL_DAA,
        "daa_per_year": DAA_PER_YEAR,
        "referral_fee_ppm": REFERRAL_PPM,
    })
    return cfg


def art(command, cfg=None, **values):
    if cfg is not None:
        return genesis.config_tool(command, cfg, **values)
    return genesis.artifact(command, **values)


def call(command, cfg, **values):
    return bytes.fromhex(genesis.config_tool(command, cfg, **values))


def token(owner, amount, identifier):
    return art("kcc-state", owner=owner, amount=amount, identifier=identifier, minter=False)


def token_call(owner, amount, identifier, outputs, signature="00" * 65,
               leader=True, current_minter=False):
    return bytes.fromhex(genesis.tool(
        "kcc-transfer-call", current_owner=owner, current_amount=amount,
        current_identifier=identifier, current_minter=current_minter,
        outputs_json=json.dumps(outputs), signature=signature, witness=0,
        leader=leader,
    ))


def out(tx, index, covenant_id=None):
    output = tx.outputs[index]
    return entry(str(tx.id), index, output.value, output.script_public_key, covenant_id)


def assert_script(name, actual_entry, artifact):
    expected = spk(artifact)
    actual = actual_entry["utxoEntry"]["scriptPublicKey"]
    if isinstance(actual, str):
        actual_version, actual_script = int(actual[:4], 16), actual[4:]
    else:
        actual_version, actual_script = actual["version"], actual["script"]
    if actual_version != expected.version or actual_script != expected.script:
        raise RuntimeError(f"{name}: local state differs from the network UTXO")


def write_probe(name, tx):
    path = Path(f"{PROBE_PREFIX}-{name}.local.json")
    path.write_text(json.dumps({"tx": tx.to_dict()}, indent=2, default=str))
    diagnose = Path("target/debug/tx-diagnose")
    result = __import__("subprocess").run(
        [str(diagnose), "--summary", str(path)], capture_output=True,
        text=True, check=True,
    )
    statuses = [line for line in result.stdout.splitlines() if line.startswith("input ")]
    failures = [line for line in statuses if not line.endswith(": OK")]
    if not statuses or failures:
        raise RuntimeError(f"{name}: consensus VM: {failures or ['no input']}")
    return len(statuses)


def probe_transaction(name):
    """Reload the exact transaction already validated by the local VM."""
    raw = json.loads(Path(f"{PROBE_PREFIX}-{name}.local.json").read_text())["tx"]
    restored = Transaction.from_dict(raw)
    for transaction_input in restored.inputs:
        transaction_input.compute_budget = CB
    fresh = Transaction(
        restored.version, restored.inputs, restored.outputs, restored.lock_time,
        SUBNETWORK_ID, restored.gas, bytes.fromhex(restored.payload),
        restored.storage_mass,
    )
    fresh.finalize()
    return fresh


def save_progress(plan, completed, stage, account_daa=None):
    PROGRESS.write_text(json.dumps({
        "network": NETWORK_ID, "protocol": PROTOCOL, "stage": stage,
        "completed": completed, "account_daa": account_daa, "plan": plan,
    }, indent=2))


async def submit_named(client, plan, name):
    tx = probe_transaction(name)
    expected = next(item["id"] for item in plan["transactions"] if item["name"] == name)
    if str(tx.id) != expected:
        raise RuntimeError(f"{name}: the persisted probe no longer matches the plan")
    await submit(client, name, tx)
    return tx


async def submit(client, name, tx):
    response = await client.submit_transaction({"transaction": tx, "allowOrphan": False})
    txid = response["transactionId"]
    if txid != str(tx.id):
        raise RuntimeError(f"{name}: unexpected RPC transaction ID")
    await wait_utxo(
        client,
        genesis.address_from_script_public_key(tx.outputs[-1].script_public_key, NETWORK_TYPE).to_string(),
        txid, len(tx.outputs)-1,
    )
    print(json.dumps({"confirmed": name, "transaction_id": txid}, indent=2))


async def build_savings(client, key, deployment):
    owner = deployment["owner"]
    owner_spk = pay_to_address_script(key.to_public_key().to_address(NETWORK_TYPE))
    live = copy.deepcopy(deployment["live"])
    controller = live["savings_controller"]
    registry = live["savings_registry"]
    reserve = live["reserve"]
    reserve_token = live["reserve_kusd"]
    user_token = live["kusd_change"]
    wallet = live["wallet"]

    reserve_start = reserve_after_proposal(deployment)
    current = base_config(deployment, reserve_kusd=reserve_start)
    controller0 = art("controller-state", current)
    reserve0 = art("reserve-state", current)
    registry0 = art("registry-state", current, initialized=True)
    user0 = token(owner, SAVED, 0)
    reserve_token0 = token(deployment["reserve_id"], reserve_start, 2)
    for name, utxo, artifact in [
        ("Controller", controller, controller0), ("Reserve", reserve, reserve0),
        ("Registry", registry, registry0), ("User KUSD", user_token, user0),
        ("KUSD Reserve", reserve_token, reserve_token0),
    ]:
        assert_script(name, utxo, artifact)

    opened = copy.deepcopy(current)
    opened["savings"].update({"account_nonce": 1, "total_saved": SAVED})
    account_cfg = copy.deepcopy(current)
    account0 = art("account-state", account_cfg)
    controller_opened = art("controller-state", opened)
    probe = make_tx(
        [inp(controller), inp(user_token), inp(wallet)],
        [TransactionOutput(DEPOSIT, spk(controller_opened)),
         TransactionOutput(DEPOSIT, spk(account0))],
    )
    probe.populate_genesis_covenants([GenesisCovenantGroup(0, [1])])
    account_id = probe.outputs[1].to_dict()["covenant"]["covenantId"]
    account_token0 = token(account_id, SAVED, 2)
    open_controller = call(
        "open-call", current, next_config=opened, account_config=account_cfg,
        account_id=account_id, account_output=1,
    )

    def make_open(change, mass):
        outputs = [
            TransactionOutput(DEPOSIT, spk(controller_opened), binding(0, deployment["savings_controller_id"])),
            TransactionOutput(DEPOSIT, spk(account0)),
            TransactionOutput(DEPOSIT, spk(account_token0), binding(1, deployment["asset_id"])),
            TransactionOutput(change, owner_spk),
        ]
        unsigned = make_tx([inp(controller), inp(user_token), inp(wallet)], outputs, mass)
        unsigned.populate_genesis_covenants([GenesisCovenantGroup(0, [1])])
        sig = create_input_signature(unsigned, 1, key)[2:]
        kcc = token_call(owner, SAVED, 0, [(account_id, 2, SAVED, False)], sig)
        tx = make_tx([inp(controller, open_controller), inp(user_token, kcc), inp(wallet)], outputs, mass)
        tx.populate_genesis_covenants([GenesisCovenantGroup(0, [1])])
        tx.inputs[2].signature_script = bytes.fromhex(create_input_signature(tx, 2, key))
        return tx

    open_tx, open_fee = await sized(
        client, make_open,
        controller["utxoEntry"]["amount"] + user_token["utxoEntry"]["amount"] + wallet["utxoEntry"]["amount"],
        3 * DEPOSIT,
    )
    controller = out(open_tx, 0, deployment["savings_controller_id"])
    account = out(open_tx, 1, account_id)
    account_token = out(open_tx, 2, deployment["asset_id"])
    wallet = out(open_tx, 3)

    annual = SAVED * RATE_PPM // 1_000_000
    gross = annual * ACCRUAL_DAA // DAA_PER_YEAR
    referral = gross * REFERRAL_PPM // 1_000_000
    net = gross - referral
    refreshed_account = copy.deepcopy(account_cfg)
    refreshed_account["savings"].update({
        "saved": SAVED + net, "delay_remaining_daa": 0,
        "remaining_accrual_daa": MAX_ACCRUAL_DAA - ACCRUAL_DAA,
    })
    refreshed_controller = copy.deepcopy(opened)
    refreshed_controller["savings"]["total_saved"] = SAVED + net
    refreshed_reserve = copy.deepcopy(current)
    refreshed_reserve["base"]["reserve_kusd"] = reserve_start - gross
    controller1 = art("controller-state", refreshed_controller)
    account1 = art("account-state", refreshed_account)
    reserve1 = art("reserve-state", refreshed_reserve)
    account_token1 = token(account_id, SAVED + net, 2)
    reserve_token1 = token(deployment["reserve_id"], reserve_start - gross, 2)
    referral_token = token(owner, referral, 0)
    registry_call = call("registry-preserve-call", opened)
    controller_call = call(
        "controller-refresh-call", opened, next_config=refreshed_controller,
        account_config=refreshed_account, account_input=2,
    )
    reserve_call = call(
        "reserve-refresh-call", opened, next_config=refreshed_reserve,
        account_config=refreshed_account, registry_input=0,
        controller_input=1, account_input=2, accrual_daa=ACCRUAL_DAA,
        account_token_input=4, reserve_token_input=5,
    )
    token_outputs = [
        (account_id, 2, SAVED + net, False),
        (deployment["reserve_id"], 2, reserve_start - gross, False),
        (owner, 0, referral, False),
    ]
    account_kcc = token_call(account_id, SAVED, 2, token_outputs)
    reserve_kcc = token_call(deployment["reserve_id"], reserve_start, 2, [], leader=False)

    def make_refresh(change, mass):
        outputs = [
            TransactionOutput(DEPOSIT, spk(registry0), binding(0, deployment["savings_registry_id"])),
            TransactionOutput(DEPOSIT, spk(controller1), binding(1, deployment["savings_controller_id"])),
            TransactionOutput(DEPOSIT, spk(account1), binding(2, account_id)),
            TransactionOutput(DEPOSIT, spk(reserve1), binding(3, deployment["reserve_id"])),
            TransactionOutput(DEPOSIT, spk(account_token1), binding(4, deployment["asset_id"])),
            TransactionOutput(DEPOSIT, spk(reserve_token1), binding(4, deployment["asset_id"])),
            TransactionOutput(DEPOSIT, spk(referral_token), binding(4, deployment["asset_id"])),
            TransactionOutput(change, owner_spk),
        ]
        unsigned = make_tx([
            inp(registry), inp(controller), inp(account, sequence=DELAY_DAA + ACCRUAL_DAA),
            inp(reserve), inp(account_token), inp(reserve_token), inp(wallet),
        ], outputs, mass)
        owner_sig = create_input_signature(unsigned, 2, key)[2:]
        account_call = call(
            "account-refresh-call", account_cfg,
            next_config=refreshed_account, owner_signature=owner_sig,
            accrual_daa=ACCRUAL_DAA, controller_input=1, reserve_input=3,
            account_token_input=4, reserve_token_input=5,
            account_id=account_id, referral_amount=referral,
        )
        tx = make_tx([
            inp(registry, registry_call), inp(controller, controller_call),
            inp(account, account_call, sequence=DELAY_DAA + ACCRUAL_DAA),
            inp(reserve, reserve_call), inp(account_token, account_kcc),
            inp(reserve_token, reserve_kcc), inp(wallet),
        ], outputs, mass)
        tx.inputs[6].signature_script = bytes.fromhex(create_input_signature(tx, 6, key))
        return tx

    refresh_tx, refresh_fee = await sized(
        client, make_refresh,
        sum(value["utxoEntry"]["amount"] for value in (
            registry, controller, account, reserve, account_token,
            reserve_token, wallet,
        )),
        7 * DEPOSIT,
    )
    registry_after = out(refresh_tx, 0, deployment["savings_registry_id"])
    controller_after = out(refresh_tx, 1, deployment["savings_controller_id"])
    account_after = out(refresh_tx, 2, account_id)
    reserve_after = out(refresh_tx, 3, deployment["reserve_id"])
    account_token_after = out(refresh_tx, 4, deployment["asset_id"])
    reserve_token_after = out(refresh_tx, 5, deployment["asset_id"])
    referral_after = out(refresh_tx, 6, deployment["asset_id"])
    wallet_after = out(refresh_tx, 7)

    closed = copy.deepcopy(refreshed_controller)
    closed["savings"]["total_saved"] = 0
    controller2 = art("controller-state", closed)
    payout = token(owner, SAVED + net, 0)
    close_controller = call(
        "controller-close-call", refreshed_controller,
        next_config=closed, account_input=1,
    )
    close_kcc = token_call(account_id, SAVED + net, 2, [(owner, 0, SAVED + net, False)])

    def make_close(change, mass):
        outputs = [
            TransactionOutput(DEPOSIT, spk(controller2), binding(0, deployment["savings_controller_id"])),
            TransactionOutput(DEPOSIT, spk(payout), binding(2, deployment["asset_id"])),
            TransactionOutput(DEPOSIT, owner_spk), TransactionOutput(change, owner_spk),
        ]
        unsigned = make_tx([
            inp(controller_after), inp(account_after), inp(account_token_after), inp(wallet_after),
        ], outputs, mass)
        owner_sig = create_input_signature(unsigned, 1, key)[2:]
        account_close = call(
            "account-close-call", refreshed_account, next_config=closed,
            owner_signature=owner_sig, controller_input=0,
            account_token_input=2, kas_output=2,
        )
        tx = make_tx([
            inp(controller_after, close_controller), inp(account_after, account_close),
            inp(account_token_after, close_kcc), inp(wallet_after),
        ], outputs, mass)
        tx.inputs[3].signature_script = bytes.fromhex(create_input_signature(tx, 3, key))
        return tx

    close_tx, close_fee = await sized(
        client, make_close, 3 * DEPOSIT + wallet_after["utxoEntry"]["amount"], 3 * DEPOSIT,
    )
    probes = [("savings_open", open_tx), ("savings_refresh", refresh_tx), ("savings_withdraw", close_tx)]
    checked = sum(write_probe(name, tx) for name, tx in probes)
    return {
        "network": NETWORK_ID,
        "protocol": PROTOCOL,
        "account_id": account_id, "gross_interest": gross,
        "principal_kusd": SAVED,
        "reserve_before_kusd": reserve_start,
        "reserve_after_kusd": reserve_start - gross,
        "referral_interest": referral, "net_interest": net,
        "checked_inputs": checked,
        "transactions": [
            {"name": name, "id": str(tx.id), "fee_sompi": fee}
            for (name, tx), fee in zip(probes, [open_fee, refresh_fee, close_fee])
        ],
        "txs": probes,
        "matures_after": DELAY_DAA + ACCRUAL_DAA,
        "live": {
            **{name: value for name, value in live.items() if name != "kusd_change"},
            "savings_registry": registry_after,
            "savings_controller": out(close_tx, 0, deployment["savings_controller_id"]),
            "reserve": reserve_after, "reserve_kusd": reserve_token_after,
            "savings_payout": out(close_tx, 1, deployment["asset_id"]),
            "savings_referral": referral_after, "wallet": out(close_tx, 3),
        },
    }


async def rebuild_withdraw(client, key, deployment, plan):
    """Rebuild close from confirmed refresh outputs."""
    owner = deployment["owner"]
    owner_spk = pay_to_address_script(key.to_public_key().to_address(NETWORK_TYPE))
    account_id = plan["account_id"]
    refresh_tx = probe_transaction("savings_refresh")
    controller_after = out(refresh_tx, 1, deployment["savings_controller_id"])
    account_after = out(refresh_tx, 2, account_id)
    account_token_after = out(refresh_tx, 4, deployment["asset_id"])
    wallet_after = out(refresh_tx, 7)

    current = base_config(deployment)
    opened = copy.deepcopy(current)
    opened["savings"].update({"account_nonce": 1, "total_saved": SAVED})
    account_cfg = copy.deepcopy(current)
    refreshed_account = copy.deepcopy(account_cfg)
    refreshed_account["savings"].update({
        "saved": SAVED + plan["net_interest"], "delay_remaining_daa": 0,
        "remaining_accrual_daa": MAX_ACCRUAL_DAA - ACCRUAL_DAA,
    })
    refreshed_controller = copy.deepcopy(opened)
    refreshed_controller["savings"]["total_saved"] = SAVED + plan["net_interest"]
    closed = copy.deepcopy(refreshed_controller)
    closed["savings"]["total_saved"] = 0
    controller2 = art("controller-state", closed)
    payout = token(owner, SAVED + plan["net_interest"], 0)
    close_controller = call(
        "controller-close-call", refreshed_controller,
        next_config=closed, account_input=1,
    )
    close_kcc = token_call(
        account_id, SAVED + plan["net_interest"], 2,
        [(owner, 0, SAVED + plan["net_interest"], False)],
    )

    def make_close(change, mass):
        outputs = [
            TransactionOutput(DEPOSIT, spk(controller2), binding(0, deployment["savings_controller_id"])),
            TransactionOutput(DEPOSIT, spk(payout), binding(2, deployment["asset_id"])),
            TransactionOutput(DEPOSIT, owner_spk), TransactionOutput(change, owner_spk),
        ]
        unsigned = make_tx([
            inp(controller_after), inp(account_after), inp(account_token_after), inp(wallet_after),
        ], outputs, mass)
        owner_sig = create_input_signature(unsigned, 1, key)[2:]
        account_close = call(
            "account-close-call", refreshed_account, next_config=closed,
            owner_signature=owner_sig, controller_input=0,
            account_token_input=2, kas_output=2,
        )
        tx = make_tx([
            inp(controller_after, close_controller), inp(account_after, account_close),
            inp(account_token_after, close_kcc), inp(wallet_after),
        ], outputs, mass)
        tx.inputs[3].signature_script = bytes.fromhex(create_input_signature(tx, 3, key))
        return tx

    close_tx, close_fee = await sized(
        client, make_close,
        3 * DEPOSIT + wallet_after["utxoEntry"]["amount"], 3 * DEPOSIT,
    )
    checked = write_probe("savings_withdraw", close_tx)
    item = next(item for item in plan["transactions"] if item["name"] == "savings_withdraw")
    item.update({"id": str(close_tx.id), "fee_sompi": close_fee})
    plan["checked_inputs"] = sum(
        write_probe(name, probe_transaction(name))
        for name in ("savings_open", "savings_refresh")
    ) + checked
    plan["live"].update({
        "savings_controller": out(close_tx, 0, deployment["savings_controller_id"]),
        "savings_payout": out(close_tx, 1, deployment["asset_id"]),
        "wallet": out(close_tx, 3),
    })
    return plan


async def savings(client, key, dry_run):
    global SAVED
    deployment = json.loads(DEPLOYMENT.read_text())
    if REMAINING.exists():
        remaining = json.loads(REMAINING.read_text())
        if remaining.get("stage") != "repay_close_validated":
            raise SystemExit(f"incomplete repay/close state in {REMAINING}")
        # repay/close spent the wallet and KUSD from the
        # genesis manifest. Continue only from their actual successors.
        deployment["live"].update({
            "bootstrap_module": remaining["live"]["module"],
            "bootstrap_module_minter": remaining["live"]["module_minter"],
            "kusd_change": remaining["live"]["kusd_remainder"],
            "wallet": remaining["live"]["wallet"],
        })
        SAVED = int(remaining["remainder_kusd"])
    else:
        SAVED = owner_kusd_change(deployment)
    if not dry_run and PROGRESS.exists():
        progress = json.loads(PROGRESS.read_text())
        completed = list(progress["completed"])
        account_daa = progress.get("account_daa")
        if completed:
            serializable = progress["plan"]
            built = None
            if "savings_open" in completed and "savings_refresh" not in completed:
                rebuilt = await build_savings(client, key, deployment)
                rebuilt_serializable = {
                    key: value for key, value in rebuilt.items() if key != "txs"
                }
                previous_open = next(
                    item["id"] for item in serializable["transactions"]
                    if item["name"] == "savings_open"
                )
                rebuilt_open = next(
                    item["id"] for item in rebuilt_serializable["transactions"]
                    if item["name"] == "savings_open"
                )
                if previous_open != rebuilt_open:
                    raise RuntimeError("Savings open changed while rebuilding refresh")
                serializable = rebuilt_serializable
                save_progress(serializable, completed, "refresh_rebuilt", account_daa)
            elif "savings_refresh" in completed and "savings_withdraw" not in completed:
                serializable = await rebuild_withdraw(client, key, deployment, serializable)
                save_progress(serializable, completed, "withdraw_rebuilt", account_daa)
        else:
            built = await build_savings(client, key, deployment)
            serializable = {k: v for k, v in built.items() if k != "txs"}
            account_daa = None
            save_progress(serializable, completed, "preflight_validated")
    else:
        built = await build_savings(client, key, deployment)
        serializable = {k: v for k, v in built.items() if k != "txs"}
        completed, account_daa = [], None
    if dry_run:
        print(json.dumps({"dry_run": True, **serializable}, indent=2))
        return
    save_progress(serializable, completed, "preflight_validated", account_daa)
    for name in ("savings_open", "savings_refresh", "savings_withdraw"):
        if name in completed:
            continue
        if name == "savings_refresh":
            if account_daa is None:
                raise RuntimeError("missing Savings account confirmation DAA")
            maturity = int(account_daa) + serializable["matures_after"]
            while int((await client.get_block_dag_info())["virtualDaaScore"]) < maturity:
                await asyncio.sleep(1)
        tx = await submit_named(client, serializable, name)
        completed.append(name)
        if name == "savings_open":
            opened = await wait_utxo(
                client,
                genesis.address_from_script_public_key(tx.outputs[1].script_public_key, NETWORK_TYPE).to_string(),
                str(tx.id), 1,
            )
            account_daa = int(opened["utxoEntry"]["blockDaaScore"])
        save_progress(serializable, completed, f"{name}_confirmed", account_daa)

    # Replace synthetic entries with confirmed network entries.
    live = {}
    for name, expected in serializable["live"].items():
        live[name] = await wait_utxo(
            client, expected["address"], expected["outpoint"]["transactionId"],
            expected["outpoint"]["index"], timeout=60,
        )
    serializable["live"] = live
    serializable["stage"] = "savings_validated"
    STATE.write_text(json.dumps(serializable, indent=2))
    save_progress(serializable, completed, serializable["stage"], account_daa)
    print(json.dumps({"stage": serializable["stage"], "file": str(STATE)}, indent=2))


async def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("command", choices=["savings-dry-run", "savings"])
    args = parser.parse_args()
    url, key = load_env()
    client = RpcClient(resolver=None, url=None if url == "public" else url, network_id=NETWORK_ID)
    await client.connect()
    try:
        await savings(client, key, args.command.endswith("dry-run"))
    finally:
        await client.disconnect()


if __name__ == "__main__":
    try:
        asyncio.run(main())
        ERROR_LOG.unlink(missing_ok=True)
    except Exception as error:
        ERROR_LOG.write_text(f"{type(error).__name__}: {error}\n")
        raise
