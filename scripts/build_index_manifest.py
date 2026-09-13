"""Build an indexer manifest from a fully validated KUSD deployment."""
import json
import os
from pathlib import Path

TAG = os.environ.get("KUSD_TAG", "kusd")
PROTOCOL = "KUSD"
DEPLOYMENT = Path(f"{TAG}-deployment.local.json")
REMAINING = Path(f"{TAG}-remaining-validation.local.json")
SAVINGS = Path(f"{TAG}-live-validation.local.json")
CHALLENGE = Path(f"{TAG}-challenge-validation.local.json")
MULTIWALLET = Path(f"{TAG}-multiwallet-validation.local.json")
OUTPUT = Path(f"{TAG}-index-manifest.local.json")

DEBT0 = 1_500_000_000
ASSIGNED0 = 150_000_000
FEE0 = 30_000_000
CHANGE0 = 320_000_000
PROPOSAL_FEE = 100_000_000
CHANGE_AFTER_PROPOSAL = CHANGE0 - PROPOSAL_FEE
RESERVE_AFTER_PROPOSAL = 1_000_000_000
DEBT1 = 800_000_000
ASSIGNED1 = 80_000_000
FEE1 = 16_000_000
USABLE1 = 704_000_000
REWARD1 = 8_000_000
COLLATERAL = 100_000_000_000


def op(txid, index):
    return {"transaction_id": txid, "index": index}


def made(txid, index, entity):
    return {"outpoint": op(txid, index), "entity": entity}


def event(txid, consumed=(), created=(), action=None):
    value = {"transaction_id": txid, "consumed": list(consumed), "created": list(created)}
    if action:
        value["governance_action"] = action
    return value


def token(amount, minter=False):
    return {"kind": "token", "amount": amount, "is_minter": minter}


def kps(amount, minter=False):
    return {"kind": "equity_token", "amount": amount, "is_minter": minter}


def module(remaining, nonce, expiration):
    return {"kind": "module", "remaining_mint": remaining,
            "position_nonce": nonce, "expiration_daa": expiration}


def position(debt=DEBT0):
    return {"kind": "position", "collateral_sompi": COLLATERAL,
            "debt": debt, "challenge_id": None}


def challenged(debt, challenge_id):
    return {"kind": "challenged_position", "collateral_sompi": COLLATERAL,
            "debt": debt, "challenge_id": challenge_id}


def reserve(balance, collateral=0):
    return {"kind": "reserve", "kusd_balance": balance,
            "total_kps": 1_000_000_000, "collateral_sompi": collateral}


def controller(total, enabled=True, account_nonce=0):
    return {"kind": "savings_controller", "enabled": enabled,
            "series_nonce": 1, "account_nonce": account_nonce,
            "total_saved": total, "current_rate_ppm": 20_000,
            "interest_delay_daa": 50, "max_accrual_daa": 10_000}


def account(owner, saved, delay, remaining):
    return {"kind": "savings_account", "owner": owner, "saved": saved,
            "rate_ppm": 20_000, "delay_remaining_daa": delay,
            "remaining_accrual_daa": remaining, "referral_fee_ppm": 100_000}


def tracked(utxo, entity):
    return {"address": utxo["address"],
            "outpoint": op(utxo["outpoint"]["transactionId"], utxo["outpoint"]["index"]),
            "covenant_id": utxo["utxoEntry"]["covenantId"], "entity": entity}


def main():
    d = json.loads(DEPLOYMENT.read_text())
    r = json.loads(REMAINING.read_text()) if REMAINING.exists() else None
    s = json.loads(SAVINGS.read_text())
    c = json.loads(CHALLENGE.read_text())
    m = json.loads(MULTIWALLET.read_text()) if MULTIWALLET.exists() else None
    if (d.get("protocol"), d.get("stage"), s.get("stage"), c.get("stage")) != (
        PROTOCOL, "savings_ready", "savings_validated", "challenge_backstop_validated"
    ):
        raise SystemExit(f"incomplete {PROTOCOL} deployment state")
    if r is not None and r.get("stage") != "repay_close_validated":
        raise SystemExit(f"incomplete repay/close state in {REMAINING}")
    if m is not None and m.get("stage") != "multiwallet_settlement_validated":
        raise SystemExit(f"incomplete multi-wallet state in {MULTIWALLET}")
    dt = {x["name"]: x["id"] for x in d["transactions"]}
    rt = {x["name"]: x["id"] for x in r["transactions"]} if r else {}
    st = {x["name"]: x["id"] for x in s["transactions"]}
    ct = {x["name"]: x["id"] for x in c["transactions"]}
    mt = {x["name"]: x["id"] for x in m["transactions"]} if m else {}
    expiration = d["expiration_daa"]
    owner = d["owner"]
    account_id, challenge_id = s["account_id"], c["challenge_id"]
    net, referral = s["net_interest"], s["referral_interest"]
    remaining_fee = int(r.get("risk_fee_kusd", FEE1)) if r else FEE1
    remaining_usable = int(r.get("usable_kusd", USABLE1)) if r else USABLE1
    challenge_fee = int(c.get("risk_fee_kusd", FEE1))
    challenge_usable = int(c.get("usable_kusd", USABLE1))
    multiwallet_fee = int(m.get("risk_fee_kusd", FEE1)) if m else FEE1
    multiwallet_usable = int(m.get("usable_kusd", USABLE1)) if m else USABLE1
    reserve_after_savings = int(s.get(
        "reserve_after_kusd", RESERVE_AFTER_PROPOSAL - s["gross_interest"]
    ))
    saved = (CHANGE_AFTER_PROPOSAL if r is None else int(
        r.get("remainder_kusd", CHANGE_AFTER_PROPOSAL + remaining_usable - (DEBT1 - ASSIGNED1))
    ))

    bootstrap_open, split = dt["bootstrap_open"], dt["split_reserve_deposit"]
    reserve_init, execute_module = dt["reserve_initialize"], dt["execute_governed_module"]
    split_proposal_fee = dt["split_proposal_fee"]
    propose_module = dt["propose_governed_module"]
    governance_genesis = dt["governance_genesis"]
    root_handover = dt["root_handover"]
    activate_module = dt["activate_governed_module"]
    registry_init, execute_savings = dt["savings_registry_initialize"], dt["execute_savings"]
    open_s, refresh_s, withdraw_s = st["savings_open"], st["savings_refresh"], st["savings_withdraw"]
    open_c, start, anchor, activate, backstop = (
        ct["challenge_open"], ct["challenge_start"], ct["challenge_anchor"],
        ct["challenge_activate"], ct["challenge_backstop"]
    )

    gov = {"kind": "governance", "proposal_nonce": 1, "execution_nonce": 1,
           "active_proposal_id": None, "voting_delay_daa": 100,
           "execution_window_daa": 10_000, "veto_threshold_ppm": 20_000,
           "proposal_deposit_sompi": 100_000_000,
           "proposal_fee_kusd": PROPOSAL_FEE, "daa_per_year": 31_536_000,
           "economic_bounds": {
               "min_module_allocation": 100_000_000,
               "max_module_allocation": 1_000_000_000_000,
               "max_debt_per_position": 200_000_000_000,
               "min_collateral_sompi": 100_000_000,
               "max_collateral_sompi": 100_000_000_000_000,
               "min_module_duration_daa": 100,
               "max_module_duration_daa": 126_144_000,
               "min_challenge_period_daa": 10,
               "max_challenge_period_daa": 2_592_000,
               "min_auction_duration_daa": 10,
               "max_auction_duration_daa": 2_592_000,
               "max_risk_premium_ppm": 200_000,
               "min_reserve_contribution_ppm": 10_000,
               "max_reserve_contribution_ppm": 500_000,
           }}
    gov_initial = {**gov, "proposal_nonce": 0, "execution_nonce": 0}
    gov_pending = {**gov, "execution_nonce": 0,
                   "active_proposal_id": d["proposal_id"]}
    proposal = {
        "kind": "module_proposal", "governance_id": d["governance_id"],
        "proposer": owner, "proposal_nonce": 1, "activated": False,
        "allocation": 50_000_000_000,
        "max_debt_per_position": 2_000_000_000,
        "minimum_collateral_sompi": 100_000_000,
        "liquidation_price": 3_500_000, "expiration_daa": expiration,
        "challenge_period_daa": 3_600, "auction_duration_daa": 3_600,
        "challenge_reward_ppm": 10_000,
        "reserve_contribution_ppm": 100_000,
        "risk_premium_ppm": 20_000, "daa_per_year": 31_536_000,
        "deposit_sompi": 100_000_000,
    }
    history = [
        event(bootstrap_open, created=[
            made(bootstrap_open, 0, token(0, True)), made(bootstrap_open, 1, token(0, True)),
            made(bootstrap_open, 2, token(1_320_000_000)), made(bootstrap_open, 3, token(ASSIGNED0)),
            made(bootstrap_open, 4, token(FEE0)),
            made(bootstrap_open, 5, module(98_500_000_000, 1, expiration)),
            made(bootstrap_open, 6, position()),
        ]),
        event(split, consumed=[op(bootstrap_open, 2)], created=[
            made(split, 0, token(1_000_000_000)), made(split, 1, token(CHANGE0))]),
        event(reserve_init, consumed=[op(split, 0)], created=[
            made(reserve_init, 0, reserve(1_000_000_000)),
            made(reserve_init, 1, token(1_000_000_000)),
            made(reserve_init, 2, kps(0, True)), made(reserve_init, 3, kps(1_000_000_000))]),
        event(split_proposal_fee, consumed=[op(split, 1)], created=[
            made(split_proposal_fee, 0, token(PROPOSAL_FEE)),
            made(split_proposal_fee, 1, token(CHANGE_AFTER_PROPOSAL))]),
        event(governance_genesis, created=[made(governance_genesis, 0, gov_initial)]),
        event(root_handover, created=[made(root_handover, 0, {
            "kind": "root", "remaining_allocation": 900_000_000_000,
            "module_nonce": 1,
        })]),
        event(propose_module,
              consumed=[op(governance_genesis, 0), op(split_proposal_fee, 0)],
              created=[
                  made(propose_module, 0, gov_pending),
                  made(propose_module, 1, proposal),
                  made(propose_module, 2, token(PROPOSAL_FEE)),
              ], action="propose_module"),
        event(activate_module, consumed=[op(propose_module, 1)], created=[
            made(activate_module, 0, {**proposal, "activated": True})
        ], action="activate_proposal"),
        event(execute_module,
              consumed=[op(propose_module, 0), op(activate_module, 0),
                        op(root_handover, 0), op(propose_module, 2)], created=[
            made(execute_module, 0, gov),
            made(execute_module, 1, {"kind": "root", "remaining_allocation": 850_000_000_000, "module_nonce": 2}),
            made(execute_module, 2, token(0, True)), made(execute_module, 3, token(0, True)),
            made(execute_module, 4, module(50_000_000_000, 0, expiration)),
            made(execute_module, 6, token(PROPOSAL_FEE))],
              action="execute_module"),
        event(registry_init, created=[made(registry_init, 0, {
            "kind": "savings_registry", "controller_id": d["savings_controller_id"], "initialized": True})]),
        event(execute_savings, created=[
            made(execute_savings, 0, {"kind": "savings_governor", "proposal_nonce": 1,
                                      "execution_nonce": 1, "active_proposal_id": None}),
            made(execute_savings, 1, controller(0)),
        ]),
    ]

    module_tx, module_remaining, module_nonce = bootstrap_open, 98_500_000_000, 1
    savings_token_tx, savings_token_index = split_proposal_fee, 1
    remaining_final = []
    if r is not None:
        ropen, rsplit, repay, close = (
            rt["remaining_open"], rt["remaining_split"],
            rt["remaining_repay"], rt["remaining_close"],
        )
        history.extend([
            event(ropen, consumed=[op(bootstrap_open, 0), op(bootstrap_open, 5)], created=[
                made(ropen, 0, token(0, True)), made(ropen, 1, token(0, True)),
                made(ropen, 2, token(remaining_usable)), made(ropen, 3, token(ASSIGNED1)),
                made(ropen, 4, token(remaining_fee)),
                made(ropen, 5, module(97_700_000_000, 2, expiration)),
                made(ropen, 6, position(DEBT1)),
            ]),
            event(rsplit, consumed=[op(split_proposal_fee, 1), op(ropen, 2)], created=[
                made(rsplit, 0, token(DEBT1 - ASSIGNED1)),
                made(rsplit, 1, token(saved)),
            ]),
            event(repay, consumed=[op(ropen, 6), op(ropen, 1),
                                   op(rsplit, 0), op(ropen, 3)], created=[
                made(repay, 0, token(0, True)), made(repay, 1, position(0)),
            ]),
            event(close, consumed=[op(repay, 0), op(repay, 1)]),
        ])
        module_tx = ropen
        module_remaining = int(r.get("module_remaining_after", 97_700_000_000))
        module_nonce = int(r.get("position_nonce_after", 2))
        savings_token_tx, savings_token_index = rsplit, 1
        remaining_final.append((r["live"]["fee_kusd"], token(remaining_fee)))

    history.extend([
        event(open_s, consumed=[op(execute_savings, 1),
                               op(savings_token_tx, savings_token_index)], created=[
            made(open_s, 0, controller(saved, account_nonce=1)),
            made(open_s, 1, account(owner, saved, 50, 10_000)),
            made(open_s, 2, token(saved))], action="open_savings_account"),
        event(refresh_s, consumed=[op(registry_init, 0), op(open_s, 0), op(open_s, 1),
                                   op(reserve_init, 0), op(open_s, 2), op(reserve_init, 1)], created=[
            made(refresh_s, 0, {"kind": "savings_registry", "controller_id": d["savings_controller_id"], "initialized": True}),
            made(refresh_s, 1, controller(saved + net, account_nonce=1)),
            made(refresh_s, 2, account(owner, saved + net, 0, 9_000)),
            made(refresh_s, 3, reserve(reserve_after_savings)),
            made(refresh_s, 4, token(saved + net)),
            made(refresh_s, 5, token(reserve_after_savings)),
            made(refresh_s, 6, token(referral)),
        ], action="refresh_savings_account"),
        event(withdraw_s, consumed=[op(refresh_s, 1), op(refresh_s, 2), op(refresh_s, 4)], created=[
            made(withdraw_s, 0, controller(0, account_nonce=1)),
            made(withdraw_s, 1, token(saved + net))], action="close_savings_account"),
        event(open_c, consumed=[op(module_tx, 0), op(module_tx, 5)], created=[
            made(open_c, 0, token(0, True)), made(open_c, 1, token(0, True)),
            made(open_c, 2, token(challenge_usable)), made(open_c, 3, token(ASSIGNED1)),
            made(open_c, 4, token(challenge_fee)),
            made(open_c, 5, module(module_remaining - DEBT1, module_nonce + 1, expiration)),
            made(open_c, 6, position(DEBT1))]),
        event(start, consumed=[op(open_c, 6)], created=[
            made(start, 0, challenged(DEBT1, challenge_id)),
            made(start, 1, {"kind": "challenge_anchor", "collateral_sompi": COLLATERAL, "debt": DEBT1})]),
        event(anchor, consumed=[op(start, 1)], created=[
            made(anchor, 0, {"kind": "challenge", "collateral_sompi": COLLATERAL, "debt": DEBT1})]),
        event(activate, consumed=[op(anchor, 0)], created=[
            made(activate, 0, {"kind": "auction", "collateral_sompi": COLLATERAL,
                               "debt": DEBT1, "reward": c["reward"]})]),
        event(backstop, consumed=[op(activate, 0), op(start, 0), op(open_c, 1), op(open_c, 3),
                                  op(refresh_s, 3), op(refresh_s, 5)], created=[
            made(backstop, 0, token(c["reserve_after"])), made(backstop, 1, token(c["reward"])),
            made(backstop, 2, reserve(c["reserve_after"], COLLATERAL))]),
    ])

    if m is not None:
        fund, payment, mopen, mstart, manchor, mactivate, msettle = (
            mt["multiwallet_fund"], mt["multiwallet_payment"],
            mt["multiwallet_open"], mt["multiwallet_start"],
            mt["multiwallet_anchor"], mt["multiwallet_activate"],
            mt["multiwallet_settle"],
        )
        history.extend([
            event(fund),
            event(payment, consumed=[op(open_c, 2), op(backstop, 1), op(withdraw_s, 1)],
                  created=[made(payment, 0, token(728_000_000)),
                           made(payment, 1, token(m.get("remainder_kusd", 288_000_173)))]),
            event(mopen, consumed=[op(open_c, 0), op(open_c, 5)], created=[
                made(mopen, 0, token(0, True)), made(mopen, 1, token(0, True)),
                made(mopen, 2, token(multiwallet_usable)), made(mopen, 3, token(ASSIGNED1)),
                made(mopen, 4, token(multiwallet_fee)),
                made(mopen, 5, module(
                    int(m.get("module_remaining_after", 96_100_000_000)),
                    int(m.get("position_nonce_after", 4)), expiration)),
                made(mopen, 6, position(DEBT1)),
            ]),
            event(mstart, consumed=[op(mopen, 6)], created=[
                made(mstart, 0, challenged(DEBT1, m["challenge_id"])),
                made(mstart, 1, {"kind": "challenge_anchor",
                                  "collateral_sompi": COLLATERAL, "debt": DEBT1}),
            ]),
            event(manchor, consumed=[op(mstart, 1)], created=[
                made(manchor, 0, {"kind": "challenge",
                                   "collateral_sompi": COLLATERAL, "debt": DEBT1}),
            ]),
            event(mactivate, consumed=[op(manchor, 0)], created=[
                made(mactivate, 0, {"kind": "auction",
                                     "collateral_sompi": COLLATERAL,
                                     "debt": DEBT1, "reward": REWARD1}),
            ]),
            event(msettle, consumed=[op(mactivate, 0), op(mstart, 0),
                                     op(mopen, 1), op(payment, 0), op(mopen, 3)],
                  created=[made(msettle, 0, token(REWARD1)),
                           made(msettle, 1, token(0))]),
        ])

    dl, sl, cl = d["live"], s["live"], c["live"]
    final = [
        (sl["root"], {"kind": "root", "remaining_allocation": 850_000_000_000, "module_nonce": 2}),
        (sl["root_minter"], token(0, True)), (sl["governance"], gov),
        (sl["governed_module"], module(50_000_000_000, 0, expiration)),
        (sl["governed_module_minter"], token(0, True)),
        (cl["module"], module(module_remaining - DEBT1, module_nonce + 1, expiration)),
        (cl["module_minter"], token(0, True)),
        (sl["position"], position()), (sl["position_minter"], token(0, True)),
        (sl["assigned_kusd"], token(ASSIGNED0)), (sl["fee_kusd"], token(FEE0)),
        (dl["proposal_fee_refund"], token(PROPOSAL_FEE)),
        (sl["kps_minter"], kps(0, True)), (sl["kps_owner"], kps(1_000_000_000)),
        (sl["savings_governor"], {"kind": "savings_governor", "proposal_nonce": 1,
                                   "execution_nonce": 1, "active_proposal_id": None}),
        (sl["savings_registry"], {"kind": "savings_registry",
                                  "controller_id": d["savings_controller_id"], "initialized": True}),
        (sl["savings_controller"], controller(0, account_nonce=1)),
        (sl["savings_payout"], token(saved + net)),
        (sl["savings_referral"], token(referral)),
        (cl["usable_kusd"], token(challenge_usable)), (cl["fee_kusd"], token(challenge_fee)),
        (cl["reserve_kusd"], token(c["reserve_after"])),
        (cl["challenge_reward"], token(c["reward"])),
        (cl["reserve"], reserve(c["reserve_after"], COLLATERAL)),
    ] + remaining_final
    if m is not None:
        spent = {
            (cl["module"]["outpoint"]["transactionId"], cl["module"]["outpoint"]["index"]),
            (cl["module_minter"]["outpoint"]["transactionId"], cl["module_minter"]["outpoint"]["index"]),
            (cl["usable_kusd"]["outpoint"]["transactionId"], cl["usable_kusd"]["outpoint"]["index"]),
            (cl["challenge_reward"]["outpoint"]["transactionId"], cl["challenge_reward"]["outpoint"]["index"]),
            (sl["savings_payout"]["outpoint"]["transactionId"], sl["savings_payout"]["outpoint"]["index"]),
        }
        final = [
            (utxo, entity) for utxo, entity in final
            if (utxo["outpoint"]["transactionId"], utxo["outpoint"]["index"]) not in spent
        ]
        ml = m["live"]
        final.extend([
            (ml["module"], module(
                int(m.get("module_remaining_after", 96_100_000_000)),
                int(m.get("position_nonce_after", 4)), expiration)),
            (ml["module_minter"], token(0, True)),
            (ml["usable_kusd"], token(multiwallet_usable)),
            (ml["fee_kusd"], token(multiwallet_fee)),
            (ml["kusd_remainder"], token(m.get("remainder_kusd", 288_000_173))),
            (ml["challenger_reward"], token(REWARD1)),
            (ml["owner_surplus"], token(0)),
        ])
    manifest = {"network": "testnet-10", "protocol": PROTOCOL,
                "history_scope": "economic genesis, repay/close, Savings and Challenge/backstop",
                "tracked": [tracked(utxo, entity) for utxo, entity in final], "history": history}
    OUTPUT.write_text(json.dumps(manifest, indent=2))
    print(json.dumps({"output": str(OUTPUT), "history": len(history),
                      "tracked": len(final), "expected_supply": 1_500_000_000,
                      "expected_debt": 1_500_000_000}, indent=2))


if __name__ == "__main__":
    main()
