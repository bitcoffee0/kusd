"""Build and run a fresh open -> repay -> close network lifecycle.

Every transaction is validated by the local consensus VM before broadcast.
"""
import argparse
import asyncio
import copy
import json
import os
from pathlib import Path

from kaspa import (
    GenesisCovenantGroup,
    RpcClient,
    TransactionOutput,
    create_input_signature,
    pay_to_address_script,
)

import challenge_cli as challenge
import savings_cli as live
from network import (
    DEPOSIT,
    NETWORK_ID,
    NETWORK_TYPE,
    binding,
    inp,
    load_env,
    make_tx,
    sized,
    spk,
    wait_utxo,
)

TAG = os.environ.get("KUSD_TAG", "kusd")
PROTOCOL = "KUSD"
DEPLOYMENT = Path(f"{TAG}-deployment.local.json")
DRY_DEPLOYMENT = Path(f"{TAG}-full-dry-run.local.json")
SAVINGS = Path(f"{TAG}-live-validation.local.json")
CHALLENGE = Path(f"{TAG}-challenge-validation.local.json")
STATE = Path(f"{TAG}-remaining-validation.local.json")
PROGRESS = Path(f"{TAG}-remaining-progress.local.json")
PROBE_PREFIX = f"{TAG}-remaining-probe"

DEBT = 800_000_000
ASSIGNED = 80_000_000
USER_BURN = DEBT - ASSIGNED
COLLATERAL = 100_000_000_000
MODULE_AFTER_BOOTSTRAP = 98_500_000_000
NONCE_AFTER_BOOTSTRAP = 1


def write_probe(name, tx):
    old = live.PROBE_PREFIX
    try:
        live.PROBE_PREFIX = PROBE_PREFIX
        return live.write_probe(name, tx)
    finally:
        live.PROBE_PREFIX = old


def probe(name):
    old = live.PROBE_PREFIX
    try:
        live.PROBE_PREFIX = PROBE_PREFIX
        return live.probe_transaction(name)
    finally:
        live.PROBE_PREFIX = old


def txout(tx, index, covenant_id=None):
    return live.out(tx, index, covenant_id)


def token(owner, amount, identifier, minter=False):
    return live.art(
        "kcc-state", owner=owner, amount=amount, identifier=identifier, minter=minter
    )


def cfg(
    deployment,
    *,
    module_remaining,
    nonce,
    position=challenge.ZERO,
    debt=DEBT,
    assigned=ASSIGNED,
    current_daa=None,
):
    value = challenge.cfg(
        deployment,
        position=position,
        module_remaining=module_remaining,
        position_nonce=nonce,
    )
    value["base"].update({"debt": debt, "assigned_reserve": assigned})
    if current_daa is not None:
        value["base"]["current_daa"] = current_daa
    return value


async def build_repay_close(client, key):
    deployment_path = DEPLOYMENT if DEPLOYMENT.exists() else DRY_DEPLOYMENT
    if not deployment_path.exists():
        raise SystemExit(
            f"missing {TAG} deployment ({DEPLOYMENT} ou {DRY_DEPLOYMENT})"
        )
    deployment = json.loads(deployment_path.read_text())
    operation_daa = int((await client.get_block_dag_info())["virtualDaaScore"])
    remaining_daa = int(deployment["expiration_daa"]) - operation_daa
    if remaining_daa <= 0:
        raise RuntimeError("MintingModule expired; a fresh deployment is required")
    risk_premium_ppm = int(deployment.get("economics", {}).get("risk_premium_ppm", 20_000))
    current_fee_ppm = risk_premium_ppm * remaining_daa // live.DAA_PER_YEAR
    fee_amount = DEBT * current_fee_ppm // 1_000_000
    usable_amount = DEBT - ASSIGNED - fee_amount

    owner = deployment["owner"]
    asset = deployment["asset_id"]
    module_id = deployment["bootstrap_module_id"]
    reserve_id = deployment["reserve_id"]
    owner_spk = pay_to_address_script(key.to_public_key().to_address(NETWORK_TYPE))
    if CHALLENGE.exists():
        challenged = json.loads(CHALLENGE.read_text())
        if challenged.get("stage") != "challenge_backstop_validated":
            raise SystemExit(f"incomplete Challenge state in {CHALLENGE}")
        module = copy.deepcopy(challenged["live"]["module"])
        module_minter = copy.deepcopy(challenged["live"]["module_minter"])
        wallet = copy.deepcopy(challenged["live"]["wallet"])
        module_before = 97_700_000_000
        nonce_before = 2
    else:
        module = copy.deepcopy(deployment["live"]["bootstrap_module"])
        module_minter = copy.deepcopy(deployment["live"]["bootstrap_module_minter"])
        wallet = copy.deepcopy(deployment["live"]["wallet"])
        module_before = MODULE_AFTER_BOOTSTRAP
        nonce_before = NONCE_AFTER_BOOTSTRAP

    if SAVINGS.exists():
        savings = json.loads(SAVINGS.read_text())
        source = copy.deepcopy(savings["live"]["savings_payout"])
        source_amount = int(savings["live"]["savings_payout"].get(
            "decodedAmount", live.SAVED + savings.get("net_interest", 0)
        ))
        wallet = copy.deepcopy(savings["live"].get("wallet", wallet))
    else:
        source = copy.deepcopy(deployment["live"]["kusd_change"])
        source_amount = live.owner_kusd_change(deployment)

    initial = cfg(
        deployment, module_remaining=module_before, nonce=nonce_before,
        current_daa=operation_daa,
    )
    live.assert_script(
        "Current Module",
        module,
        challenge.art(
            "module-state",
            initial,
            remaining_mint=module_before,
            state_position_nonce=nonce_before,
        ),
    )
    live.assert_script("supplemental KUSD", source, token(owner, source_amount, 0))

    position_probe = challenge.art(
        "position-state",
        initial,
        state_debt=DEBT,
        state_assigned_reserve=ASSIGNED,
        challenge_id=challenge.ZERO,
    )
    genesis_probe = make_tx(
        [inp(module_minter), inp(module), inp(wallet)],
        [TransactionOutput(DEPOSIT, owner_spk) for _ in range(6)]
        + [TransactionOutput(COLLATERAL, spk(position_probe))],
    )
    genesis_probe.populate_genesis_covenants([GenesisCovenantGroup(2, [6])])
    position_id = genesis_probe.outputs[6].to_dict()["covenant"]["covenantId"]
    position_cfg = cfg(
        deployment,
        position=position_id,
        module_remaining=module_before,
        nonce=nonce_before,
        current_daa=operation_daa,
    )
    position0 = challenge.art(
        "position-state",
        position_cfg,
        state_debt=DEBT,
        state_assigned_reserve=ASSIGNED,
        challenge_id=challenge.ZERO,
    )
    position_zero = challenge.art(
        "position-state",
        position_cfg,
        state_debt=0,
        state_assigned_reserve=0,
        challenge_id=challenge.ZERO,
    )
    next_module = challenge.art(
        "module-state",
        position_cfg,
        remaining_mint=module_before - DEBT,
        state_position_nonce=nonce_before + 1,
    )
    specs = [
        (module_id, 2, 0, True),
        (position_id, 2, 0, True),
        (owner, 0, usable_amount, False),
        (position_id, 2, ASSIGNED, False),
        (reserve_id, 2, fee_amount, False),
    ]
    token_arts = [token(o, amount, kind, minter) for o, kind, amount, minter in specs]
    module_call = challenge.call(
        "module-open-call", position_cfg, position_id=position_id, position_output=6
    )
    minter_call = live.token_call(module_id, 0, 2, specs, current_minter=True)

    def make_open(change, mass):
        outputs = [
            TransactionOutput(DEPOSIT, spk(artifact), binding(0, asset))
            for artifact in token_arts
        ]
        outputs += [
            TransactionOutput(DEPOSIT, spk(next_module), binding(1, module_id)),
            TransactionOutput(COLLATERAL, spk(position0)),
            TransactionOutput(change, owner_spk),
        ]
        tx = make_tx(
            [inp(module_minter, minter_call), inp(module, module_call), inp(wallet)],
            outputs,
            mass,
            lock_time=operation_daa,
        )
        tx.populate_genesis_covenants([GenesisCovenantGroup(2, [6])])
        tx.inputs[2].signature_script = bytes.fromhex(create_input_signature(tx, 2, key))
        return tx

    open_tx, open_fee = await sized(
        client,
        make_open,
        2 * DEPOSIT + wallet["utxoEntry"]["amount"],
        6 * DEPOSIT + COLLATERAL,
    )
    module_minter_after = txout(open_tx, 0, asset)
    position_minter = txout(open_tx, 1, asset)
    usable = txout(open_tx, 2, asset)
    assigned = txout(open_tx, 3, asset)
    fee_token = txout(open_tx, 4, asset)
    module_after = txout(open_tx, 5, module_id)
    position = txout(open_tx, 6, position_id)
    wallet_after_open = txout(open_tx, 7)

    remainder_amount = source_amount + usable_amount - USER_BURN
    repayment_art = token(owner, USER_BURN, 0)
    remainder_art = token(owner, remainder_amount, 0)

    def make_split(change, mass):
        outputs = [
            TransactionOutput(DEPOSIT, spk(repayment_art), binding(0, asset)),
            TransactionOutput(DEPOSIT, spk(remainder_art), binding(0, asset)),
            TransactionOutput(change, owner_spk),
        ]
        unsigned = make_tx([inp(source), inp(usable), inp(wallet_after_open)], outputs, mass)
        source_sig = create_input_signature(unsigned, 0, key)[2:]
        usable_sig = create_input_signature(unsigned, 1, key)[2:]
        source_call = live.token_call(
            owner,
            source_amount,
            0,
            [(owner, 0, USER_BURN, False), (owner, 0, remainder_amount, False)],
            source_sig,
        )
        usable_call = live.token_call(owner, usable_amount, 0, [], usable_sig, leader=False)
        tx = make_tx(
            [inp(source, source_call), inp(usable, usable_call), inp(wallet_after_open)],
            outputs,
            mass,
        )
        tx.inputs[2].signature_script = bytes.fromhex(create_input_signature(tx, 2, key))
        return tx

    split_tx, split_fee = await sized(
        client,
        make_split,
        2 * DEPOSIT + wallet_after_open["utxoEntry"]["amount"],
        2 * DEPOSIT,
    )
    repayment = txout(split_tx, 0, asset)
    remainder = txout(split_tx, 1, asset)
    wallet_after_split = txout(split_tx, 2)

    repay_call = challenge.call(
        "base-position-repay-call", position_cfg, repay_amount=DEBT
    )
    minter_repay = live.token_call(
        position_id,
        0,
        2,
        [(position_id, 2, 0, True)],
        current_minter=True,
    )

    def make_repay(change, mass):
        outputs = [
            TransactionOutput(DEPOSIT, spk(token(position_id, 0, 2, True)), binding(1, asset)),
            TransactionOutput(COLLATERAL, spk(position_zero), binding(0, position_id)),
            TransactionOutput(change, owner_spk),
        ]
        unsigned = make_tx(
            [inp(position), inp(position_minter), inp(repayment), inp(assigned), inp(wallet_after_split)],
            outputs,
            mass,
        )
        repayment_sig = create_input_signature(unsigned, 2, key)[2:]
        repayment_call = live.token_call(owner, USER_BURN, 0, [], repayment_sig, leader=False)
        assigned_call = live.token_call(position_id, ASSIGNED, 2, [], leader=False)
        tx = make_tx(
            [
                inp(position, repay_call),
                inp(position_minter, minter_repay),
                inp(repayment, repayment_call),
                inp(assigned, assigned_call),
                inp(wallet_after_split),
            ],
            outputs,
            mass,
        )
        tx.inputs[4].signature_script = bytes.fromhex(create_input_signature(tx, 4, key))
        return tx

    repay_tx, repay_fee = await sized(
        client,
        make_repay,
        COLLATERAL + 3 * DEPOSIT + wallet_after_split["utxoEntry"]["amount"],
        COLLATERAL + DEPOSIT,
    )
    minter_zero = txout(repay_tx, 0, asset)
    position_repaid = txout(repay_tx, 1, position_id)
    wallet_after_repay = txout(repay_tx, 2)

    def make_close(change, mass):
        outputs = [
            TransactionOutput(COLLATERAL, owner_spk),
            TransactionOutput(change, owner_spk),
        ]
        unsigned = make_tx(
            [inp(position_repaid), inp(minter_zero), inp(wallet_after_repay)], outputs, mass
        )
        owner_sig = create_input_signature(unsigned, 0, key)[2:]
        close_call = challenge.call(
            "base-position-close-call", position_cfg, owner_signature=owner_sig
        )
        minter_close = live.token_call(position_id, 0, 2, [], current_minter=True)
        tx = make_tx(
            [inp(position_repaid, close_call), inp(minter_zero, minter_close), inp(wallet_after_repay)],
            outputs,
            mass,
        )
        tx.inputs[2].signature_script = bytes.fromhex(create_input_signature(tx, 2, key))
        return tx

    close_tx, close_fee = await sized(
        client,
        make_close,
        COLLATERAL + DEPOSIT + wallet_after_repay["utxoEntry"]["amount"],
        COLLATERAL,
    )

    transactions = [
        ("remaining_open", open_tx, open_fee),
        ("remaining_split", split_tx, split_fee),
        ("remaining_repay", repay_tx, repay_fee),
        ("remaining_close", close_tx, close_fee),
    ]
    checked = sum(write_probe(name, tx) for name, tx, _ in transactions)
    return {
        "network": NETWORK_ID,
        "protocol": PROTOCOL,
        "path": "open_repay_close",
        "position_id": position_id,
        "debt_burned": DEBT,
        "operation_daa": operation_daa,
        "remaining_module_daa": remaining_daa,
        "effective_risk_premium_ppm": current_fee_ppm,
        "risk_fee_kusd": fee_amount,
        "usable_kusd": usable_amount,
        "remainder_kusd": remainder_amount,
        "module_remaining_before": module_before,
        "module_remaining_after": module_before - DEBT,
        "position_nonce_before": nonce_before,
        "position_nonce_after": nonce_before + 1,
        "collateral_returned_sompi": COLLATERAL,
        "checked_inputs": checked,
        "transactions": [
            {"name": name, "id": str(tx.id), "fee_sompi": fee}
            for name, tx, fee in transactions
        ],
        "live": {
            "module": module_after,
            "module_minter": module_minter_after,
            "fee_kusd": fee_token,
            "kusd_remainder": remainder,
            "collateral_return": txout(close_tx, 0),
            "wallet": txout(close_tx, 1),
        },
    }


def save(plan, completed, stage):
    PROGRESS.write_text(
        json.dumps({"stage": stage, "completed": completed, "plan": plan}, indent=2)
    )


async def submit_plan(client, plan):
    completed = []
    if PROGRESS.exists():
        progress = json.loads(PROGRESS.read_text())
        if progress.get("plan", {}).get("position_id") == plan["position_id"]:
            completed = list(progress.get("completed", []))
    for item in plan["transactions"]:
        name = item["name"]
        if name in completed:
            continue
        tx = probe(name)
        if str(tx.id) != item["id"]:
            raise RuntimeError(f"{name}: txid probe divergent")
        response = await client.submit_transaction(
            {"transaction": tx, "allowOrphan": False}
        )
        if response["transactionId"] != item["id"]:
            raise RuntimeError(f"{name}: txid RPC divergent")
        await wait_utxo(
            client,
            live.genesis.address_from_script_public_key(
                tx.outputs[-1].script_public_key, NETWORK_TYPE
            ).to_string(),
            item["id"],
            len(tx.outputs) - 1,
        )
        completed.append(name)
        save(plan, completed, f"{name}_confirmed")
        print(json.dumps({"confirmed": name, "transaction_id": item["id"]}, indent=2))
    plan["stage"] = "repay_close_validated"
    plan["live"] = {
        name: await wait_utxo(
            client,
            value["address"],
            value["outpoint"]["transactionId"],
            value["outpoint"]["index"],
        )
        for name, value in plan["live"].items()
    }
    STATE.write_text(json.dumps(plan, indent=2))
    save(plan, completed, plan["stage"])


async def verify(client):
    state = json.loads(STATE.read_text())
    checked = {}
    for name, expected in state["live"].items():
        checked[name] = (
            await wait_utxo(
                client,
                expected["address"],
                expected["outpoint"]["transactionId"],
                expected["outpoint"]["index"],
                timeout=15,
            )
        )["outpoint"]
    print(json.dumps({"stage": state["stage"], "live": checked}, indent=2))


async def main():
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "command", choices=["repay-close-dry-run", "repay-close", "verify"]
    )
    args = parser.parse_args()
    url, key = load_env()
    client = RpcClient(
        resolver=None, url=None if url == "public" else url, network_id=NETWORK_ID
    )
    await client.connect()
    try:
        if args.command == "verify":
            await verify(client)
            return
        if args.command == "repay-close" and STATE.exists():
            raise SystemExit(f"{STATE} already exists; refusing rebroadcast")
        progress = json.loads(PROGRESS.read_text()) if PROGRESS.exists() else None
        if (
            args.command == "repay-close"
            and progress
            and progress.get("completed")
        ):
            plan = progress["plan"]
        else:
            plan = await build_repay_close(client, key)
        print(
            json.dumps(
                {
                    "dry_run": args.command.endswith("dry-run"),
                    "position_id": plan["position_id"],
                    "checked_inputs": plan["checked_inputs"],
                    "transactions": plan["transactions"],
                },
                indent=2,
            )
        )
        if args.command == "repay-close":
            await submit_plan(client, plan)
    finally:
        await client.disconnect()


if __name__ == "__main__":
    asyncio.run(main())
