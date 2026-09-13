"""Validate an auction with distinct owner, challenger, and bidder wallets."""
import argparse
import asyncio
import copy
import json
import os
import secrets
from pathlib import Path

from kaspa import (
    GenesisCovenantGroup,
    PrivateKey,
    RpcClient,
    TransactionOutput,
    create_input_signature,
    pay_to_address_script,
    sign_transaction,
)

os.environ.setdefault("KUSD_TAG", "kusd")

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

TAG = os.environ["KUSD_TAG"]
KEYS = Path(f"{TAG}-multiwallet.env.local")
STATE = Path(f"{TAG}-multiwallet-validation.local.json")
PROGRESS = Path(f"{TAG}-multiwallet-progress.local.json")
PREFIX = f"{TAG}-multiwallet-probe"

DEBT = 800_000_000
ASSIGNED = 80_000_000
REWARD = 8_000_000
PAYMENT = DEBT - ASSIGNED + REWARD
COLLATERAL = 100_000_000_000
PERIOD = 3_600
MODULE_BEFORE = 96_900_000_000
NONCE_BEFORE = 3


def role_keys():
    if not KEYS.exists():
        generated = []
        for _ in range(2):
            while True:
                try:
                    generated.append(PrivateKey(secrets.token_hex(32)))
                    break
                except Exception:
                    pass
        KEYS.write_text(
            "KASPA_CHALLENGER_PRIVATE_KEY=" + generated[0].to_string() + "\n"
            "KASPA_BIDDER_PRIVATE_KEY=" + generated[1].to_string() + "\n"
        )
    values = dict(
        line.split("=", 1) for line in KEYS.read_text().splitlines() if "=" in line
    )
    return (
        PrivateKey(values["KASPA_CHALLENGER_PRIVATE_KEY"]),
        PrivateKey(values["KASPA_BIDDER_PRIVATE_KEY"]),
    )


def cfg(deployment, challenger_key, *, position=challenge.ZERO, reserve=None,
        module_remaining=MODULE_BEFORE, position_nonce=NONCE_BEFORE,
        current_daa=None):
    value = challenge.cfg(
        deployment,
        position=position,
        reserve=reserve,
        collateral=COLLATERAL,
        module_remaining=module_remaining,
        position_nonce=position_nonce,
        current_daa=current_daa,
    )
    value["base"]["challenger"] = list(
        bytes.fromhex(challenger_key.to_public_key().to_x_only_public_key().to_string())
    )
    return value


def token(owner, amount, identifier, minter=False):
    return live.art(
        "kcc-state", owner=owner, amount=amount, identifier=identifier, minter=minter
    )


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
        return live.probe_transaction(name)
    finally:
        live.PROBE_PREFIX = old


def txout(tx, index, covenant_id=None):
    return live.out(tx, index, covenant_id)


async def build(client, owner_key):
    deployment = json.loads(challenge.live.DEPLOYMENT.read_text())
    savings = json.loads(challenge.live.STATE.read_text())
    previous = json.loads(challenge.STATE.read_text())
    if previous.get("stage") != "challenge_backstop_validated":
        raise SystemExit("validated backstop state required")
    challenger_key, bidder_key = role_keys()
    owner = deployment["owner"]
    challenger = challenger_key.to_public_key().to_x_only_public_key().to_string()
    bidder = bidder_key.to_public_key().to_x_only_public_key().to_string()
    owner_spk = pay_to_address_script(owner_key.to_public_key().to_address(NETWORK_TYPE))
    challenger_spk = pay_to_address_script(
        challenger_key.to_public_key().to_address(NETWORK_TYPE)
    )
    bidder_spk = pay_to_address_script(bidder_key.to_public_key().to_address(NETWORK_TYPE))
    asset = deployment["asset_id"]
    module_id = deployment["bootstrap_module_id"]
    operation_daa = int((await client.get_block_dag_info())["virtualDaaScore"])
    remaining_daa = int(deployment["expiration_daa"]) - operation_daa
    if remaining_daa <= 0:
        raise RuntimeError("MintingModule expired; a fresh deployment is required")
    annual_risk_ppm = int(deployment.get("economics", {}).get("risk_premium_ppm", 20_000))
    effective_risk_ppm = annual_risk_ppm * remaining_daa // live.DAA_PER_YEAR
    fee_amount = DEBT * effective_risk_ppm // 1_000_000
    usable_amount = DEBT - ASSIGNED - fee_amount
    reserve_balance = int(previous["reserve_after"])
    module_before = int(previous.get("module_remaining_after", MODULE_BEFORE))
    nonce_before = int(previous.get("position_nonce_after", NONCE_BEFORE))

    wallet = copy.deepcopy(previous["live"]["wallet"])

    def make_fund(change, mass):
        tx = make_tx(
            [inp(wallet)],
            [
                TransactionOutput(1_010 * 100_000_000, challenger_spk),
                TransactionOutput(10 * 100_000_000, bidder_spk),
                TransactionOutput(change, owner_spk),
            ],
            mass,
        )
        return sign_transaction(tx, [owner_key], True)

    fund_tx, fund_fee = await sized(
        client, make_fund, wallet["utxoEntry"]["amount"], 1_020 * 100_000_000
    )
    challenger_wallet = txout(fund_tx, 0)
    bidder_wallet = txout(fund_tx, 1)
    owner_wallet = txout(fund_tx, 2)

    sources = [
        copy.deepcopy(previous["live"]["usable_kusd"]),
        copy.deepcopy(previous["live"]["challenge_reward"]),
        copy.deepcopy(savings["live"]["savings_payout"]),
    ]
    source_amounts = [
        int(previous.get("usable_kusd", 704_000_000)),
        int(previous["reward"]),
        int(savings.get("principal_kusd", live.SAVED)) + int(savings.get("net_interest", 0)),
    ]
    remainder_amount = sum(source_amounts) - PAYMENT
    payment_art = token(bidder, PAYMENT, 0)
    remainder_art = token(owner, remainder_amount, 0)
    payment_specs = [(bidder, 0, PAYMENT, False), (owner, 0, remainder_amount, False)]

    def make_payment(change, mass):
        outputs = [
            TransactionOutput(DEPOSIT, spk(payment_art), binding(0, asset)),
            TransactionOutput(DEPOSIT, spk(remainder_art), binding(0, asset)),
            TransactionOutput(change, owner_spk),
        ]
        unsigned = make_tx(
            [inp(x) for x in sources] + [inp(owner_wallet)], outputs, mass
        )
        signatures = [
            create_input_signature(unsigned, index, owner_key)[2:]
            for index in range(3)
        ]
        calls = [
            live.token_call(owner, source_amounts[0], 0, payment_specs, signatures[0]),
            live.token_call(
                owner, source_amounts[1], 0, [], signatures[1], leader=False
            ),
            live.token_call(
                owner, source_amounts[2], 0, [], signatures[2], leader=False
            ),
        ]
        tx = make_tx(
            [inp(x, call) for x, call in zip(sources, calls)]
            + [inp(owner_wallet)],
            outputs,
            mass,
        )
        tx.inputs[3].signature_script = bytes.fromhex(
            create_input_signature(tx, 3, owner_key)
        )
        return tx

    payment_tx, payment_fee = await sized(
        client,
        make_payment,
        3 * DEPOSIT + owner_wallet["utxoEntry"]["amount"],
        2 * DEPOSIT,
    )
    payment = txout(payment_tx, 0, asset)
    remainder = txout(payment_tx, 1, asset)
    owner_wallet = txout(payment_tx, 2)

    module = copy.deepcopy(previous["live"]["module"])
    module_minter = copy.deepcopy(previous["live"]["module_minter"])
    initial = cfg(
        deployment, challenger_key, reserve=reserve_balance,
        module_remaining=module_before, position_nonce=nonce_before,
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
    position_probe = challenge.art(
        "position-state",
        initial,
        state_debt=DEBT,
        state_assigned_reserve=ASSIGNED,
        challenge_id=challenge.ZERO,
    )
    id_probe = make_tx(
        [inp(module_minter), inp(module), inp(owner_wallet)],
        [TransactionOutput(DEPOSIT, owner_spk) for _ in range(6)]
        + [TransactionOutput(COLLATERAL, spk(position_probe))],
    )
    id_probe.populate_genesis_covenants([GenesisCovenantGroup(2, [6])])
    position_id = id_probe.outputs[6].to_dict()["covenant"]["covenantId"]
    position_cfg = cfg(
        deployment, challenger_key, position=position_id,
        reserve=reserve_balance, module_remaining=module_before,
        position_nonce=nonce_before, current_daa=operation_daa,
    )
    position0 = challenge.art(
        "position-state",
        position_cfg,
        state_debt=DEBT,
        state_assigned_reserve=ASSIGNED,
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
        (deployment["reserve_id"], 2, fee_amount, False),
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
        ] + [
            TransactionOutput(DEPOSIT, spk(next_module), binding(1, module_id)),
            TransactionOutput(COLLATERAL, spk(position0)),
            TransactionOutput(change, owner_spk),
        ]
        tx = make_tx(
            [inp(module_minter, minter_call), inp(module, module_call), inp(owner_wallet)],
            outputs,
            mass,
            lock_time=operation_daa,
        )
        tx.populate_genesis_covenants([GenesisCovenantGroup(2, [6])])
        tx.inputs[2].signature_script = bytes.fromhex(
            create_input_signature(tx, 2, owner_key)
        )
        return tx

    open_tx, open_fee = await sized(
        client,
        make_open,
        2 * DEPOSIT + owner_wallet["utxoEntry"]["amount"],
        6 * DEPOSIT + COLLATERAL,
    )
    next_module_minter = txout(open_tx, 0, asset)
    position_minter = txout(open_tx, 1, asset)
    usable = txout(open_tx, 2, asset)
    assigned = txout(open_tx, 3, asset)
    fee_token = txout(open_tx, 4, asset)
    next_module_entry = txout(open_tx, 5, module_id)
    position = txout(open_tx, 6, position_id)
    owner_wallet = txout(open_tx, 7)

    anchor = challenge.art("base-anchor-state", position_cfg)
    challenge_id_probe = make_tx(
        [inp(position), inp(challenger_wallet)],
        [
            TransactionOutput(COLLATERAL, challenger_spk),
            TransactionOutput(COLLATERAL, spk(anchor)),
        ],
    )
    challenge_id_probe.populate_genesis_covenants([GenesisCovenantGroup(1, [1])])
    challenge_id = challenge_id_probe.outputs[1].to_dict()["covenant"]["covenantId"]
    challenged = challenge.art(
        "challenged-position-state",
        position_cfg,
        state_debt=DEBT,
        state_assigned_reserve=ASSIGNED,
        challenge_id=challenge_id,
    )
    start_call = challenge.call(
        "base-start-challenge-call",
        position_cfg,
        challenger=challenger,
        challenge_id=challenge_id,
        challenge_output=1,
    )

    def make_start(change, mass):
        outputs = [
            TransactionOutput(COLLATERAL, spk(challenged), binding(0, position_id)),
            TransactionOutput(COLLATERAL, spk(anchor)),
            TransactionOutput(change, challenger_spk),
        ]
        tx = make_tx([inp(position, start_call), inp(challenger_wallet)], outputs, mass)
        tx.populate_genesis_covenants([GenesisCovenantGroup(1, [1])])
        tx.inputs[1].signature_script = bytes.fromhex(
            create_input_signature(tx, 1, challenger_key)
        )
        return tx

    start_tx, start_fee = await sized(
        client,
        make_start,
        COLLATERAL + challenger_wallet["utxoEntry"]["amount"],
        2 * COLLATERAL,
    )
    challenged_position = txout(start_tx, 0, position_id)
    anchor_entry = txout(start_tx, 1, challenge_id)
    challenger_wallet = txout(start_tx, 2)

    challenge_cfg = copy.deepcopy(position_cfg)
    challenge_cfg["base"]["challenger"] = list(bytes.fromhex(challenger))
    challenge_art = challenge.art("base-challenge-state", challenge_cfg)
    anchor_call = challenge.call(
        "base-anchor-init-call",
        challenge_cfg,
        challenger=challenger,
        challenge_output=0,
    )

    def make_anchor(change, mass):
        outputs = [
            TransactionOutput(COLLATERAL, spk(challenge_art), binding(0, challenge_id)),
            TransactionOutput(change, challenger_spk),
        ]
        tx = make_tx([inp(anchor_entry, anchor_call), inp(challenger_wallet)], outputs, mass)
        tx.inputs[1].signature_script = bytes.fromhex(
            create_input_signature(tx, 1, challenger_key)
        )
        return tx

    anchor_tx, anchor_fee = await sized(
        client,
        make_anchor,
        COLLATERAL + challenger_wallet["utxoEntry"]["amount"],
        COLLATERAL,
    )
    challenge_entry = txout(anchor_tx, 0, challenge_id)
    challenger_wallet = txout(anchor_tx, 1)

    auction = challenge.art("base-auction-state", challenge_cfg)
    activate_call = challenge.call(
        "base-challenge-activate-call",
        challenge_cfg,
        challenger=challenger,
        auction_output=0,
    )

    def make_activate(change, mass):
        outputs = [
            TransactionOutput(COLLATERAL, spk(auction), binding(0, challenge_id)),
            TransactionOutput(change, challenger_spk),
        ]
        tx = make_tx(
            [inp(challenge_entry, activate_call, sequence=PERIOD), inp(challenger_wallet)],
            outputs,
            mass,
        )
        tx.inputs[1].signature_script = bytes.fromhex(
            create_input_signature(tx, 1, challenger_key)
        )
        return tx

    activate_tx, activate_fee = await sized(
        client,
        make_activate,
        COLLATERAL + challenger_wallet["utxoEntry"]["amount"],
        COLLATERAL,
    )
    auction_entry = txout(activate_tx, 0, challenge_id)
    challenger_wallet = txout(activate_tx, 1)

    reward_art = token(challenger, REWARD, 0)
    owner_surplus_art = token(owner, 0, 0)
    auction_call = challenge.call(
        "base-auction-settle-call",
        challenge_cfg,
        bidder=bidder,
        bidder_collateral_output=2,
        challenger_deposit_output=3,
        elapsed_daa=PERIOD,
    )
    position_call = challenge.call(
        "base-position-settle-call",
        challenge_cfg,
        challenger=challenger,
        challenge_id=challenge_id,
    )
    minter_settle = live.token_call(
        position_id,
        0,
        2,
        [(challenger, 0, REWARD, False), (owner, 0, 0, False)],
        current_minter=True,
    )
    assigned_call = live.token_call(position_id, ASSIGNED, 2, [], leader=False)

    def make_settle(change, mass):
        outputs = [
            TransactionOutput(DEPOSIT, spk(reward_art), binding(2, asset)),
            TransactionOutput(DEPOSIT, spk(owner_surplus_art), binding(2, asset)),
            TransactionOutput(COLLATERAL, bidder_spk),
            TransactionOutput(COLLATERAL, challenger_spk),
            TransactionOutput(change, bidder_spk),
        ]
        unsigned = make_tx(
            [
                inp(auction_entry),
                inp(challenged_position),
                inp(position_minter),
                inp(payment),
                inp(assigned),
                inp(bidder_wallet),
            ],
            outputs,
            mass,
            lock_time=PERIOD,
        )
        bidder_sig = create_input_signature(unsigned, 3, bidder_key)[2:]
        payment_call = live.token_call(
            bidder, PAYMENT, 0, [], bidder_sig, leader=False
        )
        tx = make_tx(
            [
                inp(auction_entry, auction_call),
                inp(challenged_position, position_call),
                inp(position_minter, minter_settle),
                inp(payment, payment_call),
                inp(assigned, assigned_call),
                inp(bidder_wallet),
            ],
            outputs,
            mass,
            lock_time=PERIOD,
        )
        tx.inputs[5].signature_script = bytes.fromhex(
            create_input_signature(tx, 5, bidder_key)
        )
        return tx

    settle_tx, settle_fee = await sized(
        client,
        make_settle,
        2 * COLLATERAL + 3 * DEPOSIT + bidder_wallet["utxoEntry"]["amount"],
        2 * COLLATERAL + 2 * DEPOSIT,
    )
    transactions = [
        ("multiwallet_fund", fund_tx, fund_fee),
        ("multiwallet_payment", payment_tx, payment_fee),
        ("multiwallet_open", open_tx, open_fee),
        ("multiwallet_start", start_tx, start_fee),
        ("multiwallet_anchor", anchor_tx, anchor_fee),
        ("multiwallet_activate", activate_tx, activate_fee),
        ("multiwallet_settle", settle_tx, settle_fee),
    ]
    checked = sum(write_probe(name, tx) for name, tx, _ in transactions)
    return {
        "network": NETWORK_ID,
        "protocol": "KUSD",
        "position_id": position_id,
        "challenge_id": challenge_id,
        "owner": owner,
        "challenger": challenger,
        "bidder": bidder,
        "debt_burned": DEBT,
        "operation_daa": operation_daa,
        "remaining_module_daa": remaining_daa,
        "effective_risk_premium_ppm": effective_risk_ppm,
        "risk_fee_kusd": fee_amount,
        "usable_kusd": usable_amount,
        "remainder_kusd": remainder_amount,
        "module_remaining_before": module_before,
        "module_remaining_after": module_before - DEBT,
        "position_nonce_before": nonce_before,
        "position_nonce_after": nonce_before + 1,
        "checked_inputs": checked,
        "matures_after": PERIOD,
        "transactions": [
            {"name": name, "id": str(tx.id), "fee_sompi": fee}
            for name, tx, fee in transactions
        ],
        "live": {
            "module": next_module_entry,
            "module_minter": next_module_minter,
            "usable_kusd": usable,
            "fee_kusd": fee_token,
            "kusd_remainder": remainder,
            "challenger_reward": txout(settle_tx, 0, asset),
            "owner_surplus": txout(settle_tx, 1, asset),
            "bidder_collateral": txout(settle_tx, 2),
            "challenger_deposit": txout(settle_tx, 3),
            "bidder_wallet": txout(settle_tx, 4),
            "owner_wallet": owner_wallet,
        },
    }


def save(plan, completed, stage, challenge_daa=None, auction_daa=None):
    PROGRESS.write_text(
        json.dumps(
            {
                "stage": stage,
                "completed": completed,
                "challenge_daa": challenge_daa,
                "auction_daa": auction_daa,
                "plan": plan,
            },
            indent=2,
        )
    )


def rebuild_settle(plan, auction_daa):
    """Bind settlement to the DAA score of the confirmed Auction UTXO."""
    _, bidder_key = role_keys()
    deployment = json.loads(challenge.live.DEPLOYMENT.read_text())
    asset = deployment["asset_id"]
    auction_entry = txout(
        probe("multiwallet_activate"), 0, plan["challenge_id"]
    )
    auction_entry["utxoEntry"]["blockDaaScore"] = auction_daa
    challenged_position = txout(
        probe("multiwallet_start"), 0, plan["position_id"]
    )
    position_minter = txout(probe("multiwallet_open"), 1, asset)
    payment = txout(probe("multiwallet_payment"), 0, asset)
    assigned = txout(probe("multiwallet_open"), 3, asset)
    bidder_wallet = txout(probe("multiwallet_fund"), 1)
    old = probe("multiwallet_settle")
    old_calls = [bytes.fromhex(item.signature_script_as_hex) for item in old.inputs]
    lock_time = auction_daa + PERIOD

    unsigned = make_tx(
        [
            inp(auction_entry),
            inp(challenged_position),
            inp(position_minter),
            inp(payment),
            inp(assigned),
            inp(bidder_wallet),
        ],
        old.outputs,
        old.storage_mass,
        lock_time=lock_time,
    )
    bidder_signature = create_input_signature(unsigned, 3, bidder_key)[2:]
    payment_call = live.token_call(
        plan["bidder"], PAYMENT, 0, [], bidder_signature, leader=False
    )
    rebuilt = make_tx(
        [
            inp(auction_entry, old_calls[0]),
            inp(challenged_position, old_calls[1]),
            inp(position_minter, old_calls[2]),
            inp(payment, payment_call),
            inp(assigned, old_calls[4]),
            inp(bidder_wallet),
        ],
        old.outputs,
        old.storage_mass,
        lock_time=lock_time,
    )
    rebuilt.inputs[5].signature_script = bytes.fromhex(
        create_input_signature(rebuilt, 5, bidder_key)
    )
    rebuilt.finalize()
    checked = write_probe("multiwallet_settle", rebuilt)
    item = next(
        value for value in plan["transactions"]
        if value["name"] == "multiwallet_settle"
    )
    item["id"] = str(rebuilt.id)
    plan["settle_lock_time"] = lock_time
    plan["settle_checked_inputs"] = checked
    for name, index, covenant_id in (
        ("challenger_reward", 0, asset),
        ("owner_surplus", 1, asset),
        ("bidder_collateral", 2, None),
        ("challenger_deposit", 3, None),
        ("bidder_wallet", 4, None),
    ):
        plan["live"][name] = txout(rebuilt, index, covenant_id)
    return rebuilt


async def submit(client, plan):
    completed, challenge_daa, auction_daa = [], None, None
    if PROGRESS.exists():
        progress = json.loads(PROGRESS.read_text())
        if progress.get("plan", {}).get("position_id") == plan["position_id"]:
            completed = list(progress.get("completed", []))
            challenge_daa = progress.get("challenge_daa")
            auction_daa = progress.get("auction_daa")
    for item in plan["transactions"]:
        name = item["name"]
        if name in completed:
            continue
        if name == "multiwallet_activate":
            if challenge_daa is None:
                raise RuntimeError("missing Challenge confirmation DAA")
            maturity = challenge_daa + plan["matures_after"]
            while await live.genesis.current_daa_resilient(client) < maturity:
                await asyncio.sleep(1)
        if name == "multiwallet_settle":
            if auction_daa is None:
                raise RuntimeError("missing Auction confirmation DAA")
            maturity = auction_daa + plan["matures_after"]
            if plan.get("settle_lock_time") != maturity:
                rebuild_settle(plan, auction_daa)
                save(
                    plan, completed, "multiwallet_settle_rebuilt",
                    challenge_daa, auction_daa,
                )
            while await live.genesis.current_daa_resilient(client) < maturity:
                await asyncio.sleep(1)
        tx = probe(name)
        if str(tx.id) != item["id"]:
            raise RuntimeError(f"{name}: txid probe divergent")
        response = await client.submit_transaction(
            {"transaction": tx, "allowOrphan": False}
        )
        if response["transactionId"] != item["id"]:
            raise RuntimeError(f"{name}: txid RPC divergent")
        await live.genesis.wait_utxo_resilient(
            client,
            live.genesis.address_from_script_public_key(
                tx.outputs[-1].script_public_key, NETWORK_TYPE
            ).to_string(),
            item["id"],
            len(tx.outputs) - 1,
        )
        if name == "multiwallet_anchor":
            confirmed = await live.genesis.wait_utxo_resilient(
                client,
                live.genesis.address_from_script_public_key(
                    tx.outputs[0].script_public_key, NETWORK_TYPE
                ).to_string(),
                item["id"],
                0,
            )
            challenge_daa = int(confirmed["utxoEntry"]["blockDaaScore"])
        if name == "multiwallet_activate":
            confirmed = await live.genesis.wait_utxo_resilient(
                client,
                live.genesis.address_from_script_public_key(
                    tx.outputs[0].script_public_key, NETWORK_TYPE
                ).to_string(),
                item["id"],
                0,
            )
            auction_daa = int(confirmed["utxoEntry"]["blockDaaScore"])
        completed.append(name)
        save(
            plan, completed, f"{name}_confirmed", challenge_daa, auction_daa
        )
        print(json.dumps({"confirmed": name, "transaction_id": item["id"]}, indent=2))
    plan["live"] = {
        name: await live.genesis.wait_utxo_resilient(
            client,
            value["address"],
            value["outpoint"]["transactionId"],
            value["outpoint"]["index"],
            timeout=60,
        )
        for name, value in plan["live"].items()
    }
    plan["roles_distinct"] = len(
        {plan["owner"], plan["challenger"], plan["bidder"]}
    ) == 3
    plan["stage"] = "multiwallet_settlement_validated"
    STATE.write_text(json.dumps(plan, indent=2))
    save(plan, completed, plan["stage"], challenge_daa, auction_daa)


async def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("command", choices=["dry-run", "deploy"])
    args = parser.parse_args()
    url, owner_key = load_env()
    client = RpcClient(
        resolver=None, url=None if url == "public" else url, network_id=NETWORK_ID
    )
    await client.connect()
    try:
        if args.command == "deploy" and STATE.exists():
            raise SystemExit(f"{STATE} already exists; refusing rebroadcast")
        if args.command == "deploy" and PROGRESS.exists():
            progress = json.loads(PROGRESS.read_text())
            plan = progress["plan"] if progress.get("completed") else await build(client, owner_key)
        else:
            plan = await build(client, owner_key)
        print(
            json.dumps(
                {
                    "dry_run": args.command == "dry-run",
                    "position_id": plan["position_id"],
                    "challenge_id": plan["challenge_id"],
                    "checked_inputs": plan["checked_inputs"],
                    "transactions": plan["transactions"],
                },
                indent=2,
            )
        )
        if args.command == "deploy":
            await submit(client, plan)
    finally:
        await client.disconnect()


if __name__ == "__main__":
    asyncio.run(main())
