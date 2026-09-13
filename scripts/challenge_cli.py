"""Run the resumable Position -> Challenge -> Auction -> Reserve lifecycle."""
import argparse
import asyncio
import copy
import json
import os
from pathlib import Path

from kaspa import GenesisCovenantGroup, RpcClient, Transaction, TransactionOutput, create_input_signature, pay_to_address_script

import savings_cli as live
from network import DEPOSIT, NETWORK_ID, NETWORK_TYPE, SUBNETWORK_ID, binding, entry, inp, load_env, make_tx, sized, spk, wait_utxo

TAG = os.environ.get("KUSD_TAG", "kusd")
PROTOCOL = "KUSD"
STATE = Path(f"{TAG}-challenge-validation.local.json")
PROGRESS = Path(f"{TAG}-challenge-progress.local.json")
ERROR_LOG = Path(f"{TAG}-challenge-last-error.local.txt")
PREFIX = f"{TAG}-challenge-probe"
ZERO = "00" * 32
DEBT = 800_000_000
ASSIGNED = 80_000_000
REWARD = 8_000_000
PAYMENT = DEBT - ASSIGNED + REWARD
PERIOD = 3_600
COLLATERAL = 100_000_000_000
PREVIOUS_MODULE_REMAINING = int(
    os.environ.get("KUSD_MODULE_REMAINING", "98500000000")
)
PREVIOUS_POSITION_NONCE = int(os.environ.get("KUSD_POSITION_NONCE", "1"))
SAVINGS_GROSS = (
    live.SAVED * live.RATE_PPM // 1_000_000
) * live.ACCRUAL_DAA // live.DAA_PER_YEAR
TOTAL_KPS = 1_000_000_000
BACKSTOP_TOKEN_KAS = 2 * DEPOSIT


def reserve_before(deployment, savings_state=None):
    if savings_state is not None and "reserve_after_kusd" in savings_state:
        return int(savings_state["reserve_after_kusd"])
    return live.reserve_after_proposal(deployment) - SAVINGS_GROSS


def module_cursor():
    if live.REMAINING.exists():
        remaining = json.loads(live.REMAINING.read_text())
        if remaining.get("stage") == "repay_close_validated":
            return (
                int(remaining.get("module_remaining_after", 97_700_000_000)),
                int(remaining.get("position_nonce_after", 2)),
            )
    return PREVIOUS_MODULE_REMAINING, PREVIOUS_POSITION_NONCE


def cfg(deployment, *, position=ZERO, challenge=ZERO, reserve=None,
        collateral=0, module_remaining=PREVIOUS_MODULE_REMAINING,
        position_nonce=PREVIOUS_POSITION_NONCE, current_daa=None):
    if reserve is None:
        reserve = reserve_before(deployment)
    value = live.base_config(
        deployment, reserve_kusd=reserve, total_saved=0,
        account_nonce=1, saved=0, delay=0,
        remaining=live.MAX_ACCRUAL_DAA,
    )
    value["base"].update({
        "challenger": list(bytes.fromhex(deployment["owner"])),
        "module_id": list(bytes.fromhex(deployment["bootstrap_module_id"])),
        "position_id": list(bytes.fromhex(position)),
        "debt": DEBT, "assigned_reserve": ASSIGNED,
        "collateral_sompi": COLLATERAL, "challenge_period_daa": PERIOD,
        "reserve_kusd": reserve, "total_kps": TOTAL_KPS,
        "reserve_collateral_sompi": collateral,
        "module_remaining_mint": module_remaining, "position_nonce": position_nonce,
    })
    if current_daa is not None:
        value["base"]["current_daa"] = current_daa
    return value


def art(command, config, **values):
    mapping = {
        "module-state": "base-module-state", "position-state": "base-position-state",
        "challenged-position-state": "base-challenged-position-state",
    }
    if command == "module-state" and "state_position_nonce" in values:
        values["position_nonce"] = values.pop("state_position_nonce")
    if command in ("position-state", "challenged-position-state"):
        values["debt"] = values.pop("state_debt")
        values["assigned_reserve"] = values.pop("state_assigned_reserve")
    return live.genesis.config_tool(mapping.get(command, command), config, **values)


def call(command, config, **values):
    mapping = {"module-open-call": "base-module-open-call"}
    return bytes.fromhex(live.genesis.config_tool(mapping.get(command, command), config, **values))


def token(owner, amount, identifier, minter=False):
    return live.art("kcc-state", owner=owner, amount=amount, identifier=identifier, minter=minter)


def write_probe(name, tx):
    old = live.PROBE_PREFIX
    try:
        live.PROBE_PREFIX = PREFIX
        return live.write_probe(name, tx)
    finally:
        live.PROBE_PREFIX = old


def probe(name):
    old = live.PROBE_PREFIX
    try:
        live.PROBE_PREFIX = PREFIX
        tx = live.probe_transaction(name)
        if name == "challenge_backstop":
            for transaction_input in tx.inputs[:-1]:
                transaction_input.compute_budget = 300
            # A Schnorr P2PK signature consumes 100,000 script units.
            tx.inputs[-1].compute_budget = 10
            tx = Transaction(
                tx.version, tx.inputs, tx.outputs, tx.lock_time, SUBNETWORK_ID,
                tx.gas, bytes.fromhex(tx.payload), tx.storage_mass,
            )
            tx.finalize()
        return tx
    finally:
        live.PROBE_PREFIX = old


def txout(tx, index, covenant_id=None):
    return live.out(tx, index, covenant_id)


def save(plan, completed, stage, challenge_daa=None, auction_daa=None):
    PROGRESS.write_text(json.dumps({
        "network": NETWORK_ID, "protocol": PROTOCOL, "stage": stage,
        "completed": completed, "challenge_daa": challenge_daa,
        "auction_daa": auction_daa, "plan": plan,
    }, indent=2))


async def build(client, key, deployment, savings_state):
    owner = deployment["owner"]
    asset = deployment["asset_id"]
    reserve_id = deployment["reserve_id"]
    module_id = deployment["bootstrap_module_id"]
    owner_spk = pay_to_address_script(key.to_public_key().to_address(NETWORK_TYPE))
    source_live = copy.deepcopy(savings_state["live"])
    module = copy.deepcopy(source_live["bootstrap_module"])
    module_minter = copy.deepcopy(source_live["bootstrap_module_minter"])
    reserve = source_live["reserve"]
    reserve_token = source_live["reserve_kusd"]
    wallet = source_live["wallet"]
    reserve_balance_before = reserve_before(deployment, savings_state)
    module_remaining_before, position_nonce_before = module_cursor()
    operation_daa = await live.genesis.current_daa_resilient(client)
    remaining_daa = int(deployment["expiration_daa"]) - operation_daa
    if remaining_daa <= 0:
        raise RuntimeError("MintingModule expired; a fresh deployment is required")
    risk_premium_ppm = int(deployment.get("economics", {}).get("risk_premium_ppm", 20_000))
    effective_risk_ppm = risk_premium_ppm * remaining_daa // live.DAA_PER_YEAR
    fee_amount = DEBT * effective_risk_ppm // 1_000_000
    usable_amount = DEBT - ASSIGNED - fee_amount
    initial = cfg(
        deployment,
        module_remaining=module_remaining_before,
        position_nonce=position_nonce_before,
        reserve=reserve_balance_before,
        current_daa=operation_daa,
    )
    live.assert_script("Module bootstrap", module, art("module-state", initial,
        remaining_mint=module_remaining_before, state_position_nonce=position_nonce_before))
    live.assert_script("Reserve after Savings", reserve, art("reserve-state", initial))
    live.assert_script("KUSD Reserve after Savings", reserve_token, token(reserve_id, reserve_balance_before, 2))

    # 1. A new Position supplies exactly the debt that backstop will burn.
    position_probe = art("position-state", initial, state_debt=DEBT,
                         state_assigned_reserve=ASSIGNED, challenge_id=ZERO)
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
        reserve=reserve_balance_before,
        module_remaining=module_remaining_before,
        position_nonce=position_nonce_before,
        current_daa=operation_daa,
    )
    position0 = art("position-state", position_cfg, state_debt=DEBT,
                    state_assigned_reserve=ASSIGNED, challenge_id=ZERO)
    next_module = art("module-state", position_cfg,
                      remaining_mint=module_remaining_before-DEBT,
                      state_position_nonce=position_nonce_before+1)
    specs = [
        (module_id, 2, 0, True), (position_id, 2, 0, True),
        (owner, 0, usable_amount, False), (position_id, 2, ASSIGNED, False),
        (reserve_id, 2, fee_amount, False),
    ]
    token_arts = [token(o, a, k, m) for o, k, a, m in specs]
    module_call = call("module-open-call", position_cfg,
                       position_id=position_id, position_output=6)
    minter_call = live.token_call(module_id, 0, 2, specs, current_minter=True)

    def make_open(change, mass):
        outputs = [TransactionOutput(DEPOSIT, spk(a), binding(0, asset)) for a in token_arts]
        outputs += [
            TransactionOutput(DEPOSIT, spk(next_module), binding(1, module_id)),
            TransactionOutput(COLLATERAL, spk(position0)),
            TransactionOutput(change, owner_spk),
        ]
        tx = make_tx([inp(module_minter, minter_call), inp(module, module_call), inp(wallet)], outputs, mass,
                     lock_time=operation_daa)
        tx.populate_genesis_covenants([GenesisCovenantGroup(2, [6])])
        tx.inputs[2].signature_script = bytes.fromhex(create_input_signature(tx, 2, key))
        return tx

    open_tx, open_fee = await sized(client, make_open,
        2*DEPOSIT + wallet["utxoEntry"]["amount"], 6*DEPOSIT + COLLATERAL)
    module_minter_after = txout(open_tx, 0, asset)
    position_minter = txout(open_tx, 1, asset)
    usable = txout(open_tx, 2, asset)
    assigned = txout(open_tx, 3, asset)
    fee_token = txout(open_tx, 4, asset)
    module_after = txout(open_tx, 5, module_id)
    position = txout(open_tx, 6, position_id)
    wallet = txout(open_tx, 7)

    # 2. Position + full challenger deposit -> Position + Anchor.
    anchor = art("base-anchor-state", position_cfg)
    id_probe = make_tx(
        [inp(position), inp(wallet)],
        [TransactionOutput(COLLATERAL, owner_spk), TransactionOutput(COLLATERAL, spk(anchor))],
    )
    id_probe.populate_genesis_covenants([GenesisCovenantGroup(1, [1])])
    challenge_id = id_probe.outputs[1].to_dict()["covenant"]["covenantId"]
    challenged = art("challenged-position-state", position_cfg, state_debt=DEBT,
                     state_assigned_reserve=ASSIGNED, challenge_id=challenge_id)
    start_call = call("base-start-challenge-call", position_cfg,
                      challenger=owner, challenge_id=challenge_id, challenge_output=1)

    def make_start(change, mass):
        outputs = [
            TransactionOutput(COLLATERAL, spk(challenged), binding(0, position_id)),
            TransactionOutput(COLLATERAL, spk(anchor)), TransactionOutput(change, owner_spk),
        ]
        tx = make_tx([inp(position, start_call), inp(wallet)], outputs, mass)
        tx.populate_genesis_covenants([GenesisCovenantGroup(1, [1])])
        tx.inputs[1].signature_script = bytes.fromhex(create_input_signature(tx, 1, key))
        return tx

    start_tx, start_fee = await sized(client, make_start,
        COLLATERAL + wallet["utxoEntry"]["amount"], 2*COLLATERAL)
    challenged_position = txout(start_tx, 0, position_id)
    anchor_entry = txout(start_tx, 1, challenge_id)
    wallet = txout(start_tx, 2)

    # 3. The Anchor becomes a Challenge.
    challenge_cfg = copy.deepcopy(position_cfg)
    challenge_cfg["base"]["challenger"] = list(bytes.fromhex(owner))
    challenge = art("base-challenge-state", challenge_cfg)
    anchor_call = call("base-anchor-init-call", challenge_cfg,
                       challenger=owner, challenge_output=0)

    def make_anchor(change, mass):
        outputs = [
            TransactionOutput(COLLATERAL, spk(challenge), binding(0, challenge_id)),
            TransactionOutput(change, owner_spk),
        ]
        tx = make_tx([inp(anchor_entry, anchor_call), inp(wallet)], outputs, mass)
        tx.inputs[1].signature_script = bytes.fromhex(create_input_signature(tx, 1, key))
        return tx

    anchor_tx, anchor_fee = await sized(client, make_anchor,
        COLLATERAL + wallet["utxoEntry"]["amount"], COLLATERAL)
    challenge_entry = txout(anchor_tx, 0, challenge_id)
    wallet = txout(anchor_tx, 1)

    # 4. Permissionless activation after PERIOD DAA.
    auction = art("base-auction-state", challenge_cfg)
    activate_call = call("base-challenge-activate-call", challenge_cfg,
                         challenger=owner, auction_output=0)

    def make_activate(change, mass):
        outputs = [
            TransactionOutput(COLLATERAL, spk(auction), binding(0, challenge_id)),
            TransactionOutput(change, owner_spk),
        ]
        tx = make_tx([inp(challenge_entry, activate_call, sequence=PERIOD), inp(wallet)], outputs, mass)
        tx.inputs[1].signature_script = bytes.fromhex(create_input_signature(tx, 1, key))
        return tx

    activate_tx, activate_fee = await sized(client, make_activate,
        COLLATERAL + wallet["utxoEntry"]["amount"], COLLATERAL)
    auction_entry = txout(activate_tx, 0, challenge_id)
    wallet = txout(activate_tx, 1)

    # 5. Reserve pays debt-assigned+reward, burns all debt, and receives KAS.
    reserve_after_balance = reserve_balance_before - PAYMENT
    reserve_after_cfg = cfg(deployment, position=position_id,
                            reserve=reserve_after_balance, collateral=COLLATERAL)
    reserve_after = art("reserve-state", reserve_after_cfg)
    reserve_token_after = token(reserve_id, reserve_after_balance, 2)
    reward_token = token(owner, REWARD, 0)
    auction_call = call("base-auction-backstop-call", challenge_cfg,
        challenger=owner, next_reserve_kusd=reserve_after_balance,
        next_collateral_sompi=COLLATERAL, reserve_input=4,
        challenger_deposit_output=3)
    position_call = call("base-position-settle-call", challenge_cfg,
                         challenger=owner, challenge_id=challenge_id)
    position_minter_call = live.token_call(position_id, 0, 2, [
        (reserve_id, 2, reserve_after_balance, False), (owner, 0, REWARD, False),
    ], current_minter=True)
    assigned_call = live.token_call(position_id, ASSIGNED, 2, [], leader=False)
    reserve_call = call("base-reserve-backstop-call", challenge_cfg,
        challenger=owner, next_reserve_kusd=reserve_after_balance,
        next_collateral_sompi=COLLATERAL, auction_input=0)
    reserve_token_call = live.token_call(reserve_id, reserve_balance_before, 2, [], leader=False)

    def make_backstop(change, mass):
        outputs = [
            TransactionOutput(BACKSTOP_TOKEN_KAS, spk(reserve_token_after), binding(2, asset)),
            TransactionOutput(BACKSTOP_TOKEN_KAS, spk(reward_token), binding(2, asset)),
            TransactionOutput(DEPOSIT+COLLATERAL, spk(reserve_after), binding(4, reserve_id)),
            TransactionOutput(COLLATERAL, owner_spk), TransactionOutput(change, owner_spk),
        ]
        tx = make_tx([
            inp(auction_entry, auction_call, sequence=PERIOD, budget=300),
            inp(challenged_position, position_call, budget=300),
            inp(position_minter, position_minter_call, budget=300),
            inp(assigned, assigned_call, budget=300),
            inp(reserve, reserve_call, budget=300),
            inp(reserve_token, reserve_token_call, budget=300),
            inp(wallet, budget=10),
        ], outputs, mass)
        tx.inputs[6].signature_script = bytes.fromhex(create_input_signature(tx, 6, key))
        return tx

    backstop_tx, backstop_fee = await sized(client, make_backstop,
        2*COLLATERAL + 4*DEPOSIT + wallet["utxoEntry"]["amount"],
        2*COLLATERAL + DEPOSIT + 2*BACKSTOP_TOKEN_KAS)
    probes = [
        ("challenge_open", open_tx, open_fee), ("challenge_start", start_tx, start_fee),
        ("challenge_anchor", anchor_tx, anchor_fee), ("challenge_activate", activate_tx, activate_fee),
        ("challenge_backstop", backstop_tx, backstop_fee),
    ]
    checked = sum(write_probe(name, tx) for name, tx, _ in probes)
    return {
        "position_id": position_id, "challenge_id": challenge_id,
        "debt_burned": DEBT, "assigned_burned": ASSIGNED, "reward": REWARD,
        "reserve_payment": PAYMENT, "reserve_before": reserve_balance_before,
        "operation_daa": operation_daa,
        "remaining_module_daa": remaining_daa,
        "effective_risk_premium_ppm": effective_risk_ppm,
        "risk_fee_kusd": fee_amount,
        "usable_kusd": usable_amount,
        "module_remaining_before": module_remaining_before,
        "module_remaining_after": module_remaining_before - DEBT,
        "position_nonce_before": position_nonce_before,
        "position_nonce_after": position_nonce_before + 1,
        "reserve_after": reserve_after_balance, "challenge_period_daa": PERIOD,
        "checked_inputs": checked,
        "transactions": [{"name": n, "id": str(t.id), "fee_sompi": f} for n,t,f in probes],
        "live": {
            "module_minter": module_minter_after, "module": module_after,
            "usable_kusd": usable, "fee_kusd": fee_token,
            "reserve": txout(backstop_tx, 2, reserve_id),
            "reserve_kusd": txout(backstop_tx, 0, asset),
            "challenge_reward": txout(backstop_tx, 1, asset),
            "challenger_deposit": txout(backstop_tx, 3), "wallet": txout(backstop_tx, 4),
        },
    }


async def rebuild_backstop(client, key, deployment, savings_state, plan):
    """Rebuild backstop from confirmed parents with mass below 500,000."""
    owner, asset = deployment["owner"], deployment["asset_id"]
    reserve_id = deployment["reserve_id"]
    owner_spk = pay_to_address_script(key.to_public_key().to_address(NETWORK_TYPE))
    position_id, challenge_id = plan["position_id"], plan["challenge_id"]
    open_tx, start_tx, activate_tx = probe("challenge_open"), probe("challenge_start"), probe("challenge_activate")
    position_minter = txout(open_tx, 1, asset)
    assigned = txout(open_tx, 3, asset)
    challenged_position = txout(start_tx, 0, position_id)
    auction_entry = txout(activate_tx, 0, challenge_id)
    wallet = txout(activate_tx, 1)
    reserve = savings_state["live"]["reserve"]
    reserve_token = savings_state["live"]["reserve_kusd"]
    reserve_balance_before = reserve_before(deployment, savings_state)
    module_remaining_before = int(plan["module_remaining_before"])
    position_nonce_before = int(plan["position_nonce_before"])
    challenge_cfg = cfg(
        deployment, position=position_id, reserve=reserve_balance_before,
        module_remaining=module_remaining_before,
        position_nonce=position_nonce_before,
    )
    challenge_cfg["base"]["challenger"] = list(bytes.fromhex(owner))
    after_balance = plan["reserve_after"]
    after_cfg = cfg(
        deployment, position=position_id, reserve=after_balance,
        collateral=COLLATERAL, module_remaining=module_remaining_before,
        position_nonce=position_nonce_before,
    )
    reserve_after = art("reserve-state", after_cfg)
    reserve_token_after = token(reserve_id, after_balance, 2)
    reward_token = token(owner, REWARD, 0)
    auction_call = call("base-auction-backstop-call", challenge_cfg,
        challenger=owner, next_reserve_kusd=after_balance,
        next_collateral_sompi=COLLATERAL, reserve_input=4,
        challenger_deposit_output=3)
    position_call = call("base-position-settle-call", challenge_cfg,
                         challenger=owner, challenge_id=challenge_id)
    position_minter_call = live.token_call(position_id, 0, 2, [
        (reserve_id, 2, after_balance, False), (owner, 0, REWARD, False),
    ], current_minter=True)
    assigned_call = live.token_call(position_id, ASSIGNED, 2, [], leader=False)
    reserve_call = call("base-reserve-backstop-call", challenge_cfg,
        challenger=owner, next_reserve_kusd=after_balance,
        next_collateral_sompi=COLLATERAL, auction_input=0)
    reserve_token_call = live.token_call(reserve_id, reserve_balance_before, 2, [], leader=False)

    def make_tx_backstop(change, mass):
        outputs = [
            TransactionOutput(BACKSTOP_TOKEN_KAS, spk(reserve_token_after), binding(2, asset)),
            TransactionOutput(BACKSTOP_TOKEN_KAS, spk(reward_token), binding(2, asset)),
            TransactionOutput(DEPOSIT+COLLATERAL, spk(reserve_after), binding(4, reserve_id)),
            TransactionOutput(COLLATERAL, owner_spk), TransactionOutput(change, owner_spk),
        ]
        tx = make_tx([
            inp(auction_entry, auction_call, sequence=PERIOD, budget=300),
            inp(challenged_position, position_call, budget=300),
            inp(position_minter, position_minter_call, budget=300),
            inp(assigned, assigned_call, budget=300),
            inp(reserve, reserve_call, budget=300),
            inp(reserve_token, reserve_token_call, budget=300),
            inp(wallet, budget=10),
        ], outputs, mass)
        tx.inputs[6].signature_script = bytes.fromhex(create_input_signature(tx, 6, key))
        return tx

    tx, fee = await sized(client, make_tx_backstop,
        2*COLLATERAL + 4*DEPOSIT + wallet["utxoEntry"]["amount"],
        2*COLLATERAL + DEPOSIT + 2*BACKSTOP_TOKEN_KAS)
    write_probe("challenge_backstop", tx)
    item = next(x for x in plan["transactions"] if x["name"] == "challenge_backstop")
    item.update({"id": str(tx.id), "fee_sompi": fee})
    plan["backstop_mass"] = live.genesis.fee_mass_components(tx)["compute_mass"]
    plan["live"].update({
        "reserve": txout(tx, 2, reserve_id), "reserve_kusd": txout(tx, 0, asset),
        "challenge_reward": txout(tx, 1, asset),
        "challenger_deposit": txout(tx, 3), "wallet": txout(tx, 4),
    })
    return plan


async def submit_named(client, plan, name):
    tx = probe(name)
    expected = next(x["id"] for x in plan["transactions"] if x["name"] == name)
    if str(tx.id) != expected:
        raise RuntimeError(f"{name}: probe txid differs from the plan")
    response = await client.submit_transaction({"transaction": tx, "allowOrphan": False})
    if response["transactionId"] != expected:
        raise RuntimeError(f"{name}: unexpected RPC transaction ID")
    return await live.genesis.wait_utxo_resilient(
        client, live.genesis.address_from_script_public_key(tx.outputs[-1].script_public_key, NETWORK_TYPE).to_string(),
        expected, len(tx.outputs)-1,
    ), tx


async def run(client, key, dry):
    deployment = json.loads(live.DEPLOYMENT.read_text())
    savings_state = json.loads(live.STATE.read_text())
    if savings_state.get("stage") != "savings_validated":
        raise SystemExit("Savings must be validated before starting a Challenge")
    if not dry and PROGRESS.exists() and json.loads(PROGRESS.read_text()).get("completed"):
        progress = json.loads(PROGRESS.read_text())
        plan, completed = progress["plan"], list(progress["completed"])
        challenge_daa = progress.get("challenge_daa")
        auction_daa = progress.get("auction_daa")
        if "challenge_activate" in completed and "challenge_backstop" not in completed:
            plan = await rebuild_backstop(client, key, deployment, savings_state, plan)
            save(plan, completed, "backstop_rebuilt", challenge_daa, auction_daa)
    else:
        plan = await build(client, key, deployment, savings_state)
        completed, challenge_daa, auction_daa = [], None, None
    if dry:
        print(json.dumps({"dry_run": True, **plan}, indent=2))
        return
    save(plan, completed, "preflight_validated", challenge_daa, auction_daa)
    for name in ("challenge_open", "challenge_start", "challenge_anchor",
                 "challenge_activate", "challenge_backstop"):
        if name in completed:
            continue
        if name == "challenge_activate":
            if challenge_daa is None:
                raise RuntimeError("missing Challenge DAA")
            maturity = challenge_daa + PERIOD
            while await live.genesis.current_daa_resilient(client) < maturity:
                await asyncio.sleep(1)
        if name == "challenge_backstop":
            if auction_daa is None:
                raise RuntimeError("missing Auction DAA")
            maturity = auction_daa + PERIOD
            while await live.genesis.current_daa_resilient(client) < maturity:
                await asyncio.sleep(1)
        confirmed, tx = await submit_named(client, plan, name)
        completed.append(name)
        if name == "challenge_anchor":
            challenge_daa = int((await live.genesis.wait_utxo_resilient(
                client,
                live.genesis.address_from_script_public_key(tx.outputs[0].script_public_key, NETWORK_TYPE).to_string(),
                str(tx.id), 0,
            ))["utxoEntry"]["blockDaaScore"])
        if name == "challenge_activate":
            auction_daa = int((await live.genesis.wait_utxo_resilient(
                client,
                live.genesis.address_from_script_public_key(
                    tx.outputs[0].script_public_key, NETWORK_TYPE
                ).to_string(),
                str(tx.id), 0,
            ))["utxoEntry"]["blockDaaScore"])
        save(plan, completed, f"{name}_confirmed", challenge_daa, auction_daa)
        print(json.dumps({"confirmed": name, "transaction_id": str(tx.id)}, indent=2))
    actual = {}
    for name, expected in plan["live"].items():
        actual[name] = await live.genesis.wait_utxo_resilient(client, expected["address"],
            expected["outpoint"]["transactionId"], expected["outpoint"]["index"], timeout=60)
    plan["live"] = actual
    plan["stage"] = "challenge_backstop_validated"
    STATE.write_text(json.dumps(plan, indent=2))
    save(plan, completed, plan["stage"], challenge_daa, auction_daa)
    print(json.dumps({"stage": plan["stage"], "file": str(STATE)}, indent=2))


async def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("command", choices=["dry-run", "deploy"])
    args = parser.parse_args()
    url, key = load_env()
    client = RpcClient(resolver=None, url=None if url == "public" else url, network_id=NETWORK_ID)
    await client.connect()
    try:
        await run(client, key, args.command == "dry-run")
    finally:
        await client.disconnect()


if __name__ == "__main__":
    try:
        asyncio.run(main())
        ERROR_LOG.unlink(missing_ok=True)
    except Exception as error:
        ERROR_LOG.write_text(f"{type(error).__name__}: {error}\n")
        raise
