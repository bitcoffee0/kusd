"""Build, preflight, deploy, and verify the complete KUSD covenant stack."""
import argparse
import asyncio
import json
import os
import subprocess
import tempfile
from pathlib import Path

from kaspa import (
    Transaction,
    GenesisCovenantGroup,
    RpcClient,
    TransactionOutput,
    address_from_script_public_key,
    create_input_signature,
    pay_to_address_script,
    sign_transaction,
)

from network import (
    CB,
    DEPOSIT,
    NETWORK_ID,
    NETWORK_TYPE,
    SUBNETWORK_ID,
    binding,
    entry,
    fee_mass_components,
    inp,
    load_env,
    make_tx,
    sized,
    spk,
    wait_utxo,
    wallet_utxos,
)

TAG = os.environ.get("KUSD_TAG", "kusd")
PROTOCOL = "KUSD"
INCLUDE_SAVINGS = True
TOOL = Path("target/debug/savings-tool")
DRY_RUN = Path(f"{TAG}-dry-run.local.json")
STATE = Path(f"{TAG}-deployment.local.json")
PROGRESS = Path(f"{TAG}-deployment-progress.local.json")
PROBE_PREFIX = f"{TAG}-probe"
ERROR_LOG = Path(f"{TAG}-deployment-last-error.local.txt")
ZERO = "00" * 32
SAVINGS_CONTROLLER_ID = ZERO
SAVINGS_GOVERNOR_ID = ZERO
SAVINGS_REGISTRY_ID = ZERO
ROOT_CAP = 1_000_000_000_000
BOOTSTRAP_CAP = 100_000_000_000
GOVERNED_CAP = 50_000_000_000
POSITION_CAP = 2_000_000_000
DEBT = 1_500_000_000
CONTRIBUTION_PPM = 100_000
FEE_PPM = 20_000
DAA_PER_YEAR = 31_536_000
ASSIGNED = DEBT * CONTRIBUTION_PPM // 1_000_000
FEE = DEBT * FEE_PPM // 1_000_000
USABLE = DEBT - ASSIGNED - FEE
RESERVE_DEPOSIT = 1_000_000_000
KUSD_CHANGE = USABLE - RESERVE_DEPOSIT
COLLATERAL = 100_000_000_000
PRICE = 3_500_000
PERIOD = 3_600
REWARD_PPM = 10_000
VOTING_DELAY = 100
EXECUTION_WINDOW = 10_000
VETO_THRESHOLD = 20_000
# The full genesis has several confirmed ancestors before the first
# time-sensitive covenant transition. At 10 BPS, 300 DAA was too short in a
# real public-RPC deployment even though the offline consensus preflight was
# valid. Keep enough deterministic runway for compilation, RPC latency and
# confirmations, and rebuild a stale dry-run before spending anything.
PLANNED_DAA_DELAY = int(os.environ.get("KUSD_DEPLOYMENT_DAA_BUFFER", "10000"))
MIN_DEPLOYMENT_DAA_RUNWAY = int(os.environ.get("KUSD_MIN_DEPLOYMENT_DAA_RUNWAY", "4000"))
PROPOSAL_DAA_DELAY = int(os.environ.get("KUSD_PROPOSAL_DAA_BUFFER", "3000"))


def _bytes32(value):
    return list(bytes.fromhex(value))


def _config(values):
    base = {
        "owner": _bytes32(values.get("owner", ZERO)),
        "challenger": _bytes32(values.get("challenger", ZERO)),
        "asset_id": _bytes32(values.get("asset_id", ZERO)),
        "kps_id": _bytes32(values.get("kps_id", ZERO)),
        "reserve_id": _bytes32(values.get("reserve_id", ZERO)),
        "module_id": _bytes32(values.get("module_id", ZERO)),
        "position_id": _bytes32(values.get("position_id", ZERO)),
        "debt": values.get("debt", DEBT),
        "assigned_reserve": values.get("assigned_reserve", ASSIGNED),
        "reserve_contribution_ppm": values.get("reserve_contribution_ppm", CONTRIBUTION_PPM),
        "risk_premium_ppm": values.get("risk_premium_ppm", FEE_PPM),
        "daa_per_year": values.get("daa_per_year", 31_536_000),
        "liquidation_price": values.get("liquidation_price", PRICE),
        "challenge_period_daa": values.get("challenge_period_daa", PERIOD),
        "auction_duration_daa": values.get("auction_duration_daa", PERIOD),
        "challenge_reward_ppm": values.get("challenge_reward_ppm", REWARD_PPM),
        "collateral_sompi": values.get("collateral_sompi", COLLATERAL),
        "minimum_collateral_sompi": values.get("minimum_collateral_sompi", 100_000_000),
        "current_daa": values.get("current_daa", 1),
        # Transition commands may provide both
        # current economic parameters and explicit prev/next state. The
        # state_* value is authoritative; reversing precedence previously built
        # a genesis Reserve credited before its KUSD deposit.
        "reserve_kusd": values.get("state_reserve_kusd", values.get("reserve_kusd", 0)),
        "total_kps": values.get("state_total_kps", values.get("total_kps", 0)),
        "reserve_collateral_sompi": values.get("state_collateral_sompi", values.get("reserve_collateral_sompi", 0)),
        "minimum_kps_holding_daa": values.get("minimum_kps_holding_daa", 90),
        "max_kps_vote_weight": values.get("max_kps_vote_weight", 4),
        "module_remaining_mint": values.get("module_remaining_mint", BOOTSTRAP_CAP),
        "max_debt_per_position": values.get("max_debt_per_position", POSITION_CAP),
        "module_expiration_daa": values.get("module_expiration_daa", 1),
        "position_nonce": values.get("state_position_nonce", values.get("position_nonce", 0)),
    }
    governance = {
        "proposer": _bytes32(values.get("proposer", values.get("owner", ZERO))),
        "governance_id": _bytes32(values.get("governance_id", ZERO)),
        "root_id": _bytes32(values.get("root_id", ZERO)),
        "proposal_nonce": values.get("proposal_nonce", 0),
        "execution_nonce": values.get("execution_nonce", 0),
        "voting_delay_daa": values.get("voting_delay_daa", VOTING_DELAY),
        "execution_window_daa": values.get("execution_window_daa", EXECUTION_WINDOW),
        "veto_threshold_ppm": values.get("veto_threshold_ppm", VETO_THRESHOLD),
        "proposal_deposit_sompi": values.get("proposal_deposit_sompi", DEPOSIT),
        "proposal_fee_kusd": values.get("proposal_fee_kusd", 100_000_000),
        "min_module_allocation": values.get("min_module_allocation", 100_000_000),
        "max_module_allocation": values.get("max_module_allocation", ROOT_CAP),
        "max_debt_per_position": values.get("governance_max_debt_per_position", POSITION_CAP * 100),
        "min_collateral_sompi": values.get("min_collateral_sompi", 100_000_000),
        "max_collateral_sompi": values.get("max_collateral_sompi", 100_000_000_000_000),
        "min_module_duration_daa": values.get("min_module_duration_daa", 100),
        "max_module_duration_daa": values.get("max_module_duration_daa", 126_144_000),
        "min_challenge_period_daa": values.get("min_challenge_period_daa", 10),
        "max_challenge_period_daa": values.get("max_challenge_period_daa", 2_592_000),
        "min_auction_duration_daa": values.get("min_auction_duration_daa", 10),
        "max_auction_duration_daa": values.get("max_auction_duration_daa", 2_592_000),
        "max_risk_premium_ppm": values.get("max_risk_premium_ppm", 200_000),
        "min_reserve_contribution_ppm": values.get("min_reserve_contribution_ppm", 10_000),
        "max_reserve_contribution_ppm": values.get("max_reserve_contribution_ppm", 500_000),
        "root_remaining_allocation": values.get("root_remaining_allocation", ROOT_CAP),
    }
    savings = {
        "owner": base["owner"], "referrer": base["owner"], "proposer": base["owner"],
        "controller_id": _bytes32(SAVINGS_CONTROLLER_ID), "governor_id": _bytes32(SAVINGS_GOVERNOR_ID),
        "registry_id": _bytes32(SAVINGS_REGISTRY_ID),
        "active_proposal_id": _bytes32(ZERO), "enabled": False,
        "proposal_nonce": 0, "execution_nonce": 0, "series_nonce": 0,
        "account_nonce": 0, "total_saved": 0, "saved": 1_000_000_000,
        "rate_ppm": 20_000, "current_rate_ppm": 0,
        "interest_delay_daa": 50, "delay_remaining_daa": 50,
        "max_accrual_daa": 10_000, "remaining_accrual_daa": 10_000,
        "daa_per_year": 31_536_000, "referral_fee_ppm": 100_000,
        "voting_delay_daa": 100, "execution_window_daa": 10_000,
        "veto_threshold_ppm": 200_000, "proposal_deposit_sompi": DEPOSIT,
        "account_value_sompi": DEPOSIT,
    }
    return {"base": base, "savings": savings, "governance": governance}


def full_tool(command, **values):
    if not TOOL.exists():
        subprocess.run(["cargo", "build", "--bin", TOOL.name], check=True)
    passthrough = {"kcc-state", "kcc-transfer-call", "kps-transfer-call"}
    if command in passthrough:
        old = Path("target/debug/protocol-tool")
        if not old.exists(): subprocess.run(["cargo", "build", "--bin", old.name], check=True)
        args=[str(old),command]
        for key,value in values.items(): args += ["--"+key.replace("_","-"),str(value).lower() if isinstance(value,bool) else str(value)]
        raw=subprocess.check_output(args,text=True).strip()
        return json.loads(raw) if raw.startswith("{") else raw
    mapping = {
        "root-state": "base-root-state", "module-state": "base-module-state",
        "position-state": "base-position-state", "reserve-state": "reserve-state",
        "kps-state": "base-kps-state", "governance-state": "base-governance-state",
        "proposal-state": "base-proposal-state", "root-init-call": "base-root-init-call",
        "root-bootstrap-module-call": "base-root-bootstrap-call",
        "module-open-call": "base-module-open-call",
        "reserve-initialize-call": "base-reserve-initialize-call",
        "propose-module-call": "base-governance-propose-call",
        "reserve-collect-proposal-fee-call": "base-reserve-collect-proposal-fee-call",
        "activate-proposal-call": "base-proposal-activate-call",
        "governance-finish-call": "base-governance-execute-call",
        "proposal-finish-call": "base-proposal-execute-call",
        "root-handover-call": "base-root-handover-call",
        "root-execute-module-call": "base-root-execute-call",
    }
    selected = {
        "root-state": {"initialized":"initialized","module_nonce":"module_nonce","remaining_allocation":"remaining_allocation","authority":"authority","authority_type":"authority_type"},
        "module-state": {"remaining_mint":"remaining_mint","state_position_nonce":"position_nonce"},
        "position-state": {"state_debt":"debt","state_assigned_reserve":"assigned_reserve","challenge_id":"challenge_id"},
        "reserve-state": {},
        "kps-state": {"token_owner":"owner","amount":"amount","identifier":"identifier","minter":"minter"},
        "governance-state": {"active_proposal_id":"active_proposal_id"},
        "proposal-state": {"activated":"activated"},
        "root-init-call": {"current_authority":"current_authority","next_asset_id":"next_asset_id","root_allocation":"root_allocation"},
        "root-bootstrap-module-call": {"current_authority":"current_authority","authority_signature":"signature","root_id":"root_id","output_module_id":"module_id","previous_module_nonce":"previous_nonce","previous_remaining_allocation":"previous_remaining","module_output":"module_output"},
        "module-open-call": {"output_position_id":"position_id","position_output":"position_output"},
        "reserve-initialize-call": {"output_reserve_id":"reserve_id","depositor":"depositor","deposit_amount":"deposit_amount"},
        "propose-module-call": {"proposal_id":"proposal_id","proposal_output":"proposal_output"},
        "reserve-collect-proposal-fee-call": {"proposal_id":"proposal_id","fee_amount":"fee_amount"},
        "activate-proposal-call": {},
        "governance-finish-call": {"active_proposal_id":"active_proposal_id"},
        "proposal-finish-call": {"refund_output":"refund_output","fee_input":"fee_input","fee_output":"fee_output"},
        "root-handover-call": {"current_authority":"current_authority","authority_signature":"signature","module_nonce":"module_nonce","remaining_allocation":"remaining"},
        "root-execute-module-call": {"root_id":"root_id","module_id":"module_id","previous_module_nonce":"previous_nonce","previous_remaining_allocation":"previous_remaining","module_output":"module_output"},
    }[command]
    config=_config(values)
    with tempfile.NamedTemporaryFile("w",suffix=".json",delete=False) as handle:
        json.dump(config,handle); path=handle.name
    try:
        args=[str(TOOL),mapping[command],"--config",path]
        for old,new in selected.items():
            value=values[old]; args += ["--"+new.replace("_","-"),str(value).lower() if isinstance(value,bool) else str(value)]
        raw=subprocess.check_output(args,text=True).strip()
        return json.loads(raw) if raw.startswith("{") else raw
    finally:
        Path(path).unlink(missing_ok=True)


tool = full_tool
_CONFIG_CACHE = {}


def artifact(command, **values):
    value = tool(command, **values)
    for field in ("bytecode", "prefix", "suffix", "template_hash"):
        if isinstance(value.get(field), list):
            value[field] = bytes(value[field]).hex()
    return value


def config_tool(command, config_value, **values):
    cache_key = json.dumps(
        [command, config_value, values], sort_keys=True, separators=(",", ":")
    )
    cached = _CONFIG_CACHE.get(cache_key)
    if cached is not None:
        return json.loads(cached)
    binary = Path("target/debug/savings-tool")
    if not binary.exists():
        subprocess.run(["cargo", "build", "--bin", binary.name], check=True)
    with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as handle:
        json.dump(config_value, handle)
        path = handle.name
    extra_paths = []
    try:
        args = [str(binary), command, "--config", path]
        for key, value in values.items():
            if isinstance(value, dict):
                with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as extra:
                    json.dump(value, extra)
                    value = extra.name
                    extra_paths.append(value)
            args += ["--" + key.replace("_", "-"), str(value).lower() if isinstance(value, bool) else str(value)]
        completed = subprocess.run(args, text=True, capture_output=True)
        if completed.returncode != 0:
            details = (completed.stderr or completed.stdout).strip()
            raise RuntimeError(
                f"{command}: builder failed ({completed.returncode}): {details}"
            )
        raw = completed.stdout.strip()
        parsed = json.loads(raw) if raw.startswith("{") else raw
        if isinstance(parsed, dict):
            for field in ("bytecode", "prefix", "suffix", "template_hash"):
                if isinstance(parsed.get(field), list): parsed[field] = bytes(parsed[field]).hex()
        _CONFIG_CACHE[cache_key] = json.dumps(parsed)
        return json.loads(_CONFIG_CACHE[cache_key])
    finally:
        Path(path).unlink(missing_ok=True)
        for extra_path in extra_paths:
            Path(extra_path).unlink(missing_ok=True)


def economic(owner, asset, kps, reserve, module=ZERO, position=ZERO,
             remaining=BOOTSTRAP_CAP, nonce=0, expiration=1, current_daa=None):
    if current_daa is None:
        current_daa = max(1, expiration - DAA_PER_YEAR)
    return {
        "owner": owner, "challenger": ZERO, "asset_id": asset, "kps_id": kps,
        "reserve_id": reserve, "module_id": module, "position_id": position,
        "debt": DEBT, "assigned_reserve": ASSIGNED,
        "reserve_contribution_ppm": CONTRIBUTION_PPM, "risk_premium_ppm": FEE_PPM,
        "daa_per_year": DAA_PER_YEAR,
        "liquidation_price": PRICE, "challenge_period_daa": PERIOD,
        "auction_duration_daa": PERIOD,
        "challenge_reward_ppm": REWARD_PPM, "collateral_sompi": COLLATERAL,
        "minimum_collateral_sompi": 100_000_000, "current_daa": current_daa,
        "reserve_kusd": RESERVE_DEPOSIT, "total_kps": RESERVE_DEPOSIT,
        "reserve_collateral_sompi": 0, "minimum_kps_holding_daa": 90,
        "max_kps_vote_weight": 4, "module_remaining_mint": remaining,
        "max_debt_per_position": POSITION_CAP, "module_expiration_daa": expiration,
        "position_nonce": nonce,
    }


def governance(owner, governance_id, root_id, proposal_nonce=0, execution_nonce=0, remaining=ROOT_CAP - BOOTSTRAP_CAP):
    return {
        "proposer": owner, "governance_id": governance_id, "root_id": root_id,
        "proposal_nonce": proposal_nonce, "execution_nonce": execution_nonce,
        "voting_delay_daa": VOTING_DELAY, "execution_window_daa": EXECUTION_WINDOW,
        "veto_threshold_ppm": VETO_THRESHOLD, "proposal_deposit_sompi": DEPOSIT,
        "proposal_fee_kusd": 100_000_000,
        "min_module_allocation": 100_000_000, "max_module_allocation": ROOT_CAP,
        "governance_max_debt_per_position": POSITION_CAP * 100,
        "min_collateral_sompi": 100_000_000, "max_collateral_sompi": 100_000_000_000_000,
        "min_module_duration_daa": 100, "max_module_duration_daa": 126_144_000,
        "min_challenge_period_daa": 10, "max_challenge_period_daa": 2_592_000,
        "min_auction_duration_daa": 10, "max_auction_duration_daa": 2_592_000,
        "max_risk_premium_ppm": 200_000,
        "min_reserve_contribution_ppm": 10_000, "max_reserve_contribution_ppm": 500_000,
        "root_remaining_allocation": remaining,
    }


def metrics(tx, fee):
    return {"id": str(tx.id), "fee_sompi": fee, **fee_mass_components(tx)}


def expected(tx, specs):
    txid = str(tx.id)
    return [entry(txid, index, value, script, covenant_id) for index, value, script, covenant_id in specs]


def validate_probe_consensus(probes):
    """Execute every input in the local consensus VM before deployment."""
    diagnose = Path("target/debug/tx-diagnose")
    if not diagnose.exists():
        subprocess.run(["cargo", "build", "--bin", "tx-diagnose"], check=True)
    checked_inputs = 0
    for name, _ in probes:
        path = Path(f"{PROBE_PREFIX}-{name}.local.json")
        completed = subprocess.run(
            [str(diagnose), "--summary", str(path)],
            check=True, capture_output=True, text=True,
        )
        statuses = [line for line in completed.stdout.splitlines() if line.startswith("input ")]
        failures = [line for line in statuses if not line.endswith(": OK")]
        if not statuses or failures:
            detail = ", ".join(failures) if failures else "no input executed"
            raise RuntimeError(f"{name}: local consensus preflight failed: {detail}")
        checked_inputs += len(statuses)
    return checked_inputs


async def dry_run(client, key):
    global SAVINGS_CONTROLLER_ID, SAVINGS_GOVERNOR_ID, SAVINGS_REGISTRY_ID
    if PLANNED_DAA_DELAY <= MIN_DEPLOYMENT_DAA_RUNWAY:
        raise RuntimeError(
            "KUSD_DEPLOYMENT_DAA_BUFFER must be greater than "
            f"KUSD_MIN_DEPLOYMENT_DAA_RUNWAY ({MIN_DEPLOYMENT_DAA_RUNWAY})"
        )
    address = key.to_public_key().to_address(NETWORK_TYPE)
    owner = key.to_public_key().to_x_only_public_key().to_string()
    owner_spk = pay_to_address_script(address)
    funding = (await wallet_utxos(client, address))[0]
    daa = await current_daa_resilient(client)
    # Time-sensitive descendants are prebuilt against a future DAA. Deployment
    # waits for it before submission, ensuring every parent is confirmed while
    # retaining deterministic transaction IDs across the complete dry-run.
    operation_daa = daa + PLANNED_DAA_DELAY
    proposal_operation_daa = operation_daa + PROPOSAL_DAA_DELAY
    expiration = operation_daa + DAA_PER_YEAR
    result = {"network": NETWORK_ID, "protocol": PROTOCOL, "dry_run": True, "owner": owner,
              "virtual_daa_score": daa, "operation_daa": operation_daa,
              "proposal_operation_daa": proposal_operation_daa,
              "expiration_daa": expiration, "transactions": []}
    probes = []
    def record(name, tx, fee, **extra):
        result["transactions"].append({"name": name, **extra, **metrics(tx, fee)})
        probes.append((name, tx.to_dict()))

    if INCLUDE_SAVINGS:
        registry_cfg = _config({**economic(owner,ZERO,ZERO,ZERO,expiration=expiration),
                                    **governance(owner,ZERO,ZERO,remaining=ROOT_CAP)})
        registry0 = config_tool("registry-state", registry_cfg, initialized=False)
        def build_registry(change,mass):
            tx=make_tx([inp(funding)],[TransactionOutput(DEPOSIT,spk(registry0)),TransactionOutput(change,owner_spk)],mass)
            tx.populate_genesis_covenants([GenesisCovenantGroup(0,[0])]); return sign_transaction(tx,[key],True)
        tx,fee=await sized(client,build_registry,funding["utxoEntry"]["amount"],DEPOSIT)
        registry_id=tx.outputs[0].to_dict()["covenant"]["covenantId"]
        SAVINGS_REGISTRY_ID=registry_id
        registry_entry,wallet=expected(tx,[(0,DEPOSIT,spk(registry0),registry_id),(1,tx.outputs[1].value,owner_spk,None)])
        funding=wallet
        result["savings_registry_id"]=registry_id; record("savings_registry_genesis",tx,fee)

    # 1. Root genesis. Future IDs are state fields and do not change
    # the identity of templates referenced by Root.
    e0 = economic(owner, ZERO, ZERO, ZERO, expiration=expiration)
    g0 = governance(owner, ZERO, ZERO, remaining=ROOT_CAP)
    root0 = artifact("root-state", **e0, **g0, initialized=False, module_nonce=0,
                     remaining_allocation=0, authority=owner, authority_type=0)
    def build_root(change, mass):
        tx = make_tx([inp(funding)], [TransactionOutput(DEPOSIT, spk(root0)), TransactionOutput(change, owner_spk)], mass)
        tx.populate_genesis_covenants([GenesisCovenantGroup(0, [0])])
        return sign_transaction(tx, [key], True)
    tx, fee = await sized(client, build_root, funding["utxoEntry"]["amount"], DEPOSIT)
    root_id = tx.outputs[0].to_dict()["covenant"]["covenantId"]
    root, wallet = expected(tx, [(0, DEPOSIT, spk(root0), root_id), (1, tx.outputs[1].value, owner_spk, None)])
    result["root_id"] = root_id; record("root_genesis", tx, fee)

    # 2. Asset ID + initialized Root.
    root_minter = artifact("kcc-state", owner=root_id, amount=0, identifier=2, minter=True)
    probe = make_tx([inp(root), inp(wallet)], [TransactionOutput(DEPOSIT, spk(root_minter))])
    probe.populate_genesis_covenants([GenesisCovenantGroup(1, [0])])
    asset_id = probe.outputs[0].to_dict()["covenant"]["covenantId"]
    e = economic(owner, asset_id, ZERO, ZERO, expiration=expiration)
    g = governance(owner, ZERO, root_id, remaining=ROOT_CAP)
    root1 = artifact("root-state", **e, **g, initialized=True, module_nonce=0,
                     remaining_allocation=ROOT_CAP, authority=owner, authority_type=0)
    init_call = bytes.fromhex(tool("root-init-call", **e, **g, current_authority=owner,
                                   next_asset_id=asset_id, root_allocation=ROOT_CAP))
    def build_asset(change, mass):
        tx = make_tx([inp(root, init_call), inp(wallet)], [
            TransactionOutput(DEPOSIT, spk(root_minter)),
            TransactionOutput(DEPOSIT, spk(root1), binding(0, root_id)),
            TransactionOutput(change, owner_spk)], mass)
        tx.populate_genesis_covenants([GenesisCovenantGroup(1, [0])])
        tx.inputs[1].signature_script = bytes.fromhex(create_input_signature(tx, 1, key))
        return tx
    tx, fee = await sized(client, build_asset, DEPOSIT + wallet["utxoEntry"]["amount"], 2 * DEPOSIT)
    root_minter_entry, root, wallet = expected(tx, [
        (0, DEPOSIT, spk(root_minter), asset_id), (1, DEPOSIT, spk(root1), root_id),
        (2, tx.outputs[2].value, owner_spk, None)])
    result["asset_id"] = asset_id; record("asset_init", tx, fee)

    # 3. Reserve and KPS genesis before the bootstrap Module.
    reserve0 = artifact("reserve-state", **economic(owner, asset_id, ZERO, ZERO, expiration=expiration),
                        state_reserve_kusd=0, state_total_kps=0, state_collateral_sompi=0)
    def build_reserve(change, mass):
        tx = make_tx([inp(wallet)], [TransactionOutput(DEPOSIT, spk(reserve0)), TransactionOutput(change, owner_spk)], mass)
        tx.populate_genesis_covenants([GenesisCovenantGroup(0, [0])]); return sign_transaction(tx, [key], True)
    tx, fee = await sized(client, build_reserve, wallet["utxoEntry"]["amount"], DEPOSIT)
    reserve_id = tx.outputs[0].to_dict()["covenant"]["covenantId"]
    reserve, wallet = expected(tx, [(0, DEPOSIT, spk(reserve0), reserve_id), (1, tx.outputs[1].value, owner_spk, None)])
    result["reserve_id"] = reserve_id; record("reserve_genesis", tx, fee)

    if INCLUDE_SAVINGS:
        base_cfg=_config({**economic(owner,asset_id,ZERO,reserve_id,expiration=expiration),**g})
        controller_genesis_cfg=json.loads(json.dumps(base_cfg))
        controller_genesis_cfg["savings"]["governor_id"]=[0]*32
        controller0=config_tool("controller-state",controller_genesis_cfg)
        def build_controller_genesis(change,mass):
            tx=make_tx([inp(wallet)],[TransactionOutput(DEPOSIT,spk(controller0)),TransactionOutput(change,owner_spk)],mass)
            tx.populate_genesis_covenants([GenesisCovenantGroup(0,[0])]); return sign_transaction(tx,[key],True)
        tx,fee=await sized(client,build_controller_genesis,wallet["utxoEntry"]["amount"],DEPOSIT)
        controller_id=tx.outputs[0].to_dict()["covenant"]["covenantId"]
        SAVINGS_CONTROLLER_ID=controller_id
        controller_entry,wallet=expected(tx,[(0,DEPOSIT,spk(controller0),controller_id),(1,tx.outputs[1].value,owner_spk,None)])
        result["savings_controller_id"]=controller_id; record("savings_controller_genesis",tx,fee)

    ek0 = economic(owner, asset_id, ZERO, reserve_id, expiration=expiration)
    kps_minter0 = artifact("kps-state", **ek0, token_owner=reserve_id, amount=0, identifier=2, minter=True)
    def build_kps(change, mass):
        tx = make_tx([inp(wallet)], [TransactionOutput(DEPOSIT, spk(kps_minter0)), TransactionOutput(change, owner_spk)], mass)
        tx.populate_genesis_covenants([GenesisCovenantGroup(0, [0])]); return sign_transaction(tx, [key], True)
    tx, fee = await sized(client, build_kps, wallet["utxoEntry"]["amount"], DEPOSIT)
    kps_id = tx.outputs[0].to_dict()["covenant"]["covenantId"]
    kps_minter, wallet = expected(tx, [(0, DEPOSIT, spk(kps_minter0), kps_id), (1, tx.outputs[1].value, owner_spk, None)])
    result["kps_id"] = kps_id; record("kps_genesis", tx, fee)

    if INCLUDE_SAVINGS:
        # Governor is created only after the KPS ID is known. Its state
        # therefore never contains a ZERO sentinel requiring later replacement.
        governor_cfg=_config({**economic(owner,asset_id,kps_id,reserve_id,expiration=expiration),**g})
        governor0=config_tool("governor-state",governor_cfg)
        def build_governor_genesis(change,mass):
            tx=make_tx([inp(wallet)],[TransactionOutput(DEPOSIT,spk(governor0)),TransactionOutput(change,owner_spk)],mass)
            tx.populate_genesis_covenants([GenesisCovenantGroup(0,[0])]); return sign_transaction(tx,[key],True)
        tx,fee=await sized(client,build_governor_genesis,wallet["utxoEntry"]["amount"],DEPOSIT)
        governor_id=tx.outputs[0].to_dict()["covenant"]["covenantId"]
        SAVINGS_GOVERNOR_ID=governor_id
        governor_entry,wallet=expected(tx,[(0,DEPOSIT,spk(governor0),governor_id),(1,tx.outputs[1].value,owner_spk,None)])
        result["savings_governor_id"]=governor_id; record("savings_governor_genesis",tx,fee)

        initialized_cfg=_config({**economic(owner,asset_id,kps_id,reserve_id,expiration=expiration),**g})
        controller1=config_tool("controller-state",initialized_cfg)
        def build_controller_init(change,mass):
            outputs=[TransactionOutput(DEPOSIT,spk(controller1),binding(0,controller_id)),TransactionOutput(change,owner_spk)]
            unsigned=make_tx([inp(controller_entry),inp(wallet)],outputs,mass)
            controller_sig=create_input_signature(unsigned,0,key)[2:]
            controller_call=bytes.fromhex(config_tool("controller-initialize-call",controller_genesis_cfg,next_config=initialized_cfg,owner_signature=controller_sig))
            tx=make_tx([inp(controller_entry,controller_call),inp(wallet)],outputs,mass)
            tx.inputs[1].signature_script=bytes.fromhex(create_input_signature(tx,1,key)); return tx
        tx,fee=await sized(client,build_controller_init,DEPOSIT+wallet["utxoEntry"]["amount"],DEPOSIT)
        controller_entry,wallet=expected(tx,[(0,DEPOSIT,spk(controller1),controller_id),(1,tx.outputs[1].value,owner_spk,None)])
        record("savings_controller_initialize",tx,fee)

        registry1=config_tool("registry-state",initialized_cfg,initialized=True)
        def build_registry_init(change,mass):
            outputs=[TransactionOutput(DEPOSIT,spk(registry1),binding(0,registry_id)),TransactionOutput(change,owner_spk)]
            unsigned=make_tx([inp(registry_entry),inp(wallet)],outputs,mass)
            registry_sig=create_input_signature(unsigned,0,key)[2:]
            registry_call=bytes.fromhex(config_tool("registry-initialize-call",controller_genesis_cfg,next_config=initialized_cfg,owner_signature=registry_sig))
            tx=make_tx([inp(registry_entry,registry_call),inp(wallet)],outputs,mass)
            tx.inputs[1].signature_script=bytes.fromhex(create_input_signature(tx,1,key)); return tx
        tx,fee=await sized(client,build_registry_init,DEPOSIT+wallet["utxoEntry"]["amount"],DEPOSIT)
        registry_entry,wallet=expected(tx,[(0,DEPOSIT,spk(registry1),registry_id),(1,tx.outputs[1].value,owner_spk,None)])
        record("savings_registry_initialize",tx,fee)

    # 4. Bootstrap Module under the temporary key.
    eb = economic(owner, asset_id, kps_id, reserve_id, remaining=BOOTSTRAP_CAP, expiration=expiration)
    gb = governance(owner, ZERO, root_id, remaining=ROOT_CAP)
    module0_probe = artifact("module-state", **eb, remaining_mint=BOOTSTRAP_CAP, state_position_nonce=0)
    probe = make_tx([inp(root_minter_entry), inp(root), inp(wallet)],
                    [TransactionOutput(DEPOSIT, spk(root_minter)) for _ in range(3)] + [TransactionOutput(DEPOSIT, spk(module0_probe))])
    probe.populate_genesis_covenants([GenesisCovenantGroup(2, [3])])
    module_id = probe.outputs[3].to_dict()["covenant"]["covenantId"]
    eb = economic(owner, asset_id, kps_id, reserve_id, module=module_id, remaining=BOOTSTRAP_CAP, expiration=expiration)
    module0 = artifact("module-state", **eb, remaining_mint=BOOTSTRAP_CAP, state_position_nonce=0)
    module_minter_art = artifact("kcc-state", owner=module_id, amount=0, identifier=2, minter=True)
    root2 = artifact("root-state", **eb, **gb, initialized=True, module_nonce=1,
                     remaining_allocation=ROOT_CAP - BOOTSTRAP_CAP, authority=owner, authority_type=0)
    token_outputs = json.dumps([(root_id, 2, 0, True), (module_id, 2, 0, True)])
    kcc_call = bytes.fromhex(tool("kcc-transfer-call", current_owner=root_id, current_amount=0,
                                  current_identifier=2, current_minter=True, outputs_json=token_outputs,
                                  signature="", witness=0, leader=True))
    def build_module(change, mass):
        outputs = [TransactionOutput(DEPOSIT, spk(root_minter), binding(0, asset_id)),
                   TransactionOutput(DEPOSIT, spk(module_minter_art), binding(0, asset_id)),
                   TransactionOutput(DEPOSIT, spk(root2), binding(1, root_id)),
                   TransactionOutput(DEPOSIT, spk(module0)), TransactionOutput(change, owner_spk)]
        unsigned = make_tx([inp(root_minter_entry, kcc_call), inp(root), inp(wallet)], outputs, mass)
        unsigned.populate_genesis_covenants([GenesisCovenantGroup(2, [3])])
        sig = create_input_signature(unsigned, 1, key)[2:]
        root_call = bytes.fromhex(tool("root-bootstrap-module-call", **eb, **gb,
            current_authority=owner, authority_signature=sig,
            output_module_id=module_id, previous_module_nonce=0,
            previous_remaining_allocation=ROOT_CAP, module_output=3))
        tx = make_tx([inp(root_minter_entry, kcc_call), inp(root, root_call), inp(wallet)], outputs, mass)
        tx.populate_genesis_covenants([GenesisCovenantGroup(2, [3])])
        tx.inputs[2].signature_script = bytes.fromhex(create_input_signature(tx, 2, key)); return tx
    tx, fee = await sized(client, build_module, 2 * DEPOSIT + wallet["utxoEntry"]["amount"], 4 * DEPOSIT)
    root_minter_entry, module_minter, root, module, wallet = expected(tx, [
        (0, DEPOSIT, spk(root_minter), asset_id), (1, DEPOSIT, spk(module_minter_art), asset_id),
        (2, DEPOSIT, spk(root2), root_id), (3, DEPOSIT, spk(module0), module_id),
        (4, tx.outputs[4].value, owner_spk, None)])
    result["bootstrap_module_id"] = module_id; record("bootstrap_module", tx, fee)

    # 5. First mint: usable amount, assigned reserve, and fee are distinct UTXOs.
    position_probe = artifact("position-state", **eb, state_debt=DEBT,
                              state_assigned_reserve=ASSIGNED, challenge_id=ZERO)
    probe = make_tx([inp(module_minter), inp(module), inp(wallet)],
                    [TransactionOutput(DEPOSIT, spk(root_minter)) for _ in range(6)] + [TransactionOutput(COLLATERAL, spk(position_probe))])
    probe.populate_genesis_covenants([GenesisCovenantGroup(2, [6])])
    position_id = probe.outputs[6].to_dict()["covenant"]["covenantId"]
    ep = economic(owner, asset_id, kps_id, reserve_id, module=module_id, position=position_id,
                  remaining=BOOTSTRAP_CAP, nonce=0, expiration=expiration,
                  current_daa=operation_daa)
    position = artifact("position-state", **ep, state_debt=DEBT, state_assigned_reserve=ASSIGNED, challenge_id=ZERO)
    next_module = artifact("module-state", **ep, remaining_mint=BOOTSTRAP_CAP-DEBT, state_position_nonce=1)
    token_specs = [(module_id,2,0,True),(position_id,2,0,True),(owner,0,USABLE,False),
                   (position_id,2,ASSIGNED,False),(reserve_id,2,FEE,False)]
    token_arts = [artifact("kcc-state", owner=o, amount=a, identifier=k, minter=m) for o,k,a,m in token_specs]
    kcc_open = bytes.fromhex(tool("kcc-transfer-call", current_owner=module_id, current_amount=0,
        current_identifier=2, current_minter=True, outputs_json=json.dumps(token_specs), signature="", witness=0, leader=True))
    module_open = bytes.fromhex(tool("module-open-call", **ep, output_position_id=position_id, position_output=6))
    def build_open(change, mass):
        outputs = [TransactionOutput(DEPOSIT, spk(a), binding(0, asset_id)) for a in token_arts]
        outputs += [TransactionOutput(DEPOSIT, spk(next_module), binding(1, module_id)),
                    TransactionOutput(COLLATERAL, spk(position)), TransactionOutput(change, owner_spk)]
        tx = make_tx([inp(module_minter, kcc_open), inp(module, module_open), inp(wallet)],
                     outputs, mass, lock_time=operation_daa)
        tx.populate_genesis_covenants([GenesisCovenantGroup(2, [6])])
        tx.inputs[2].signature_script = bytes.fromhex(create_input_signature(tx, 2, key)); return tx
    tx, fee = await sized(client, build_open, 2*DEPOSIT + wallet["utxoEntry"]["amount"], 6*DEPOSIT + COLLATERAL)
    opened = expected(tx, [(i,DEPOSIT,spk(token_arts[i]),asset_id) for i in range(5)] +
        [(5,DEPOSIT,spk(next_module),module_id),(6,COLLATERAL,spk(position),position_id),(7,tx.outputs[7].value,owner_spk,None)])
    module_minter, position_minter, usable_kusd, assigned_kusd, fee_kusd, module, position_entry, wallet = opened
    result["position_id"] = position_id; record("bootstrap_open", tx, fee)

    # 6. Split the usable UTXO for the exact initial deposit.
    dep_art = artifact("kcc-state", owner=owner, amount=RESERVE_DEPOSIT, identifier=0, minter=False)
    rem_art = artifact("kcc-state", owner=owner, amount=KUSD_CHANGE, identifier=0, minter=False)
    def build_split(change, mass):
        outputs=[TransactionOutput(DEPOSIT,spk(dep_art),binding(0,asset_id)),TransactionOutput(DEPOSIT,spk(rem_art),binding(0,asset_id)),TransactionOutput(change,owner_spk)]
        unsigned=make_tx([inp(usable_kusd),inp(wallet)],outputs,mass); sig=create_input_signature(unsigned,0,key)[2:]
        call=bytes.fromhex(tool("kcc-transfer-call",current_owner=owner,current_amount=USABLE,current_identifier=0,current_minter=False,outputs_json=json.dumps([(owner,0,RESERVE_DEPOSIT,False),(owner,0,KUSD_CHANGE,False)]),signature=sig,witness=0,leader=True))
        tx=make_tx([inp(usable_kusd,call),inp(wallet)],outputs,mass); tx.inputs[1].signature_script=bytes.fromhex(create_input_signature(tx,1,key)); return tx
    tx,fee=await sized(client,build_split,DEPOSIT+wallet["utxoEntry"]["amount"],2*DEPOSIT)
    deposit_kusd,kusd_change,wallet=expected(tx,[(0,DEPOSIT,spk(dep_art),asset_id),(1,DEPOSIT,spk(rem_art),asset_id),(2,tx.outputs[2].value,owner_spk,None)])
    record("split_reserve_deposit", tx, fee)

    # 7. Initialise Reserve/KPS atomiquement.
    ef = economic(owner,asset_id,kps_id,reserve_id,module=module_id,position=position_id,remaining=BOOTSTRAP_CAP-DEBT,nonce=1,expiration=expiration)
    reserve1=artifact("reserve-state",**ef,state_reserve_kusd=RESERVE_DEPOSIT,state_total_kps=RESERVE_DEPOSIT,state_collateral_sompi=0)
    reserve_token=artifact("kcc-state",owner=reserve_id,amount=RESERVE_DEPOSIT,identifier=2,minter=False)
    kps_minter_art=artifact("kps-state",**ef,token_owner=reserve_id,amount=0,identifier=2,minter=True)
    shares=artifact("kps-state",**ef,token_owner=owner,amount=RESERVE_DEPOSIT,identifier=0,minter=False)
    reserve_call=bytes.fromhex(tool("reserve-initialize-call",**ef,output_reserve_id=reserve_id,depositor=owner,deposit_amount=RESERVE_DEPOSIT))
    kps_call=bytes.fromhex(tool("kps-transfer-call",**ef,current_owner=reserve_id,current_amount=0,current_identifier=2,current_minter=True,outputs_json=json.dumps([(reserve_id,2,0,True),(owner,0,RESERVE_DEPOSIT,False)]),signature="",witness=0,leader=True))
    def build_initialize(change,mass):
        outputs=[TransactionOutput(DEPOSIT,spk(reserve1),binding(0,reserve_id)),TransactionOutput(DEPOSIT,spk(reserve_token),binding(1,asset_id)),TransactionOutput(DEPOSIT,spk(kps_minter_art),binding(2,kps_id)),TransactionOutput(DEPOSIT,spk(shares),binding(2,kps_id)),TransactionOutput(change,owner_spk)]
        unsigned=make_tx([inp(reserve),inp(deposit_kusd),inp(kps_minter),inp(wallet)],outputs,mass); sig=create_input_signature(unsigned,1,key)[2:]
        kcc=bytes.fromhex(tool("kcc-transfer-call",current_owner=owner,current_amount=RESERVE_DEPOSIT,current_identifier=0,current_minter=False,outputs_json=json.dumps([(reserve_id,2,RESERVE_DEPOSIT,False)]),signature=sig,witness=0,leader=True))
        tx=make_tx([inp(reserve,reserve_call),inp(deposit_kusd,kcc),inp(kps_minter,kps_call),inp(wallet)],outputs,mass); tx.inputs[3].signature_script=bytes.fromhex(create_input_signature(tx,3,key)); return tx
    tx,fee=await sized(client,build_initialize,3*DEPOSIT+wallet["utxoEntry"]["amount"],4*DEPOSIT)
    reserve,reserve_kusd,kps_minter,kps_owner,wallet=expected(tx,[(0,DEPOSIT,spk(reserve1),reserve_id),(1,DEPOSIT,spk(reserve_token),asset_id),(2,DEPOSIT,spk(kps_minter_art),kps_id),(3,DEPOSIT,spk(shares),kps_id),(4,tx.outputs[4].value,owner_spk,None)])
    record("reserve_initialize", tx, fee)

    # 8. Governance genesis followed by irreversible Root handover.
    eg=economic(owner,asset_id,kps_id,reserve_id,module=module_id,position=position_id,remaining=GOVERNED_CAP,expiration=expiration)
    gg=governance(owner,ZERO,root_id,remaining=ROOT_CAP-BOOTSTRAP_CAP)
    gov0=artifact("governance-state",**eg,**gg,active_proposal_id=ZERO)
    def build_gov(change,mass):
        tx=make_tx([inp(wallet)],[TransactionOutput(DEPOSIT,spk(gov0)),TransactionOutput(change,owner_spk)],mass); tx.populate_genesis_covenants([GenesisCovenantGroup(0,[0])]); return sign_transaction(tx,[key],True)
    tx,fee=await sized(client,build_gov,wallet["utxoEntry"]["amount"],DEPOSIT)
    governance_id=tx.outputs[0].to_dict()["covenant"]["covenantId"]
    gg=governance(owner,governance_id,root_id,remaining=ROOT_CAP-BOOTSTRAP_CAP)
    gov0=artifact("governance-state",**eg,**gg,active_proposal_id=ZERO)
    governance_entry,wallet=expected(tx,[(0,DEPOSIT,spk(gov0),governance_id),(1,tx.outputs[1].value,owner_spk,None)])
    result["governance_id"]=governance_id; record("governance_genesis", tx, fee)
    root_gov=artifact("root-state",**eg,**gg,initialized=True,module_nonce=1,remaining_allocation=ROOT_CAP-BOOTSTRAP_CAP,authority=governance_id,authority_type=4)
    def build_handover(change,mass):
        outputs=[TransactionOutput(DEPOSIT,spk(root_gov),binding(0,root_id)),TransactionOutput(change,owner_spk)]
        unsigned=make_tx([inp(root),inp(wallet)],outputs,mass); sig=create_input_signature(unsigned,0,key)[2:]
        call=bytes.fromhex(tool("root-handover-call",**eg,**gg,current_authority=owner,authority_signature=sig,module_nonce=1,remaining_allocation=ROOT_CAP-BOOTSTRAP_CAP))
        tx=make_tx([inp(root,call),inp(wallet)],outputs,mass); tx.inputs[1].signature_script=bytes.fromhex(create_input_signature(tx,1,key)); return tx
    tx,fee=await sized(client,build_handover,DEPOSIT+wallet["utxoEntry"]["amount"],DEPOSIT)
    root,wallet=expected(tx,[(0,DEPOSIT,spk(root_gov),root_id),(1,tx.outputs[1].value,owner_spk,None)])
    record("root_handover", tx, fee)

    # 9. Split an exact owner-held KUSD proposal fee. The proposal transaction
    # locks it under the Proposal Covenant ID. Execute/cancel refunds it; veto
    # is the only terminal path that can merge it into the Reserve.
    proposal_fee = gg["proposal_fee_kusd"]
    if KUSD_CHANGE <= proposal_fee:
        raise RuntimeError("insufficient owner KUSD for proposal fee and change")
    proposal_change_amount = KUSD_CHANGE - proposal_fee
    proposal_fee_art = artifact(
        "kcc-state", owner=owner, amount=proposal_fee, identifier=0, minter=False
    )
    proposal_change_art = artifact(
        "kcc-state", owner=owner, amount=proposal_change_amount,
        identifier=0, minter=False
    )
    proposal_split_specs = [
        (owner, 0, proposal_fee, False),
        (owner, 0, proposal_change_amount, False),
    ]
    def build_proposal_fee_split(change, mass):
        outputs = [
            TransactionOutput(DEPOSIT, spk(proposal_fee_art), binding(0, asset_id)),
            TransactionOutput(DEPOSIT, spk(proposal_change_art), binding(0, asset_id)),
            TransactionOutput(change, owner_spk),
        ]
        unsigned = make_tx([inp(kusd_change), inp(wallet)], outputs, mass)
        signature = create_input_signature(unsigned, 0, key)[2:]
        token_call = bytes.fromhex(tool(
            "kcc-transfer-call", current_owner=owner, current_amount=KUSD_CHANGE,
            current_identifier=0, current_minter=False,
            outputs_json=json.dumps(proposal_split_specs), signature=signature,
            witness=0, leader=True,
        ))
        tx = make_tx([inp(kusd_change, token_call), inp(wallet)], outputs, mass)
        tx.inputs[1].signature_script = bytes.fromhex(create_input_signature(tx, 1, key))
        return tx
    tx, fee = await sized(
        client, build_proposal_fee_split,
        DEPOSIT + wallet["utxoEntry"]["amount"], 2 * DEPOSIT,
    )
    proposal_fee_token, kusd_change, wallet = expected(tx, [
        (0, DEPOSIT, spk(proposal_fee_art), asset_id),
        (1, DEPOSIT, spk(proposal_change_art), asset_id),
        (2, tx.outputs[2].value, owner_spk, None),
    ])
    record("split_proposal_fee", tx, fee)

    # 10. Permissionless Module proposal. The exact KUSD fee is escrowed under
    # the new Proposal Covenant ID until execute, expiry cancellation, or veto.
    g1=governance(owner,governance_id,root_id,proposal_nonce=1,remaining=ROOT_CAP-BOOTSTRAP_CAP)
    proposal_pending=artifact("proposal-state",**eg,**g1,activated=False)
    probe=make_tx(
        [inp(governance_entry),inp(proposal_fee_token),inp(wallet)],
        [TransactionOutput(DEPOSIT,spk(gov0)),
         TransactionOutput(DEPOSIT,spk(proposal_pending)),
         TransactionOutput(DEPOSIT,owner_spk),
         TransactionOutput(DEPOSIT,owner_spk)],
        lock_time=proposal_operation_daa,
    )
    probe.populate_genesis_covenants([GenesisCovenantGroup(1,[1])])
    proposal_id=probe.outputs[1].to_dict()["covenant"]["covenantId"]
    gov_pending=artifact("governance-state",**eg,**g1,active_proposal_id=proposal_id)
    propose_call=bytes.fromhex(tool("propose-module-call",**eg,**gg,proposal_id=proposal_id,proposal_output=1))
    proposal_fee_escrow = artifact(
        "kcc-state", owner=proposal_id, amount=proposal_fee,
        identifier=2, minter=False,
    )
    def build_propose(change,mass):
        outputs=[
            TransactionOutput(DEPOSIT,spk(gov_pending),binding(0,governance_id)),
            TransactionOutput(DEPOSIT,spk(proposal_pending)),
            TransactionOutput(DEPOSIT,spk(proposal_fee_escrow),binding(1,asset_id)),
            TransactionOutput(change,owner_spk),
        ]
        unsigned=make_tx(
            [inp(governance_entry,propose_call),inp(proposal_fee_token),inp(wallet)],
            outputs,mass,lock_time=proposal_operation_daa,
        )
        unsigned.populate_genesis_covenants([GenesisCovenantGroup(1,[1])])
        fee_signature=create_input_signature(unsigned,1,key)[2:]
        fee_token_call=bytes.fromhex(tool(
            "kcc-transfer-call",current_owner=owner,current_amount=proposal_fee,
            current_identifier=0,current_minter=False,
            outputs_json=json.dumps([(proposal_id,2,proposal_fee,False)]),
            signature=fee_signature,witness=0,leader=True,
        ))
        tx=make_tx(
            [inp(governance_entry,propose_call),inp(proposal_fee_token,fee_token_call),inp(wallet)],
            outputs,mass,lock_time=proposal_operation_daa,
        )
        tx.populate_genesis_covenants([GenesisCovenantGroup(1,[1])])
        tx.inputs[2].signature_script=bytes.fromhex(create_input_signature(tx,2,key))
        return tx
    tx,fee=await sized(
        client,build_propose,
        2*DEPOSIT+wallet["utxoEntry"]["amount"],3*DEPOSIT,
    )
    governance_entry,proposal_entry,proposal_fee_escrow_entry,wallet=expected(tx,[
        (0,DEPOSIT,spk(gov_pending),governance_id),
        (1,DEPOSIT,spk(proposal_pending),proposal_id),
        (2,DEPOSIT,spk(proposal_fee_escrow),asset_id),
        (3,tx.outputs[3].value,owner_spk,None),
    ])
    result["proposal_id"]=proposal_id; record("propose_governed_module", tx, fee)

    # 11. Activation is broadcast only after VOTING_DELAY.
    proposal_active=artifact("proposal-state",**eg,**g1,activated=True)
    activate_call=bytes.fromhex(tool("activate-proposal-call",**eg,**g1))
    def build_activate(change,mass):
        outputs=[TransactionOutput(DEPOSIT,spk(proposal_active),binding(0,proposal_id)),TransactionOutput(change,owner_spk)]
        tx=make_tx([inp(proposal_entry,activate_call,sequence=VOTING_DELAY),inp(wallet)],outputs,mass); tx.inputs[1].signature_script=bytes.fromhex(create_input_signature(tx,1,key)); return tx
    tx,fee=await sized(client,build_activate,DEPOSIT+wallet["utxoEntry"]["amount"],DEPOSIT)
    proposal_entry,wallet=expected(tx,[(0,DEPOSIT,spk(proposal_active),proposal_id),(1,tx.outputs[1].value,owner_spk,None)])
    record("activate_governed_module", tx, fee, requires_age_daa=VOTING_DELAY)

    # 12. Atomic Governance + Proposal + Root + KUSD minter execution.
    gexec=governance(owner,governance_id,root_id,proposal_nonce=1,execution_nonce=1,remaining=ROOT_CAP-BOOTSTRAP_CAP-GOVERNED_CAP)
    governed_probe=artifact("module-state",**eg,remaining_mint=GOVERNED_CAP,state_position_nonce=0)
    probe=make_tx([inp(governance_entry),inp(proposal_entry),inp(root),inp(root_minter_entry),inp(proposal_fee_escrow_entry),inp(wallet)],
                  [TransactionOutput(DEPOSIT,spk(gov0)) for _ in range(4)]+[TransactionOutput(DEPOSIT,spk(governed_probe))])
    probe.populate_genesis_covenants([GenesisCovenantGroup(5,[4])])
    governed_module_id=probe.outputs[4].to_dict()["covenant"]["covenantId"]
    enew=economic(owner,asset_id,kps_id,reserve_id,module=governed_module_id,remaining=GOVERNED_CAP,expiration=expiration)
    governed_module=artifact("module-state",**enew,remaining_mint=GOVERNED_CAP,state_position_nonce=0)
    governed_minter=artifact("kcc-state",owner=governed_module_id,amount=0,identifier=2,minter=True)
    gov_clear=artifact("governance-state",**enew,**gexec,active_proposal_id=ZERO)
    root3=artifact("root-state",**enew,**gexec,initialized=True,module_nonce=2,remaining_allocation=ROOT_CAP-BOOTSTRAP_CAP-GOVERNED_CAP,authority=governance_id,authority_type=4)
    gov_exec=bytes.fromhex(tool("governance-finish-call",**enew,**g1,active_proposal_id=proposal_id,action="execute"))
    proposal_exec=bytes.fromhex(tool("proposal-finish-call",**enew,**g1,action="execute",refund_output=5,fee_input=4,fee_output=6))
    root_exec=bytes.fromhex(tool("root-execute-module-call",**enew,**g1,active_proposal_id=proposal_id,previous_module_nonce=1,previous_remaining_allocation=ROOT_CAP-BOOTSTRAP_CAP,module_output=4))
    proposal_fee_refund=artifact("kcc-state",owner=owner,amount=proposal_fee,identifier=0,minter=False)
    def build_execute(change,mass):
        outputs=[TransactionOutput(DEPOSIT,spk(gov_clear),binding(0,governance_id)),TransactionOutput(DEPOSIT,spk(root3),binding(2,root_id)),TransactionOutput(DEPOSIT,spk(root_minter),binding(3,asset_id)),TransactionOutput(DEPOSIT,spk(governed_minter),binding(3,asset_id)),TransactionOutput(DEPOSIT,spk(governed_module)),TransactionOutput(DEPOSIT,owner_spk),TransactionOutput(DEPOSIT,spk(proposal_fee_refund),binding(4,asset_id)),TransactionOutput(change,owner_spk)]
        fee_outputs=[(root_id,2,0,True),(governed_module_id,2,0,True),(owner,0,proposal_fee,False)]
        unsigned=make_tx([inp(governance_entry,gov_exec),inp(proposal_entry,proposal_exec),inp(root,root_exec),inp(root_minter_entry),inp(proposal_fee_escrow_entry),inp(wallet)],outputs,mass)
        unsigned.populate_genesis_covenants([GenesisCovenantGroup(5,[4])])
        root_call=bytes.fromhex(tool("kcc-transfer-call",current_owner=root_id,current_amount=0,current_identifier=2,current_minter=True,outputs_json=json.dumps(fee_outputs),signature="",witness=0,leader=True))
        fee_call=bytes.fromhex(tool("kcc-transfer-call",current_owner=proposal_id,current_amount=proposal_fee,current_identifier=2,current_minter=False,outputs_json=json.dumps(fee_outputs),signature="",witness=0,leader=False))
        tx=make_tx([inp(governance_entry,gov_exec),inp(proposal_entry,proposal_exec),inp(root,root_exec),inp(root_minter_entry,root_call),inp(proposal_fee_escrow_entry,fee_call),inp(wallet)],outputs,mass)
        tx.populate_genesis_covenants([GenesisCovenantGroup(5,[4])]); tx.inputs[5].signature_script=bytes.fromhex(create_input_signature(tx,5,key)); return tx
    tx,fee=await sized(client,build_execute,5*DEPOSIT+wallet["utxoEntry"]["amount"],7*DEPOSIT)
    governance_entry,root,root_minter_entry,governed_minter,governed_module,refund,proposal_fee_refund_entry,wallet=expected(tx,[(0,DEPOSIT,spk(gov_clear),governance_id),(1,DEPOSIT,spk(root3),root_id),(2,DEPOSIT,spk(root_minter),asset_id),(3,DEPOSIT,spk(governed_minter),asset_id),(4,DEPOSIT,spk(governed_module),governed_module_id),(5,DEPOSIT,owner_spk,None),(6,DEPOSIT,spk(proposal_fee_refund),asset_id),(7,tx.outputs[7].value,owner_spk,None)])
    result["governed_module_id"]=governed_module_id; record("execute_governed_module", tx, fee)

    # The base stack is ready; the previously created Savings governor can
    # now propose and enable its first series.
    if INCLUDE_SAVINGS:
        base_json = _config({**enew, **gexec})["base"]
        gov_json = _config({**enew, **gexec})["governance"]
        savings = _config({**enew, **gexec})["savings"]
        cfg0 = {"base": base_json, "savings": savings, "governance": gov_json}
        governor0 = config_tool("governor-state", cfg0)
        pending_savings = dict(savings)
        pending_savings["proposal_nonce"] = 1
        pending_probe_cfg = {"base": base_json, "savings": pending_savings, "governance": gov_json}
        pending_art = config_tool("proposal-state", pending_probe_cfg, activated=False)
        probe = make_tx([inp(governor_entry), inp(wallet)], [
            TransactionOutput(DEPOSIT, spk(governor0)),
            TransactionOutput(DEPOSIT, spk(pending_art)), TransactionOutput(1, owner_spk)
        ])
        probe.populate_genesis_covenants([GenesisCovenantGroup(1, [1])])
        savings_proposal_id = probe.outputs[1].to_dict()["covenant"]["covenantId"]
        pending_savings["active_proposal_id"] = list(bytes.fromhex(savings_proposal_id))
        pending_cfg = {"base": base_json, "savings": pending_savings, "governance": gov_json}
        governor_pending = config_tool("governor-state", pending_cfg)
        pending_art = config_tool("proposal-state", pending_cfg, activated=False)
        propose_call = bytes.fromhex(config_tool("propose-call", cfg0,
            next_config=pending_cfg, proposal_output=1))
        def build_savings_propose(change, mass):
            outputs=[TransactionOutput(DEPOSIT,spk(governor_pending),binding(0,governor_id)),
                     TransactionOutput(DEPOSIT,spk(pending_art)),TransactionOutput(change,owner_spk)]
            tx=make_tx([inp(governor_entry,propose_call),inp(wallet)],outputs,mass)
            tx.populate_genesis_covenants([GenesisCovenantGroup(1,[1])])
            tx.inputs[1].signature_script=bytes.fromhex(create_input_signature(tx,1,key)); return tx
        tx,fee=await sized(client,build_savings_propose,DEPOSIT+wallet["utxoEntry"]["amount"],2*DEPOSIT)
        governor_entry, savings_proposal_entry, wallet=expected(tx,[
            (0,DEPOSIT,spk(governor_pending),governor_id),(1,DEPOSIT,spk(pending_art),savings_proposal_id),
            (2,tx.outputs[2].value,owner_spk,None)])
        result["savings_proposal_id"]=savings_proposal_id; record("propose_savings",tx,fee)

        # 14. Activation DAA permissionless.
        active_art=config_tool("proposal-state",pending_cfg,activated=True)
        activate_call=bytes.fromhex(config_tool("activate-call",pending_cfg))
        def build_savings_activate(change,mass):
            tx=make_tx([inp(savings_proposal_entry,activate_call,sequence=100),inp(wallet)],
                [TransactionOutput(DEPOSIT,spk(active_art),binding(0,savings_proposal_id)),TransactionOutput(change,owner_spk)],mass)
            tx.inputs[1].signature_script=bytes.fromhex(create_input_signature(tx,1,key)); return tx
        tx,fee=await sized(client,build_savings_activate,DEPOSIT+wallet["utxoEntry"]["amount"],DEPOSIT)
        savings_proposal_entry,wallet=expected(tx,[(0,DEPOSIT,spk(active_art),savings_proposal_id),(1,tx.outputs[1].value,owner_spk,None)])
        record("activate_savings",tx,fee,requires_age_daa=100)

        # 15. Atomic Governor + Proposal + Controller execution.
        executed_savings=dict(pending_savings); executed_savings["active_proposal_id"]=[0]*32; executed_savings["execution_nonce"]=1
        enabled_savings=dict(savings); enabled_savings.update({"enabled":True,"series_nonce":1,"current_rate_ppm":20_000})
        executed_cfg={"base":base_json,"savings":executed_savings,"governance":gov_json}
        enabled_cfg={"base":base_json,"savings":enabled_savings,"governance":gov_json}
        gov_done=config_tool("governor-state",executed_cfg)
        controller_enabled=config_tool("controller-state",enabled_cfg)
        gov_call=bytes.fromhex(config_tool("governor-execute-call",pending_cfg,next_config=executed_cfg,proposal_input=1))
        prop_call=bytes.fromhex(config_tool("proposal-execute-call",pending_cfg,refund_output=2))
        controller_call=bytes.fromhex(config_tool("controller-execute-call",cfg0,next_config=enabled_cfg,governor_input=0,proposal_input=1))
        def build_savings_execute(change,mass):
            tx=make_tx([inp(governor_entry,gov_call),inp(savings_proposal_entry,prop_call),inp(controller_entry,controller_call),inp(wallet)],
                [TransactionOutput(DEPOSIT,spk(gov_done),binding(0,governor_id)),
                 TransactionOutput(DEPOSIT,spk(controller_enabled),binding(2,controller_id)),
                 TransactionOutput(DEPOSIT,owner_spk),TransactionOutput(change,owner_spk)],mass)
            tx.inputs[3].signature_script=bytes.fromhex(create_input_signature(tx,3,key)); return tx
        tx,fee=await sized(client,build_savings_execute,3*DEPOSIT+wallet["utxoEntry"]["amount"],3*DEPOSIT)
        governor_entry,controller_entry,savings_refund,wallet=expected(tx,[
            (0,DEPOSIT,spk(gov_done),governor_id),(1,DEPOSIT,spk(controller_enabled),controller_id),
            (2,DEPOSIT,owner_spk,None),(3,tx.outputs[3].value,owner_spk,None)])
        record("execute_savings",tx,fee)
        result["savings"]={"enabled":True,"rate_ppm":20_000,"governor":governor_entry,
            "controller":controller_entry,"refund":savings_refund}

    result["live"]={"root":root,"root_minter":root_minter_entry,"governance":governance_entry,"reserve":reserve,"reserve_kusd":reserve_kusd,"kps_minter":kps_minter,"kps_owner":kps_owner,"bootstrap_module":module,"bootstrap_module_minter":module_minter,"governed_module":governed_module,"governed_module_minter":governed_minter,"position":position_entry,"position_minter":position_minter,"assigned_kusd":assigned_kusd,"fee_kusd":fee_kusd,"kusd_change":kusd_change,"proposal_fee_refund":proposal_fee_refund_entry,"wallet":wallet}
    result["economics"] = {
        "daa_per_year": DAA_PER_YEAR,
        "risk_premium_ppm": FEE_PPM,
        "bootstrap_fee_kusd": FEE,
        "proposal_fee_kusd": proposal_fee,
        "owner_kusd_change": proposal_change_amount,
        "proposal_fee_refund_kusd": proposal_fee,
        "owner_kusd_spendable_total": proposal_change_amount + proposal_fee,
        "reserve_kusd_after_proposal_fee": RESERVE_DEPOSIT,
        "minimum_collateral_sompi": 100_000_000,
        "auction_duration_daa": PERIOD,
    }
    if INCLUDE_SAVINGS:
        result["live"].update({"savings_registry":registry_entry,"savings_governor":governor_entry,"savings_controller":controller_entry,"savings_refund":savings_refund})
    result["total_fee_sompi"]=sum(item["fee_sompi"] for item in result["transactions"])
    for name, raw in probes:
        Path(f"{PROBE_PREFIX}-{name}.local.json").write_text(json.dumps({"tx": raw}, indent=2, default=str))
    result["consensus_preflight_inputs"] = validate_probe_consensus(probes)
    DRY_RUN.write_text(json.dumps(result,indent=2))
    print(json.dumps({"dry_run":True,"file":str(DRY_RUN),"transactions":len(result["transactions"]),"total_fee_sompi":result["total_fee_sompi"],"ids":{k:result[k] for k in ("asset_id","root_id","reserve_id","kps_id","bootstrap_module_id","position_id","governance_id","proposal_id","governed_module_id")}},indent=2))
    return result


BASE_CHAIN = [
    "root_genesis", "asset_init", "reserve_genesis", "kps_genesis",
    "bootstrap_module", "bootstrap_open", "split_reserve_deposit",
    "reserve_initialize", "governance_genesis", "root_handover",
    "split_proposal_fee", "propose_governed_module", "activate_governed_module",
    "execute_governed_module",
]
SAVINGS_CHAIN = [
    "savings_registry_genesis", "root_genesis", "asset_init", "reserve_genesis",
    "savings_controller_genesis", "kps_genesis", "savings_governor_genesis",
    "savings_controller_initialize", "savings_registry_initialize",
    "bootstrap_module", "bootstrap_open", "split_reserve_deposit", "reserve_initialize",
    "governance_genesis", "root_handover", "split_proposal_fee", "propose_governed_module",
    "activate_governed_module", "execute_governed_module", "propose_savings",
    "activate_savings", "execute_savings",
]
CHAIN = SAVINGS_CHAIN if INCLUDE_SAVINGS else BASE_CHAIN
PRE_PROPOSAL_CHAIN = [
    "bootstrap_open",
    "split_reserve_deposit",
    "reserve_initialize",
    "governance_genesis",
    "root_handover",
    "split_proposal_fee",
]
DAA_RUNWAY_PER_CONFIRMATION = 300


def probe_transaction(name):
    raw = json.loads(Path(f"{PROBE_PREFIX}-{name}.local.json").read_text())["tx"]
    tx = Transaction.from_dict(raw)
    # kaspa-python 2.0.2rc1 does not yet include computeBudget in to_dict.
    for transaction_input in tx.inputs:
        transaction_input.compute_budget = CB
    # Transaction.from_dict retains the supplied ID after restoring
    # compute budget. A new instance recalculates the actual serialized ID.
    fresh = Transaction(
        tx.version, tx.inputs, tx.outputs, tx.lock_time, SUBNETWORK_ID,
        tx.gas, bytes.fromhex(tx.payload), tx.storage_mass,
    )
    fresh.finalize()
    return fresh


def save_progress(result, completed, stage, proposal_daa=None):
    PROGRESS.write_text(json.dumps({
        "network": NETWORK_ID, "protocol": PROTOCOL, "stage": stage,
        "completed": completed, "proposal_daa": proposal_daa or {},
        "plan": result,
    }, indent=2))


async def confirm_output(client, tx, txid, index):
    address = address_from_script_public_key(
        tx.outputs[index].script_public_key, NETWORK_TYPE
    ).to_string()
    return await wait_utxo_resilient(client, address, txid, index)


async def reconnect_rpc(client):
    try:
        await client.disconnect()
    except Exception:
        pass
    await asyncio.sleep(1)
    await client.connect()


async def current_daa_resilient(client):
    last_error = None
    for _ in range(5):
        try:
            return int((await client.get_block_dag_info())["virtualDaaScore"])
        except Exception as error:
            last_error = error
            await reconnect_rpc(client)
    raise RuntimeError(f"unable to read virtual DAA after reconnect: {last_error}")


async def wait_utxo_resilient(client, address, txid, index=None, timeout=180):
    for _ in range(timeout):
        try:
            result = await client.get_utxos_by_addresses({"addresses": [address]})
        except Exception:
            await reconnect_rpc(client)
            continue
        for candidate in result["entries"]:
            outpoint = candidate["outpoint"]
            if outpoint["transactionId"] == txid and (
                index is None or outpoint["index"] == index
            ):
                return candidate
        await asyncio.sleep(1)
    raise TimeoutError(f"confirmation not observed: {txid}")


async def submit_named(client, result, name):
    tx = probe_transaction(name)
    expected_id = next(item["id"] for item in result["transactions"] if item["name"] == name)
    if str(tx.id) != expected_id:
        raise RuntimeError(f"{name}: the persisted probe no longer matches the plan")
    if tx.lock_time:
        current_daa = await current_daa_resilient(client)
        if current_daa < tx.lock_time:
            print(json.dumps({
                "waiting_for_transaction_daa": name,
                "current_daa": current_daa,
                "target_daa": tx.lock_time,
            }, indent=2))
        while current_daa < tx.lock_time:
            await asyncio.sleep(1)
            current_daa = await current_daa_resilient(client)
    response = await client.submit_transaction({"transaction": tx, "allowOrphan": False})
    txid = response["transactionId"]
    if txid != expected_id:
        raise RuntimeError(f"{name}: txid RPC {txid} != expected txid {expected_id}")
    await confirm_output(client, tx, txid, len(tx.outputs) - 1)
    return tx, txid


async def diagnose_live_inputs(client, name):
    """Re-run the local consensus VM with UTXO data observed on Testnet 10."""
    path = Path(f"{PROBE_PREFIX}-{name}.local.json")
    if not path.exists():
        raise RuntimeError(f"missing transaction probe: {path}")
    probe = json.loads(path.read_text())
    for index, transaction_input in enumerate(probe["tx"]["inputs"]):
        previous = transaction_input["previousOutpoint"]
        address = transaction_input["utxo"]["address"]
        entries = (await client.get_utxos_by_addresses({"addresses": [address]}))["entries"]
        match = next(
            (candidate for candidate in entries if candidate["outpoint"] == previous),
            None,
        )
        if match is None:
            raise RuntimeError(f"{name} input {index}: live UTXO not found: {previous}")
        live = match["utxoEntry"]
        # The wRPC binding returns scriptPublicKey as serialized bytes while
        # the Rust diagnostic expects the structured representation already
        # stored in the probe. Its script is immutable for a given outpoint;
        # only replace live consensus metadata here.
        original_script_public_key = transaction_input["utxo"]["scriptPublicKey"]
        transaction_input["utxo"] = {
            "address": match["address"],
            "outpoint": match["outpoint"],
            "amount": live["amount"],
            "scriptPublicKey": original_script_public_key,
            "blockDaaScore": live["blockDaaScore"],
            "isCoinbase": live["isCoinbase"],
            "covenantId": live.get("covenantId"),
        }
        print(json.dumps({
            "input": index,
            "outpoint": previous,
            "block_daa_score": live["blockDaaScore"],
            "transaction_lock_time": probe["tx"]["lockTime"],
        }))
    live_path = Path(f"{PROBE_PREFIX}-{name}-live.local.json")
    live_path.write_text(json.dumps(probe, indent=2))
    completed = subprocess.run(
        ["target/debug/tx-diagnose", "--summary", str(live_path)],
        check=False,
    )
    if completed.returncode:
        raise RuntimeError(
            f"live consensus diagnostic failed with status {completed.returncode}"
        )


async def deploy(client, key):
    if STATE.exists():
        raise SystemExit(f"{STATE} already exists; refusing redeployment")
    if PROGRESS.exists():
        progress = json.loads(PROGRESS.read_text())
        completed = list(progress["completed"])
        if completed:
            result = progress["plan"]
            proposal_daa = dict(progress.get("proposal_daa", {}))
        else:
            # A preflight without a recorded transaction can be regenerated from
            # the new wallet UTXO after a rejection or restart.
            result = await dry_run(client, key)
            proposal_daa = {}
            save_progress(result, completed, "preflight_validated", proposal_daa)
    elif DRY_RUN.exists():
        # A lock time older than newly confirmed parents would invalidate
        # DAA checks. A stale dry run is therefore
        # rebuilt from the live wallet UTXO before submission.
        candidate = json.loads(DRY_RUN.read_text())
        current_daa = await current_daa_resilient(client)
        if int(candidate.get("operation_daa", 0)) <= current_daa + MIN_DEPLOYMENT_DAA_RUNWAY:
            result = await dry_run(client, key)
        else:
            result = candidate
        result["protocol"] = PROTOCOL
        completed, proposal_daa = [], {}
        save_progress(result, completed, "preflight_validated", proposal_daa)
    else:
        result = await dry_run(client, key)
        completed, proposal_daa = [], {}
        save_progress(result, completed, "preflight_validated", proposal_daa)

    # Strictly sequential submission. Each activation waits for its own
    # proposal confirmation DAA, including after resume.
    for name in CHAIN:
        if name in completed:
            continue
        if (
            name in PRE_PROPOSAL_CHAIN
            and "propose_governed_module" not in completed
        ):
            current_daa = await current_daa_resilient(client)
            remaining_parents = sum(
                candidate not in completed
                for candidate in PRE_PROPOSAL_CHAIN
            )
            required_runway = remaining_parents * DAA_RUNWAY_PER_CONFIRMATION
            proposal_daa_target = int(result["proposal_operation_daa"])
            if current_daa + required_runway >= proposal_daa_target:
                raise RuntimeError(
                    "deployment plan no longer has enough DAA runway before "
                    f"propose_governed_module: current={current_daa}, "
                    f"target={proposal_daa_target}, required={required_runway}; "
                    "start again with a fresh KUSD_TAG"
                )
        proposal_name = {
            "activate_governed_module": "propose_governed_module",
            "activate_savings": "propose_savings",
        }.get(name)
        if proposal_name:
            if proposal_name not in proposal_daa:
                raise RuntimeError(f"missing DAA for {proposal_name}")
            maturity = int(proposal_daa[proposal_name]) + VOTING_DELAY
            while await current_daa_resilient(client) < maturity:
                await asyncio.sleep(1)
        tx, txid = await submit_named(client, result, name)
        completed.append(name)
        if name in ("propose_governed_module", "propose_savings"):
            proposal = await confirm_output(client, tx, txid, 1)
            proposal_daa[name] = int(proposal["utxoEntry"]["blockDaaScore"])
        save_progress(result, completed, f"{name}_confirmed", proposal_daa)
        print(json.dumps({"confirmed": name, "transaction_id": txid}, indent=2))

    # Replace synthetic entries (blockDaaScore=0) with RPC entries.
    live = {}
    for name, expected in result["live"].items():
        live[name] = await wait_utxo_resilient(
            client, expected["address"], expected["outpoint"]["transactionId"],
            expected["outpoint"]["index"], timeout=60,
        )
    result["dry_run"] = False
    result["stage"] = "savings_ready"
    result["proposal_daa"] = proposal_daa
    result["live"] = live
    STATE.write_text(json.dumps(result, indent=2))
    save_progress(result, completed, result["stage"], proposal_daa)
    print(json.dumps({
        "stage": result["stage"], "file": str(STATE),
        "transactions": {item["name"]: item["id"] for item in result["transactions"]},
    }, indent=2))


async def verify(client):
    state = json.loads(STATE.read_text())
    checked = {}
    for name, expected in state["live"].items():
        checked[name] = (await wait_utxo_resilient(
            client, expected["address"], expected["outpoint"]["transactionId"],
            expected["outpoint"]["index"], timeout=15,
        ))["outpoint"]
    print(json.dumps({"network": NETWORK_ID, "stage": state["stage"], "live": checked}, indent=2))


async def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("command", choices=["dry-run", "deploy", "verify", "diagnose"])
    parser.add_argument("--transaction", default="bootstrap_open")
    args = parser.parse_args()
    url,key=load_env(); client=RpcClient(resolver=None,url=None if url=="public" else url,network_id=NETWORK_ID)
    await client.connect();
    try:
        if args.command=="dry-run": await dry_run(client,key)
        elif args.command=="deploy": await deploy(client,key)
        elif args.command=="verify": await verify(client)
        else: await diagnose_live_inputs(client, args.transaction)
    finally: await client.disconnect()


if __name__ == "__main__":
    try:
        asyncio.run(main())
        ERROR_LOG.unlink(missing_ok=True)
    except Exception as error:
        ERROR_LOG.write_text(f"{type(error).__name__}: {error}\n")
        raise
