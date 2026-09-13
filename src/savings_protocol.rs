use crate::artifact::Artifact;
use crate::protocol::{
    GovernanceParams, GovernanceStack, ProtocolParams, ProtocolStack, TokenOutput, anchor_args,
    auction_args, challenge_args, challenged_position_args, compile_governance_stack_with_base,
    compile_position_pair, compile_stack as compile_protocol_stack, governance_args,
    governance_state_expr, module_args, module_state_expr, position_args,
    position_state_for_module, proposal_args_with_activation,
    proposal_state_expr as module_proposal_state_expr, root_args, root_args_with_state,
    root_state_expr, token_state_expr,
};
use crate::silverscript::{
    CompileOptions, CompiledContract, CovenantDeclCallOptions, compile_contract, struct_object,
};
use kaspa_txscript::{EngineFlags, script_builder::ScriptBuilder};
use serde::{Deserialize, Serialize};
use silverscript_lang::ast::{Expr, parse_type_ref};
use std::fs;

pub const PPM: i64 = 1_000_000;
pub const MAX_AMOUNT: i64 = 36_028_797_018_963_967;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SavingsParams {
    pub owner: Vec<u8>,
    pub referrer: Vec<u8>,
    pub proposer: Vec<u8>,
    pub controller_id: Vec<u8>,
    pub governor_id: Vec<u8>,
    #[serde(default = "zero_id")]
    pub registry_id: Vec<u8>,
    pub active_proposal_id: Vec<u8>,
    pub enabled: bool,
    pub proposal_nonce: i64,
    pub execution_nonce: i64,
    pub series_nonce: i64,
    pub account_nonce: i64,
    pub total_saved: i64,
    pub saved: i64,
    pub rate_ppm: i64,
    pub current_rate_ppm: i64,
    pub interest_delay_daa: i64,
    pub delay_remaining_daa: i64,
    pub max_accrual_daa: i64,
    pub remaining_accrual_daa: i64,
    pub daa_per_year: i64,
    pub referral_fee_ppm: i64,
    pub voting_delay_daa: i64,
    pub execution_window_daa: i64,
    pub veto_threshold_ppm: i64,
    pub proposal_deposit_sompi: i64,
    pub account_value_sompi: i64,
}

fn zero_id() -> Vec<u8> {
    vec![0; 32]
}

#[derive(Clone, Debug)]
pub struct SavingsStack {
    pub base: ProtocolStack,
    pub proposal: Artifact,
    pub governor: Artifact,
    pub account: Artifact,
    pub controller: Artifact,
    pub reserve: Artifact,
    pub registry: Artifact,
}

#[derive(Clone, Debug)]
pub struct FullStack {
    pub savings: SavingsStack,
    pub base_governance: GovernanceStack,
}

fn source(path: &str) -> Result<String, String> {
    fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))
}

fn compile(path: &str, args: &[Expr<'_>]) -> Result<Artifact, String> {
    let src = source(path)?;
    compile_contract(&src, args, CompileOptions::default())
        .map(|contract| {
            let layout = contract.state_layout;
            Artifact {
                bytecode: contract.bytecode.clone(),
                prefix: contract.bytecode[..layout.start].to_vec(),
                suffix: contract.bytecode[layout.start + layout.len..].to_vec(),
                template_hash: contract.template_hash().to_vec(),
            }
        })
        .map_err(|e| format!("{path}: {e}"))
}

fn compile_for_call<'a>(
    path: &str,
    src: &'a str,
    args: &[Expr<'a>],
) -> Result<CompiledContract<'a>, String> {
    compile_contract(src, args, CompileOptions::default()).map_err(|e| format!("{path}: {e}"))
}

fn empty_artifact() -> Artifact {
    Artifact {
        bytecode: vec![],
        prefix: vec![],
        suffix: vec![],
        template_hash: vec![0; 32],
    }
}

fn covenant_call(
    contract: &CompiledContract<'_>,
    policy: &str,
    args: Vec<Expr<'_>>,
    leader: bool,
) -> Result<Vec<u8>, String> {
    let mut script = contract
        .build_sig_script_for_covenant_decl(
            policy,
            args,
            CovenantDeclCallOptions { is_leader: leader },
        )
        .map_err(|e| e.to_string())?;
    script.extend(
        ScriptBuilder::with_flags(EngineFlags {
            covenants_enabled: true,
            ..Default::default()
        })
        .add_data(&contract.bytecode)
        .map_err(|e| e.to_string())?
        .drain(),
    );
    Ok(script)
}

fn entry_call(
    contract: &CompiledContract<'_>,
    policy: &str,
    args: Vec<Expr<'_>>,
) -> Result<Vec<u8>, String> {
    let mut script = contract
        .build_sig_script(policy, args)
        .map_err(|e| e.to_string())?;
    script.extend(
        ScriptBuilder::with_flags(EngineFlags {
            covenants_enabled: true,
            ..Default::default()
        })
        .add_data(&contract.bytecode)
        .map_err(|e| e.to_string())?
        .drain(),
    );
    Ok(script)
}

fn validate(base: &ProtocolParams, p: &SavingsParams) -> Result<(), String> {
    for (name, value) in [
        ("owner", &p.owner),
        ("referrer", &p.referrer),
        ("proposer", &p.proposer),
        ("controller_id", &p.controller_id),
        ("governor_id", &p.governor_id),
        ("registry_id", &p.registry_id),
        ("active_proposal_id", &p.active_proposal_id),
    ] {
        if value.len() != 32 {
            return Err(format!("{name}: 32 bytes required"));
        }
    }
    if base.asset_id.len() != 32 || base.kps_id.len() != 32 || base.reserve_id.len() != 32 {
        return Err("Asset/KPS/Reserve ID: 32 bytes required".into());
    }
    if !(0..=PPM).contains(&p.rate_ppm)
        || !(0..=PPM).contains(&p.current_rate_ppm)
        || !(0..=250_000).contains(&p.referral_fee_ppm)
        || !(1..=PPM).contains(&p.veto_threshold_ppm)
        || p.interest_delay_daa <= 0
        || p.delay_remaining_daa < 0
        || p.max_accrual_daa <= 0
        || p.remaining_accrual_daa < 0
        || p.daa_per_year <= 0
        || p.voting_delay_daa <= 0
        || p.execution_window_daa <= 0
        || p.proposal_deposit_sompi <= 0
        || p.account_value_sompi <= 0
        || p.saved < 0
        || p.total_saved < 0
        || p.saved > MAX_AMOUNT
        || p.total_saved > MAX_AMOUNT
    {
        return Err("invalid Savings parameters".into());
    }
    Ok(())
}

fn token_state(name: &'static str, output: &TokenOutput) -> Expr<'static> {
    struct_object(
        name,
        vec![
            ("ownerIdentifier", Expr::bytes(output.owner.clone())),
            ("identifierType", Expr::byte(output.identifier_type)),
            ("amount", Expr::int(output.amount)),
            ("isMinter", Expr::bool(output.is_minter)),
        ],
    )
}

fn proposal_args(p: &SavingsParams, base: &ProtocolParams, activated: bool) -> Vec<Expr<'static>> {
    vec![
        Expr::bytes(p.proposer.clone()),
        Expr::bytes(p.governor_id.clone()),
        Expr::bytes(p.controller_id.clone()),
        Expr::bytes(base.asset_id.clone()),
        Expr::int(p.proposal_nonce),
        Expr::bool(activated),
        Expr::int(p.rate_ppm),
        Expr::int(p.interest_delay_daa),
        Expr::int(p.max_accrual_daa),
        Expr::int(p.daa_per_year),
        Expr::int(p.voting_delay_daa),
        Expr::int(p.execution_window_daa),
        Expr::int(p.proposal_deposit_sompi),
    ]
}

fn governor_args(
    p: &SavingsParams,
    base: &ProtocolParams,
    proposal: &Artifact,
    kps: &Artifact,
    delegation: &Artifact,
    reserve: &Artifact,
) -> Vec<Expr<'static>> {
    vec![
        Expr::bytes(base.asset_id.clone()),
        Expr::bytes(base.kps_id.clone()),
        Expr::bytes(base.reserve_id.clone()),
        Expr::bytes(p.controller_id.clone()),
        Expr::int(p.proposal_nonce),
        Expr::int(p.execution_nonce),
        Expr::bytes(p.active_proposal_id.clone()),
        Expr::int(p.voting_delay_daa),
        Expr::int(p.execution_window_daa),
        Expr::int(p.veto_threshold_ppm),
        Expr::int(p.proposal_deposit_sompi),
        Expr::dynamic_bytes(proposal.prefix.clone()),
        Expr::dynamic_bytes(proposal.suffix.clone()),
        Expr::bytes(proposal.template_hash.clone()),
        Expr::int(kps.prefix.len() as i64),
        Expr::int(kps.suffix.len() as i64),
        Expr::bytes(kps.template_hash.clone()),
        Expr::int(delegation.prefix.len() as i64),
        Expr::int(delegation.suffix.len() as i64),
        Expr::bytes(delegation.template_hash.clone()),
        Expr::int(reserve.prefix.len() as i64),
        Expr::int(reserve.suffix.len() as i64),
        Expr::bytes(reserve.template_hash.clone()),
        Expr::int(base.max_kps_vote_weight),
    ]
}

fn account_args(
    p: &SavingsParams,
    base: &ProtocolParams,
    kcc: &Artifact,
    controller: &Artifact,
    reserve: &Artifact,
) -> Vec<Expr<'static>> {
    vec![
        Expr::bytes(p.owner.clone()),
        Expr::bytes(p.controller_id.clone()),
        Expr::bytes(base.reserve_id.clone()),
        Expr::bytes(base.asset_id.clone()),
        Expr::int(p.saved),
        Expr::int(p.rate_ppm),
        Expr::int(p.delay_remaining_daa),
        Expr::int(p.remaining_accrual_daa),
        Expr::int(p.daa_per_year),
        Expr::bytes(p.referrer.clone()),
        Expr::int(p.referral_fee_ppm),
        Expr::int(kcc.prefix.len() as i64),
        Expr::int(kcc.suffix.len() as i64),
        Expr::bytes(kcc.template_hash.clone()),
        Expr::int(controller.prefix.len() as i64),
        Expr::int(controller.suffix.len() as i64),
        Expr::bytes(controller.template_hash.clone()),
        Expr::int(reserve.prefix.len() as i64),
        Expr::int(reserve.suffix.len() as i64),
        Expr::bytes(reserve.template_hash.clone()),
    ]
}

fn controller_args(
    p: &SavingsParams,
    base: &ProtocolParams,
    kcc: &Artifact,
    account: &Artifact,
    governor: &Artifact,
    proposal: &Artifact,
    controller: &Artifact,
    reserve: &Artifact,
) -> Vec<Expr<'static>> {
    vec![
        Expr::bytes(p.owner.clone()),
        Expr::bytes(base.asset_id.clone()),
        Expr::bytes(base.reserve_id.clone()),
        Expr::bytes(p.governor_id.clone()),
        Expr::bool(p.enabled),
        Expr::int(p.series_nonce),
        Expr::int(p.account_nonce),
        Expr::int(p.total_saved),
        Expr::int(p.current_rate_ppm),
        Expr::int(p.interest_delay_daa),
        Expr::int(p.max_accrual_daa),
        Expr::int(p.daa_per_year),
        Expr::int(p.account_value_sompi),
        Expr::int(controller.prefix.len() as i64),
        Expr::int(controller.suffix.len() as i64),
        Expr::bytes(controller.template_hash.clone()),
        Expr::int(reserve.prefix.len() as i64),
        Expr::int(reserve.suffix.len() as i64),
        Expr::bytes(reserve.template_hash.clone()),
        Expr::int(kcc.prefix.len() as i64),
        Expr::int(kcc.suffix.len() as i64),
        Expr::bytes(kcc.template_hash.clone()),
        Expr::int(account.prefix.len() as i64),
        Expr::int(account.suffix.len() as i64),
        Expr::bytes(account.template_hash.clone()),
        Expr::int(governor.prefix.len() as i64),
        Expr::int(governor.suffix.len() as i64),
        Expr::bytes(governor.template_hash.clone()),
        Expr::int(proposal.prefix.len() as i64),
        Expr::int(proposal.suffix.len() as i64),
        Expr::bytes(proposal.template_hash.clone()),
    ]
}

fn reserve_args(
    base: &ProtocolParams,
    p: &SavingsParams,
    kcc: &Artifact,
    kps: &Artifact,
    auction: &Artifact,
    account: &Artifact,
    controller: &Artifact,
    registry: &Artifact,
) -> Vec<Expr<'static>> {
    vec![
        Expr::bytes(base.asset_id.clone()),
        Expr::bytes(base.kps_id.clone()),
        Expr::int(base.reserve_kusd),
        Expr::int(base.total_kps),
        Expr::int(base.reserve_collateral_sompi),
        Expr::int(kcc.prefix.len() as i64),
        Expr::int(kcc.suffix.len() as i64),
        Expr::bytes(kcc.template_hash.clone()),
        Expr::int(kps.prefix.len() as i64),
        Expr::int(kps.suffix.len() as i64),
        Expr::bytes(kps.template_hash.clone()),
        Expr::int(auction.prefix.len() as i64),
        Expr::int(auction.suffix.len() as i64),
        Expr::bytes(auction.template_hash.clone()),
        Expr::bytes(p.registry_id.clone()),
        Expr::int(registry.prefix.len() as i64),
        Expr::int(registry.suffix.len() as i64),
        Expr::bytes(registry.template_hash.clone()),
        Expr::int(account.prefix.len() as i64),
        Expr::int(account.suffix.len() as i64),
        Expr::bytes(account.template_hash.clone()),
        Expr::int(controller.prefix.len() as i64),
        Expr::int(controller.suffix.len() as i64),
        Expr::bytes(controller.template_hash.clone()),
    ]
}

pub fn compile_stack(base: &ProtocolParams, p: &SavingsParams) -> Result<SavingsStack, String> {
    validate(base, p)?;
    let mut base_stack = compile_protocol_stack(base)?;
    let registry = compile(
        "contracts/savings-registry.sil",
        &[
            Expr::bytes(p.owner.clone()),
            Expr::bytes(p.controller_id.clone()),
            Expr::bool(p.controller_id != vec![0; 32]),
        ],
    )?;
    let proposal = compile(
        "contracts/savings-proposal.sil",
        &proposal_args(p, base, false),
    )?;
    let governor_probe = compile(
        "contracts/savings-governor.sil",
        &governor_args(
            p,
            base,
            &proposal,
            &base_stack.kps,
            &base_stack.delegation,
            &empty_artifact(),
        ),
    )?;
    let account_probe = compile(
        "contracts/savings-account.sil",
        &account_args(
            p,
            base,
            &base_stack.kcc,
            &empty_artifact(),
            &empty_artifact(),
        ),
    )?;
    let controller_probe = compile(
        "contracts/savings-controller.sil",
        &controller_args(
            p,
            base,
            &base_stack.kcc,
            &account_probe,
            &governor_probe,
            &proposal,
            &empty_artifact(),
            &empty_artifact(),
        ),
    )?;
    let reserve_probe = compile(
        "contracts/equity-reserve.sil",
        &reserve_args(
            base,
            p,
            &base_stack.kcc,
            &base_stack.kps,
            &empty_artifact(),
            &account_probe,
            &controller_probe,
            &registry,
        ),
    )?;
    base_stack.reserve = reserve_probe.clone();
    base_stack.auction = compile("contracts/auction.sil", &auction_args(base, &base_stack))?;
    let reserve = compile(
        "contracts/equity-reserve.sil",
        &reserve_args(
            base,
            p,
            &base_stack.kcc,
            &base_stack.kps,
            &base_stack.auction,
            &account_probe,
            &controller_probe,
            &registry,
        ),
    )?;
    if reserve.prefix != reserve_probe.prefix
        || reserve.suffix != reserve_probe.suffix
        || reserve.template_hash != reserve_probe.template_hash
    {
        return Err("the Reserve/Auction template cycle is not stable".into());
    }
    base_stack.reserve = reserve.clone();
    let final_auction = compile("contracts/auction.sil", &auction_args(base, &base_stack))?;
    if final_auction.template_hash != base_stack.auction.template_hash
        || final_auction.prefix != base_stack.auction.prefix
        || final_auction.suffix != base_stack.auction.suffix
    {
        return Err("the second Auction compilation pass changed its identity".into());
    }
    base_stack.auction = final_auction;
    base_stack.challenge = compile(
        "contracts/challenge.sil",
        &challenge_args(base, &base_stack),
    )?;
    base_stack.anchor = compile(
        "contracts/challenge-anchor.sil",
        &anchor_args(base, &base_stack),
    )?;
    compile_position_pair(base, &mut base_stack)?;
    base_stack.module = compile(
        "contracts/minting-module.sil",
        &module_args(base, &base_stack),
    )?;

    let governor = compile(
        "contracts/savings-governor.sil",
        &governor_args(
            p,
            base,
            &proposal,
            &base_stack.kps,
            &base_stack.delegation,
            &reserve,
        ),
    )?;
    let account = compile(
        "contracts/savings-account.sil",
        &account_args(p, base, &base_stack.kcc, &controller_probe, &reserve),
    )?;
    let controller = compile(
        "contracts/savings-controller.sil",
        &controller_args(
            p,
            base,
            &base_stack.kcc,
            &account,
            &governor,
            &proposal,
            &controller_probe,
            &reserve,
        ),
    )?;
    for (name, actual, expected) in [
        ("Governor", &governor, &governor_probe),
        ("Account", &account, &account_probe),
        ("Controller", &controller, &controller_probe),
    ] {
        if actual.prefix != expected.prefix
            || actual.suffix != expected.suffix
            || actual.template_hash != expected.template_hash
        {
            return Err(format!("template {name} is not stable"));
        }
    }
    Ok(SavingsStack {
        base: base_stack,
        proposal,
        governor,
        account,
        controller,
        reserve,
        registry,
    })
}

pub fn compile_proposal_state(
    base: &ProtocolParams,
    p: &SavingsParams,
    activated: bool,
) -> Result<Artifact, String> {
    validate(base, p)?;
    compile(
        "contracts/savings-proposal.sil",
        &proposal_args(p, base, activated),
    )
}

pub fn compile_registry_state(p: &SavingsParams, initialized: bool) -> Result<Artifact, String> {
    compile(
        "contracts/savings-registry.sil",
        &[
            Expr::bytes(p.owner.clone()),
            Expr::bytes(p.controller_id.clone()),
            Expr::bool(initialized),
        ],
    )
}

fn registry_state_expr(p: &SavingsParams, initialized: bool, name: &'static str) -> Expr<'static> {
    struct_object(
        name,
        vec![
            ("bootstrapOwner", Expr::bytes(p.owner.clone())),
            ("controllerId", Expr::bytes(p.controller_id.clone())),
            ("initialized", Expr::bool(initialized)),
        ],
    )
}

pub fn registry_initialize_call(
    prev: &SavingsParams,
    next: &SavingsParams,
    owner_signature: Vec<u8>,
) -> Result<Vec<u8>, String> {
    if prev.controller_id != vec![0; 32] || next.controller_id == vec![0; 32] {
        return Err("invalid Registry initialization".into());
    }
    let src = source("contracts/savings-registry.sil")?;
    let contract = compile_for_call(
        "SavingsRegistry",
        &src,
        &[
            Expr::bytes(prev.owner.clone()),
            Expr::bytes(prev.controller_id.clone()),
            Expr::bool(false),
        ],
    )?;
    covenant_call(
        &contract,
        "initializePolicy",
        vec![
            registry_state_expr(next, true, "State"),
            Expr::bytes(owner_signature),
        ],
        true,
    )
}

pub fn registry_preserve_call(p: &SavingsParams) -> Result<Vec<u8>, String> {
    let src = source("contracts/savings-registry.sil")?;
    let contract = compile_for_call(
        "SavingsRegistry",
        &src,
        &[
            Expr::bytes(p.owner.clone()),
            Expr::bytes(p.controller_id.clone()),
            Expr::bool(true),
        ],
    )?;
    covenant_call(
        &contract,
        "preservePolicy",
        vec![registry_state_expr(p, true, "State")],
        true,
    )
}

pub fn compile_governor_state(
    base: &ProtocolParams,
    p: &SavingsParams,
) -> Result<Artifact, String> {
    let stack = compile_stack(base, p)?;
    compile(
        "contracts/savings-governor.sil",
        &governor_args(
            p,
            base,
            &stack.proposal,
            &stack.base.kps,
            &stack.base.delegation,
            &stack.reserve,
        ),
    )
}

pub fn compile_controller_state(
    base: &ProtocolParams,
    p: &SavingsParams,
) -> Result<Artifact, String> {
    let stack = compile_stack(base, p)?;
    compile(
        "contracts/savings-controller.sil",
        &controller_args(
            p,
            base,
            &stack.base.kcc,
            &stack.account,
            &stack.governor,
            &stack.proposal,
            &stack.controller,
            &stack.reserve,
        ),
    )
}

pub fn compile_account_state(base: &ProtocolParams, p: &SavingsParams) -> Result<Artifact, String> {
    let stack = compile_stack(base, p)?;
    compile(
        "contracts/savings-account.sil",
        &account_args(p, base, &stack.base.kcc, &stack.controller, &stack.reserve),
    )
}

pub fn compile_reserve_state(base: &ProtocolParams, p: &SavingsParams) -> Result<Artifact, String> {
    Ok(compile_stack(base, p)?.reserve)
}

fn proposal_state_expr(
    base: &ProtocolParams,
    p: &SavingsParams,
    activated: bool,
    name: &'static str,
) -> Expr<'static> {
    struct_object(
        name,
        vec![
            ("proposer", Expr::bytes(p.proposer.clone())),
            ("governanceId", Expr::bytes(p.governor_id.clone())),
            ("controllerId", Expr::bytes(p.controller_id.clone())),
            ("assetId", Expr::bytes(base.asset_id.clone())),
            ("proposalNonce", Expr::int(p.proposal_nonce)),
            ("activated", Expr::bool(activated)),
            ("ratePpm", Expr::int(p.rate_ppm)),
            ("interestDelayDaa", Expr::int(p.interest_delay_daa)),
            ("maxAccrualDaa", Expr::int(p.max_accrual_daa)),
            ("daaPerYear", Expr::int(p.daa_per_year)),
            ("votingDelayDaa", Expr::int(p.voting_delay_daa)),
            ("executionWindowDaa", Expr::int(p.execution_window_daa)),
            ("depositSompi", Expr::int(p.proposal_deposit_sompi)),
        ],
    )
}

fn governor_state_expr(
    base: &ProtocolParams,
    p: &SavingsParams,
    stack: &SavingsStack,
    name: &'static str,
) -> Expr<'static> {
    struct_object(
        name,
        vec![
            ("assetId", Expr::bytes(base.asset_id.clone())),
            ("kpsId", Expr::bytes(base.kps_id.clone())),
            ("reserveId", Expr::bytes(base.reserve_id.clone())),
            ("controllerId", Expr::bytes(p.controller_id.clone())),
            ("proposalNonce", Expr::int(p.proposal_nonce)),
            ("executionNonce", Expr::int(p.execution_nonce)),
            (
                "activeProposalId",
                Expr::bytes(p.active_proposal_id.clone()),
            ),
            ("votingDelayDaa", Expr::int(p.voting_delay_daa)),
            ("executionWindowDaa", Expr::int(p.execution_window_daa)),
            ("vetoThresholdPpm", Expr::int(p.veto_threshold_ppm)),
            ("proposalDepositSompi", Expr::int(p.proposal_deposit_sompi)),
            (
                "reservePrefixLenState",
                Expr::int(stack.reserve.prefix.len() as i64),
            ),
            (
                "reserveSuffixLenState",
                Expr::int(stack.reserve.suffix.len() as i64),
            ),
            (
                "reserveTemplateHashState",
                Expr::bytes(stack.reserve.template_hash.clone()),
            ),
        ],
    )
}

fn controller_state_expr(
    base: &ProtocolParams,
    p: &SavingsParams,
    stack: &SavingsStack,
    name: &'static str,
) -> Expr<'static> {
    struct_object(
        name,
        vec![
            ("assetId", Expr::bytes(base.asset_id.clone())),
            ("reserveId", Expr::bytes(base.reserve_id.clone())),
            ("governanceId", Expr::bytes(p.governor_id.clone())),
            ("enabled", Expr::bool(p.enabled)),
            ("seriesNonce", Expr::int(p.series_nonce)),
            ("accountNonce", Expr::int(p.account_nonce)),
            ("totalSaved", Expr::int(p.total_saved)),
            ("currentRatePpm", Expr::int(p.current_rate_ppm)),
            ("interestDelayDaa", Expr::int(p.interest_delay_daa)),
            ("maxAccrualDaa", Expr::int(p.max_accrual_daa)),
            ("daaPerYear", Expr::int(p.daa_per_year)),
            ("accountValueSompi", Expr::int(p.account_value_sompi)),
            (
                "controllerPrefixLenState",
                Expr::int(stack.controller.prefix.len() as i64),
            ),
            (
                "controllerSuffixLenState",
                Expr::int(stack.controller.suffix.len() as i64),
            ),
            (
                "controllerTemplateHashState",
                Expr::bytes(stack.controller.template_hash.clone()),
            ),
            (
                "reservePrefixLenState",
                Expr::int(stack.reserve.prefix.len() as i64),
            ),
            (
                "reserveSuffixLenState",
                Expr::int(stack.reserve.suffix.len() as i64),
            ),
            (
                "reserveTemplateHashState",
                Expr::bytes(stack.reserve.template_hash.clone()),
            ),
        ],
    )
}

fn account_state_expr(
    base: &ProtocolParams,
    p: &SavingsParams,
    stack: &SavingsStack,
    name: &'static str,
) -> Expr<'static> {
    struct_object(
        name,
        vec![
            ("owner", Expr::bytes(p.owner.clone())),
            ("controllerId", Expr::bytes(p.controller_id.clone())),
            ("reserveId", Expr::bytes(base.reserve_id.clone())),
            ("assetId", Expr::bytes(base.asset_id.clone())),
            ("saved", Expr::int(p.saved)),
            ("ratePpm", Expr::int(p.rate_ppm)),
            ("delayRemainingDaa", Expr::int(p.delay_remaining_daa)),
            ("remainingAccrualDaa", Expr::int(p.remaining_accrual_daa)),
            ("daaPerYear", Expr::int(p.daa_per_year)),
            ("referrer", Expr::bytes(p.referrer.clone())),
            ("referralFeePpm", Expr::int(p.referral_fee_ppm)),
            (
                "controllerPrefixLen",
                Expr::int(stack.controller.prefix.len() as i64),
            ),
            (
                "controllerSuffixLen",
                Expr::int(stack.controller.suffix.len() as i64),
            ),
            (
                "controllerTemplateHash",
                Expr::bytes(stack.controller.template_hash.clone()),
            ),
            (
                "reservePrefixLen",
                Expr::int(stack.reserve.prefix.len() as i64),
            ),
            (
                "reserveSuffixLen",
                Expr::int(stack.reserve.suffix.len() as i64),
            ),
            (
                "reserveTemplateHash",
                Expr::bytes(stack.reserve.template_hash.clone()),
            ),
        ],
    )
}

fn reserve_state_expr(
    base: &ProtocolParams,
    stack: &SavingsStack,
    name: &'static str,
) -> Expr<'static> {
    struct_object(
        name,
        vec![
            ("kusdAssetId", Expr::bytes(base.asset_id.clone())),
            ("kpsAssetId", Expr::bytes(base.kps_id.clone())),
            ("reserveKusd", Expr::int(base.reserve_kusd)),
            ("totalKps", Expr::int(base.total_kps)),
            ("collateralSompi", Expr::int(base.reserve_collateral_sompi)),
            (
                "auctionPrefixLenState",
                Expr::int(stack.base.auction.prefix.len() as i64),
            ),
            (
                "auctionSuffixLenState",
                Expr::int(stack.base.auction.suffix.len() as i64),
            ),
            (
                "auctionTemplateHashState",
                Expr::bytes(stack.base.auction.template_hash.clone()),
            ),
        ],
    )
}

fn base_challenge_state_expr(
    base: &ProtocolParams,
    stack: &SavingsStack,
    challenger: Vec<u8>,
    name: &'static str,
) -> Expr<'static> {
    struct_object(
        name,
        vec![
            ("owner", Expr::bytes(base.owner.clone())),
            ("challenger", Expr::bytes(challenger)),
            ("positionId", Expr::bytes(base.position_id.clone())),
            ("assetId", Expr::bytes(base.asset_id.clone())),
            ("debt", Expr::int(base.debt)),
            ("assignedReserve", Expr::int(base.assigned_reserve)),
            ("reserveId", Expr::bytes(base.reserve_id.clone())),
            ("liquidationPrice", Expr::int(base.liquidation_price)),
            ("rewardPpm", Expr::int(base.challenge_reward_ppm)),
            ("challengePeriodDaa", Expr::int(base.challenge_period_daa)),
            ("auctionDurationDaa", Expr::int(base.auction_duration_daa)),
            ("collateralSompi", Expr::int(base.collateral_sompi)),
            (
                "kccPrefixLen",
                Expr::int(stack.base.kcc.prefix.len() as i64),
            ),
            (
                "kccSuffixLen",
                Expr::int(stack.base.kcc.suffix.len() as i64),
            ),
            (
                "kccTemplateHash",
                Expr::bytes(stack.base.kcc.template_hash.clone()),
            ),
        ],
    )
}

fn base_auction_state_expr(
    base: &ProtocolParams,
    stack: &SavingsStack,
    challenger: Vec<u8>,
    name: &'static str,
) -> Expr<'static> {
    struct_object(
        name,
        vec![
            ("positionOwner", Expr::bytes(base.owner.clone())),
            ("challenger", Expr::bytes(challenger)),
            ("positionId", Expr::bytes(base.position_id.clone())),
            ("assetId", Expr::bytes(base.asset_id.clone())),
            ("debt", Expr::int(base.debt)),
            ("assignedReserve", Expr::int(base.assigned_reserve)),
            ("reserveId", Expr::bytes(base.reserve_id.clone())),
            ("liquidationPrice", Expr::int(base.liquidation_price)),
            ("rewardPpm", Expr::int(base.challenge_reward_ppm)),
            ("auctionDurationDaa", Expr::int(base.auction_duration_daa)),
            ("collateralSompi", Expr::int(base.collateral_sompi)),
            (
                "kccPrefixLen",
                Expr::int(stack.base.kcc.prefix.len() as i64),
            ),
            (
                "kccSuffixLen",
                Expr::int(stack.base.kcc.suffix.len() as i64),
            ),
            (
                "kccTemplateHash",
                Expr::bytes(stack.base.kcc.template_hash.clone()),
            ),
        ],
    )
}

fn base_position_state_expr(
    base: &ProtocolParams,
    stack: &SavingsStack,
    challenge_id: Vec<u8>,
    name: &'static str,
) -> Expr<'static> {
    struct_object(
        name,
        vec![
            ("owner", Expr::bytes(base.owner.clone())),
            ("assetId", Expr::bytes(base.asset_id.clone())),
            ("debt", Expr::int(base.debt)),
            ("assignedReserve", Expr::int(base.assigned_reserve)),
            ("reserveId", Expr::bytes(base.reserve_id.clone())),
            (
                "reserveContributionPpm",
                Expr::int(base.reserve_contribution_ppm),
            ),
            ("liquidationPrice", Expr::int(base.liquidation_price)),
            ("challengePeriodDaa", Expr::int(base.challenge_period_daa)),
            ("auctionDurationDaa", Expr::int(base.auction_duration_daa)),
            ("challengeRewardPpm", Expr::int(base.challenge_reward_ppm)),
            ("challengeId", Expr::bytes(challenge_id)),
            (
                "kccPrefixLen",
                Expr::int(stack.base.kcc.prefix.len() as i64),
            ),
            (
                "kccSuffixLen",
                Expr::int(stack.base.kcc.suffix.len() as i64),
            ),
            (
                "kccTemplateHash",
                Expr::bytes(stack.base.kcc.template_hash.clone()),
            ),
            (
                "positionPrefixLen",
                Expr::int(stack.base.position.prefix.len() as i64),
            ),
            (
                "positionSuffixLen",
                Expr::int(stack.base.position.suffix.len() as i64),
            ),
            (
                "positionTemplateHash",
                Expr::bytes(stack.base.position.template_hash.clone()),
            ),
        ],
    )
}

pub fn base_start_challenge_call(
    base: &ProtocolParams,
    savings: &SavingsParams,
    challenger: Vec<u8>,
    challenge_id: Vec<u8>,
    challenge_output: u8,
) -> Result<Vec<u8>, String> {
    let stack = compile_stack(base, savings)?;
    let src = source("contracts/position.sil")?;
    let contract = compile_for_call(
        "Position",
        &src,
        &position_args(base, &stack.base, vec![0; 32]),
    )?;
    entry_call(
        &contract,
        "startChallengePolicy",
        vec![
            Expr::byte(0),
            base_position_state_expr(base, &stack, challenge_id, "State"),
            Expr::byte(challenge_output),
            base_challenge_state_expr(base, &stack, challenger, "ChallengeState"),
        ],
    )
}

pub fn base_anchor_init_call(
    base: &ProtocolParams,
    savings: &SavingsParams,
    challenger: Vec<u8>,
    challenge_output: u8,
) -> Result<Vec<u8>, String> {
    let stack = compile_stack(base, savings)?;
    let src = source("contracts/challenge-anchor.sil")?;
    let contract = compile_for_call("ChallengeAnchor", &src, &anchor_args(base, &stack.base))?;
    entry_call(
        &contract,
        "initPolicy",
        vec![
            Expr::byte(challenge_output),
            base_challenge_state_expr(base, &stack, challenger, "ChallengeState"),
        ],
    )
}

pub fn base_challenge_activate_call(
    base: &ProtocolParams,
    savings: &SavingsParams,
    challenger: Vec<u8>,
    auction_output: u8,
) -> Result<Vec<u8>, String> {
    let stack = compile_stack(base, savings)?;
    let src = source("contracts/challenge.sil")?;
    let contract = compile_for_call("Challenge", &src, &challenge_args(base, &stack.base))?;
    entry_call(
        &contract,
        "activatePolicy",
        vec![
            Expr::byte(auction_output),
            base_auction_state_expr(base, &stack, challenger, "AuctionState"),
        ],
    )
}

pub fn base_position_settle_auction_call(
    base: &ProtocolParams,
    savings: &SavingsParams,
    _challenger: Vec<u8>,
    challenge_id: Vec<u8>,
) -> Result<Vec<u8>, String> {
    let stack = compile_stack(base, savings)?;
    let src = source("contracts/challenged-position.sil")?;
    let contract = compile_for_call(
        "ChallengedPosition",
        &src,
        &challenged_position_args(base, &stack.base, challenge_id),
    )?;
    entry_call(&contract, "settleAuctionPolicy", vec![])
}

pub fn base_position_repay_call(
    base: &ProtocolParams,
    savings: &SavingsParams,
    repay_amount: i64,
) -> Result<Vec<u8>, String> {
    if repay_amount <= 0 || repay_amount > base.debt {
        return Err("invalid repayAmount".into());
    }
    let stack = compile_stack(base, savings)?;
    let src = source("contracts/position.sil")?;
    let contract = compile_for_call(
        "Position",
        &src,
        &position_args(base, &stack.base, vec![0; 32]),
    )?;
    let assigned_burn = if repay_amount == base.debt {
        base.assigned_reserve
    } else {
        (repay_amount / PPM) * base.reserve_contribution_ppm
            + (repay_amount % PPM) * base.reserve_contribution_ppm / PPM
    };
    let mut next = base.clone();
    next.debt = next
        .debt
        .checked_sub(repay_amount)
        .ok_or("Position debt underflow")?;
    next.assigned_reserve = next
        .assigned_reserve
        .checked_sub(assigned_burn)
        .ok_or("assigned reserve underflow")?;
    covenant_call(
        &contract,
        "repayPolicy",
        vec![
            base_position_state_expr(&next, &stack, vec![0; 32], "State"),
            Expr::int(repay_amount),
            token_state(
                "KCC20State",
                &TokenOutput {
                    owner: base.position_id.clone(),
                    identifier_type: 2,
                    amount: 0,
                    is_minter: true,
                },
            ),
            token_state(
                "KCC20State",
                &TokenOutput {
                    owner: base.position_id.clone(),
                    identifier_type: 2,
                    amount: next.assigned_reserve,
                    is_minter: false,
                },
            ),
        ],
        true,
    )
}

pub fn base_position_close_call(
    base: &ProtocolParams,
    savings: &SavingsParams,
    owner_signature: Vec<u8>,
) -> Result<Vec<u8>, String> {
    if owner_signature.len() != 65 {
        return Err("owner signature: 65 bytes required".into());
    }
    let stack = compile_stack(base, savings)?;
    let mut repaid = base.clone();
    repaid.debt = 0;
    repaid.assigned_reserve = 0;
    let src = source("contracts/position.sil")?;
    let contract = compile_for_call(
        "Position",
        &src,
        &position_args(&repaid, &stack.base, vec![0; 32]),
    )?;
    covenant_call(
        &contract,
        "closePolicy",
        vec![
            Expr::array(
                parse_type_ref("State[]").map_err(|e| e.to_string())?,
                vec![],
            ),
            Expr::bytes(owner_signature),
        ],
        true,
    )
}

pub fn base_challenged_position_avert_call(
    base: &ProtocolParams,
    savings: &SavingsParams,
    challenger: Vec<u8>,
    challenge_id: Vec<u8>,
    active_position_output: u8,
) -> Result<Vec<u8>, String> {
    let stack = compile_stack(base, savings)?;
    let src = source("contracts/challenged-position.sil")?;
    let contract = compile_for_call(
        "ChallengedPosition",
        &src,
        &challenged_position_args(base, &stack.base, challenge_id),
    )?;
    entry_call(
        &contract,
        "avertChallengePolicy",
        vec![
            Expr::byte(active_position_output),
            base_position_state_expr(base, &stack, vec![0; 32], "State"),
            Expr::dynamic_bytes(stack.base.position.prefix.clone()),
            Expr::dynamic_bytes(stack.base.position.suffix.clone()),
            base_challenge_state_expr(base, &stack, challenger, "ChallengeState"),
        ],
    )
}

pub fn base_challenge_avert_call(
    base: &ProtocolParams,
    savings: &SavingsParams,
    buyer: Vec<u8>,
    deposit_output: u8,
) -> Result<Vec<u8>, String> {
    let stack = compile_stack(base, savings)?;
    let src = source("contracts/challenge.sil")?;
    let contract = compile_for_call("Challenge", &src, &challenge_args(base, &stack.base))?;
    let price = i128::from(base.collateral_sompi)
        .checked_mul(i128::from(base.liquidation_price))
        .ok_or("overflow prix avert")?
        / 100_000_000;
    let price = i64::try_from(price).map_err(|_| "prix avert hors plage i64")?;
    covenant_call(
        &contract,
        "avertPolicy",
        vec![
            Expr::array(
                parse_type_ref("State[]").map_err(|e| e.to_string())?,
                vec![],
            ),
            Expr::bytes(buyer),
            Expr::byte(deposit_output),
            token_state(
                "TokenState",
                &TokenOutput {
                    owner: base.challenger.clone(),
                    identifier_type: 0,
                    amount: price,
                    is_minter: false,
                },
            ),
        ],
        true,
    )
}

pub fn base_auction_settle_call(
    base: &ProtocolParams,
    savings: &SavingsParams,
    bidder: Vec<u8>,
    bidder_collateral_output: u8,
    challenger_deposit_output: u8,
    elapsed_daa: i64,
) -> Result<Vec<u8>, String> {
    let stack = compile_stack(base, savings)?;
    let src = source("contracts/auction.sil")?;
    let contract = compile_for_call("Auction", &src, &auction_args(base, &stack.base))?;
    let reward = (base.debt / PPM) * base.challenge_reward_ppm
        + (base.debt % PPM) * base.challenge_reward_ppm / PPM;
    if !(0..=base.auction_duration_daa).contains(&elapsed_daa) {
        return Err("elapsed DAA exceeds the Auction duration".into());
    }
    let floor_payment = base.debt - base.assigned_reserve + reward;
    let collateral_price = (base.collateral_sompi as i128)
        .checked_mul(base.liquidation_price as i128)
        .ok_or("overflow prix Auction")?
        / 100_000_000;
    let start_payment = i64::try_from(collateral_price)
        .map_err(|_| "prix Auction hors i64")?
        .max(floor_payment);
    let discount = crate::equity::mul_div_floor(
        start_payment - floor_payment,
        elapsed_daa,
        base.auction_duration_daa,
    )?;
    let owner_surplus = start_payment - discount - floor_payment;
    covenant_call(
        &contract,
        "settlePolicy",
        vec![
            Expr::array(
                parse_type_ref("State[]").map_err(|e| e.to_string())?,
                vec![],
            ),
            Expr::bytes(bidder),
            Expr::byte(bidder_collateral_output),
            Expr::byte(challenger_deposit_output),
            token_state(
                "TokenState",
                &TokenOutput {
                    owner: base.challenger.clone(),
                    identifier_type: 0,
                    amount: reward,
                    is_minter: false,
                },
            ),
            token_state(
                "TokenState",
                &TokenOutput {
                    owner: base.owner.clone(),
                    identifier_type: 0,
                    amount: owner_surplus,
                    is_minter: false,
                },
            ),
        ],
        true,
    )
}

pub fn base_reserve_redeem_call(
    base: &ProtocolParams,
    savings: &SavingsParams,
    redeemer: Vec<u8>,
    burned_shares: i64,
    collateral_output: u8,
) -> Result<Vec<u8>, String> {
    if burned_shares <= 0 || burned_shares > base.total_kps || base.total_kps <= 0 {
        return Err("invalid burnedShares".into());
    }
    let stack = compile_stack(base, savings)?;
    let src = source("contracts/equity-reserve.sil")?;
    let contract = compile_for_call(
        "EquityReserve",
        &src,
        &reserve_args(
            base,
            savings,
            &stack.base.kcc,
            &stack.base.kps,
            &stack.base.auction,
            &stack.account,
            &stack.controller,
            &stack.registry,
        ),
    )?;
    let mul_div = |value: i64| -> Result<i64, String> {
        let result = i128::from(burned_shares)
            .checked_mul(i128::from(value))
            .ok_or("overflow calcul redeem")?
            / i128::from(base.total_kps);
        i64::try_from(result).map_err(|_| "redeem result exceeds the i64 range".into())
    };
    let payout = mul_div(base.reserve_kusd)?;
    let payout_kas = mul_div(base.reserve_collateral_sompi)?;
    if payout == 0 && payout_kas == 0 {
        return Err("redeem without payment".into());
    }
    let mut next = base.clone();
    next.reserve_kusd -= payout;
    next.total_kps -= burned_shares;
    next.reserve_collateral_sompi -= payout_kas;
    covenant_call(
        &contract,
        "redeemPolicy",
        vec![
            reserve_state_expr(&next, &stack, "State"),
            Expr::bytes(redeemer.clone()),
            Expr::int(burned_shares),
            token_state(
                "TokenState",
                &TokenOutput {
                    owner: base.reserve_id.clone(),
                    identifier_type: 2,
                    amount: next.reserve_kusd,
                    is_minter: false,
                },
            ),
            token_state(
                "TokenState",
                &TokenOutput {
                    owner: redeemer,
                    identifier_type: 0,
                    amount: payout,
                    is_minter: false,
                },
            ),
            token_state(
                "TokenState",
                &TokenOutput {
                    owner: base.reserve_id.clone(),
                    identifier_type: 2,
                    amount: 0,
                    is_minter: true,
                },
            ),
            Expr::byte(collateral_output),
        ],
        true,
    )
}

pub fn base_auction_backstop_call(
    base: &ProtocolParams,
    savings: &SavingsParams,
    challenger: Vec<u8>,
    next_reserve_kusd: i64,
    next_collateral_sompi: i64,
    reserve_input: u8,
    challenger_deposit_output: u8,
) -> Result<Vec<u8>, String> {
    let stack = compile_stack(base, savings)?;
    let src = source("contracts/auction.sil")?;
    let contract = compile_for_call("Auction", &src, &auction_args(base, &stack.base))?;
    let mut next = base.clone();
    next.reserve_kusd = next_reserve_kusd;
    next.reserve_collateral_sompi = next_collateral_sompi;
    let reward = base.debt * base.challenge_reward_ppm / PPM;
    covenant_call(
        &contract,
        "backstopPolicy",
        vec![
            Expr::array(
                parse_type_ref("State[]").map_err(|e| e.to_string())?,
                vec![],
            ),
            Expr::byte(reserve_input),
            reserve_state_expr(&next, &stack, "ReserveState"),
            Expr::byte(challenger_deposit_output),
            token_state(
                "TokenState",
                &TokenOutput {
                    owner: challenger,
                    identifier_type: 0,
                    amount: reward,
                    is_minter: false,
                },
            ),
        ],
        true,
    )
}

pub fn base_reserve_backstop_call(
    base: &ProtocolParams,
    savings: &SavingsParams,
    challenger: Vec<u8>,
    next_reserve_kusd: i64,
    next_collateral_sompi: i64,
    auction_input: u8,
) -> Result<Vec<u8>, String> {
    let stack = compile_stack(base, savings)?;
    let src = source("contracts/equity-reserve.sil")?;
    let contract = compile_for_call(
        "EquityReserve",
        &src,
        &reserve_args(
            base,
            savings,
            &stack.base.kcc,
            &stack.base.kps,
            &stack.base.auction,
            &stack.account,
            &stack.controller,
            &stack.registry,
        ),
    )?;
    let mut next = base.clone();
    next.reserve_kusd = next_reserve_kusd;
    next.reserve_collateral_sompi = next_collateral_sompi;
    let reward = base.debt * base.challenge_reward_ppm / PPM;
    covenant_call(
        &contract,
        "backstopPolicy",
        vec![
            reserve_state_expr(&next, &stack, "State"),
            Expr::byte(auction_input),
            base_auction_state_expr(base, &stack, challenger.clone(), "AuctionState"),
            token_state(
                "TokenState",
                &TokenOutput {
                    owner: base.reserve_id.clone(),
                    identifier_type: 2,
                    amount: next_reserve_kusd,
                    is_minter: false,
                },
            ),
            token_state(
                "TokenState",
                &TokenOutput {
                    owner: challenger,
                    identifier_type: 0,
                    amount: reward,
                    is_minter: false,
                },
            ),
        ],
        true,
    )
}

pub fn base_reserve_collect_proposal_fee_call(
    base: &ProtocolParams,
    savings: &SavingsParams,
    proposal_id: Vec<u8>,
    fee_amount: i64,
) -> Result<Vec<u8>, String> {
    if proposal_id.len() != 32 || fee_amount <= 0 {
        return Err("invalid proposal ID or proposal fee".into());
    }
    let stack = compile_stack(base, savings)?;
    let src = source("contracts/equity-reserve.sil")?;
    let contract = compile_for_call(
        "EquityReserve",
        &src,
        &reserve_args(
            base,
            savings,
            &stack.base.kcc,
            &stack.base.kps,
            &stack.base.auction,
            &stack.account,
            &stack.controller,
            &stack.registry,
        ),
    )?;
    let mut next = base.clone();
    next.reserve_kusd = next
        .reserve_kusd
        .checked_add(fee_amount)
        .ok_or("overflow proposal fee")?;
    covenant_call(
        &contract,
        "collectProposalFeePolicy",
        vec![
            reserve_state_expr(&next, &stack, "State"),
            Expr::bytes(proposal_id),
            Expr::int(fee_amount),
            token_state(
                "TokenState",
                &TokenOutput {
                    owner: base.reserve_id.clone(),
                    identifier_type: 2,
                    amount: next.reserve_kusd,
                    is_minter: false,
                },
            ),
        ],
        false,
    )
}

pub fn governor_propose_call(
    base: &ProtocolParams,
    prev: &SavingsParams,
    next: &SavingsParams,
    proposal_output: u8,
) -> Result<Vec<u8>, String> {
    let stack = compile_stack(base, prev)?;
    let src = source("contracts/savings-governor.sil")?;
    let contract = compile_for_call(
        "SavingsGovernor",
        &src,
        &governor_args(
            prev,
            base,
            &stack.proposal,
            &stack.base.kps,
            &stack.base.delegation,
            &stack.reserve,
        ),
    )?;
    covenant_call(
        &contract,
        "proposeSeriesPolicy",
        vec![
            governor_state_expr(base, next, &stack, "State"),
            Expr::byte(proposal_output),
            proposal_state_expr(base, next, false, "ProposalState"),
        ],
        true,
    )
}

pub fn proposal_activate_call(base: &ProtocolParams, p: &SavingsParams) -> Result<Vec<u8>, String> {
    let src = source("contracts/savings-proposal.sil")?;
    let contract = compile_for_call("SavingsProposal", &src, &proposal_args(p, base, false))?;
    covenant_call(
        &contract,
        "activatePolicy",
        vec![proposal_state_expr(base, p, true, "State")],
        true,
    )
}

pub fn governor_execute_call(
    base: &ProtocolParams,
    prev: &SavingsParams,
    next: &SavingsParams,
    proposal_input: u8,
) -> Result<Vec<u8>, String> {
    let stack = compile_stack(base, prev)?;
    let src = source("contracts/savings-governor.sil")?;
    let contract = compile_for_call(
        "SavingsGovernor",
        &src,
        &governor_args(
            prev,
            base,
            &stack.proposal,
            &stack.base.kps,
            &stack.base.delegation,
            &stack.reserve,
        ),
    )?;
    covenant_call(
        &contract,
        "executeSeriesPolicy",
        vec![
            governor_state_expr(base, next, &stack, "State"),
            Expr::byte(proposal_input),
            proposal_state_expr(base, prev, true, "ProposalState"),
        ],
        true,
    )
}

pub fn proposal_execute_call(
    base: &ProtocolParams,
    p: &SavingsParams,
    refund_output: u8,
) -> Result<Vec<u8>, String> {
    let src = source("contracts/savings-proposal.sil")?;
    let contract = compile_for_call("SavingsProposal", &src, &proposal_args(p, base, true))?;
    entry_call(&contract, "executePolicy", vec![Expr::byte(refund_output)])
}

pub fn controller_execute_call(
    base: &ProtocolParams,
    prev: &SavingsParams,
    next: &SavingsParams,
    governor_input: u8,
    proposal_input: u8,
) -> Result<Vec<u8>, String> {
    let stack = compile_stack(base, prev)?;
    let src = source("contracts/savings-controller.sil")?;
    let contract = compile_for_call(
        "SavingsController",
        &src,
        &controller_args(
            prev,
            base,
            &stack.base.kcc,
            &stack.account,
            &stack.governor,
            &stack.proposal,
            &stack.controller,
            &stack.reserve,
        ),
    )?;
    covenant_call(
        &contract,
        "executeSeriesPolicy",
        vec![
            controller_state_expr(base, next, &stack, "State"),
            Expr::byte(governor_input),
            Expr::byte(proposal_input),
        ],
        true,
    )
}

pub fn controller_initialize_governance_call(
    base: &ProtocolParams,
    prev: &SavingsParams,
    next: &SavingsParams,
    owner_signature: Vec<u8>,
) -> Result<Vec<u8>, String> {
    if prev.governor_id != vec![0; 32] || next.governor_id == vec![0; 32] {
        return Err("invalid Controller Governor initialization".into());
    }
    let stack = compile_stack(base, prev)?;
    let src = source("contracts/savings-controller.sil")?;
    let contract = compile_for_call(
        "SavingsController",
        &src,
        &controller_args(
            prev,
            base,
            &stack.base.kcc,
            &stack.account,
            &stack.governor,
            &stack.proposal,
            &stack.controller,
            &stack.reserve,
        ),
    )?;
    covenant_call(
        &contract,
        "initializeGovernancePolicy",
        vec![
            controller_state_expr(base, next, &stack, "State"),
            Expr::bytes(owner_signature),
        ],
        true,
    )
}

pub fn controller_open_call(
    base: &ProtocolParams,
    prev: &SavingsParams,
    next: &SavingsParams,
    account: &SavingsParams,
    account_output: u8,
    account_id: Vec<u8>,
) -> Result<Vec<u8>, String> {
    if account_id.len() != 32 {
        return Err("account_id: 32 bytes required".into());
    }
    let stack = compile_stack(base, prev)?;
    let src = source("contracts/savings-controller.sil")?;
    let contract = compile_for_call(
        "SavingsController",
        &src,
        &controller_args(
            prev,
            base,
            &stack.base.kcc,
            &stack.account,
            &stack.governor,
            &stack.proposal,
            &stack.controller,
            &stack.reserve,
        ),
    )?;
    covenant_call(
        &contract,
        "openAccountPolicy",
        vec![
            controller_state_expr(base, next, &stack, "State"),
            Expr::byte(account_output),
            Expr::dynamic_bytes(stack.account.prefix.clone()),
            Expr::dynamic_bytes(stack.account.suffix.clone()),
            account_state_expr(base, account, &stack, "AccountState"),
            Expr::bytes(account.owner.clone()),
            Expr::int(account.saved),
            token_state(
                "TokenState",
                &TokenOutput {
                    owner: account_id,
                    identifier_type: 2,
                    amount: account.saved,
                    is_minter: false,
                },
            ),
        ],
        true,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn account_refresh_call(
    base: &ProtocolParams,
    prev: &SavingsParams,
    next: &SavingsParams,
    owner_signature: Vec<u8>,
    accrual_daa: i64,
    controller_input: u8,
    reserve_input: u8,
    account_token_input: u8,
    reserve_token_input: u8,
    account_id: Vec<u8>,
    referral_amount: i64,
) -> Result<Vec<u8>, String> {
    let stack = compile_stack(base, prev)?;
    let src = source("contracts/savings-account.sil")?;
    let contract = compile_for_call(
        "SavingsAccount",
        &src,
        &account_args(
            prev,
            base,
            &stack.base.kcc,
            &stack.controller,
            &stack.reserve,
        ),
    )?;
    covenant_call(
        &contract,
        "refreshPolicy",
        vec![
            account_state_expr(base, next, &stack, "State"),
            Expr::bytes(if owner_signature.is_empty() {
                vec![0; 65]
            } else {
                owner_signature
            }),
            Expr::int(accrual_daa),
            Expr::byte(controller_input),
            Expr::byte(reserve_input),
            Expr::byte(account_token_input),
            Expr::byte(reserve_token_input),
            token_state(
                "TokenState",
                &TokenOutput {
                    owner: account_id,
                    identifier_type: 2,
                    amount: next.saved,
                    is_minter: false,
                },
            ),
            token_state(
                "TokenState",
                &TokenOutput {
                    owner: prev.referrer.clone(),
                    identifier_type: 0,
                    amount: referral_amount,
                    is_minter: false,
                },
            ),
        ],
        true,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn controller_refresh_call(
    base: &ProtocolParams,
    prev: &SavingsParams,
    next: &SavingsParams,
    next_account: &SavingsParams,
    account_input: u8,
) -> Result<Vec<u8>, String> {
    let stack = compile_stack(base, prev)?;
    let src = source("contracts/savings-controller.sil")?;
    let contract = compile_for_call(
        "SavingsController",
        &src,
        &controller_args(
            prev,
            base,
            &stack.base.kcc,
            &stack.account,
            &stack.governor,
            &stack.proposal,
            &stack.controller,
            &stack.reserve,
        ),
    )?;
    covenant_call(
        &contract,
        "accountRefreshPolicy",
        vec![
            controller_state_expr(base, next, &stack, "State"),
            Expr::byte(account_input),
            account_state_expr(base, next_account, &stack, "AccountState"),
        ],
        true,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn reserve_refresh_call(
    base: &ProtocolParams,
    prev: &SavingsParams,
    next_base: &ProtocolParams,
    next_account: &SavingsParams,
    registry_input: u8,
    controller_input: u8,
    account_input: u8,
    accrual_daa: i64,
    account_token_input: u8,
    reserve_token_input: u8,
) -> Result<Vec<u8>, String> {
    let stack = compile_stack(base, prev)?;
    let next_stack = compile_stack(next_base, next_account)?;
    let src = source("contracts/equity-reserve.sil")?;
    let contract = compile_for_call(
        "EquityReserve",
        &src,
        &reserve_args(
            base,
            prev,
            &stack.base.kcc,
            &stack.base.kps,
            &stack.base.auction,
            &stack.account,
            &stack.controller,
            &stack.registry,
        ),
    )?;
    covenant_call(
        &contract,
        "paySavingsPolicy",
        vec![
            reserve_state_expr(next_base, &next_stack, "State"),
            Expr::byte(registry_input),
            Expr::byte(controller_input),
            Expr::byte(account_input),
            account_state_expr(base, next_account, &stack, "AccountState"),
            Expr::int(accrual_daa),
            Expr::byte(account_token_input),
            Expr::byte(reserve_token_input),
            token_state(
                "TokenState",
                &TokenOutput {
                    owner: base.reserve_id.clone(),
                    identifier_type: 2,
                    amount: next_base.reserve_kusd,
                    is_minter: false,
                },
            ),
        ],
        true,
    )
}

pub fn controller_close_call(
    base: &ProtocolParams,
    prev: &SavingsParams,
    next: &SavingsParams,
    account_input: u8,
) -> Result<Vec<u8>, String> {
    let stack = compile_stack(base, prev)?;
    let src = source("contracts/savings-controller.sil")?;
    let contract = compile_for_call(
        "SavingsController",
        &src,
        &controller_args(
            prev,
            base,
            &stack.base.kcc,
            &stack.account,
            &stack.governor,
            &stack.proposal,
            &stack.controller,
            &stack.reserve,
        ),
    )?;
    covenant_call(
        &contract,
        "accountClosePolicy",
        vec![
            controller_state_expr(base, next, &stack, "State"),
            Expr::byte(account_input),
        ],
        true,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn account_close_call(
    base: &ProtocolParams,
    prev: &SavingsParams,
    next_controller: &SavingsParams,
    owner_signature: Vec<u8>,
    controller_input: u8,
    account_token_input: u8,
    kas_output: u8,
) -> Result<Vec<u8>, String> {
    let stack = compile_stack(base, prev)?;
    let src = source("contracts/savings-account.sil")?;
    let contract = compile_for_call(
        "SavingsAccount",
        &src,
        &account_args(
            prev,
            base,
            &stack.base.kcc,
            &stack.controller,
            &stack.reserve,
        ),
    )?;
    covenant_call(
        &contract,
        "closePolicy",
        vec![
            Expr::array(
                parse_type_ref("State[]").map_err(|e| e.to_string())?,
                vec![],
            ),
            Expr::bytes(if owner_signature.is_empty() {
                vec![0; 65]
            } else {
                owner_signature
            }),
            Expr::byte(controller_input),
            controller_state_expr(base, next_controller, &stack, "ControllerState"),
            Expr::byte(account_token_input),
            token_state(
                "TokenState",
                &TokenOutput {
                    owner: prev.owner.clone(),
                    identifier_type: 0,
                    amount: prev.saved,
                    is_minter: false,
                },
            ),
            Expr::byte(kas_output),
        ],
        true,
    )
}

pub fn compile_full_stack(
    base: &ProtocolParams,
    savings: &SavingsParams,
    governance: &GovernanceParams,
) -> Result<FullStack, String> {
    let savings_stack = compile_stack(base, savings)?;
    let base_governance =
        compile_governance_stack_with_base(base, governance, savings_stack.base.clone())?;
    Ok(FullStack {
        savings: savings_stack,
        base_governance,
    })
}

pub fn compile_base_anchor_state(
    base: &ProtocolParams,
    savings: &SavingsParams,
) -> Result<Artifact, String> {
    Ok(compile_stack(base, savings)?.base.anchor)
}

pub fn compile_base_challenge_state(
    base: &ProtocolParams,
    savings: &SavingsParams,
) -> Result<Artifact, String> {
    Ok(compile_stack(base, savings)?.base.challenge)
}

pub fn compile_base_auction_state(
    base: &ProtocolParams,
    savings: &SavingsParams,
) -> Result<Artifact, String> {
    Ok(compile_stack(base, savings)?.base.auction)
}

pub fn compile_base_root_state(
    base: &ProtocolParams,
    savings: &SavingsParams,
    governance: &GovernanceParams,
    initialized: bool,
    module_nonce: i64,
    remaining_allocation: i64,
    authority: Vec<u8>,
    authority_type: u8,
) -> Result<Artifact, String> {
    let stack = compile_full_stack(base, savings, governance)?;
    compile(
        "contracts/root-issuance.sil",
        &root_args_with_state(
            base,
            &stack.base_governance,
            initialized,
            module_nonce,
            remaining_allocation,
            authority,
            authority_type,
        ),
    )
}

pub fn compile_base_module_state(
    base: &ProtocolParams,
    savings: &SavingsParams,
    remaining_mint: i64,
    position_nonce: i64,
) -> Result<Artifact, String> {
    let mut state = base.clone();
    state.module_remaining_mint = remaining_mint;
    state.position_nonce = position_nonce;
    let stack = compile_stack(&state, savings)?;
    compile(
        "contracts/minting-module.sil",
        &module_args(&state, &stack.base),
    )
}

pub fn compile_base_position_state(
    base: &ProtocolParams,
    savings: &SavingsParams,
    debt: i64,
    assigned_reserve: i64,
    challenge_id: Vec<u8>,
) -> Result<Artifact, String> {
    let mut state = base.clone();
    state.debt = debt;
    state.assigned_reserve = assigned_reserve;
    let stack = compile_stack(base, savings)?;
    compile(
        "contracts/position.sil",
        &position_args(&state, &stack.base, challenge_id),
    )
}

pub fn compile_base_challenged_position_state(
    base: &ProtocolParams,
    savings: &SavingsParams,
    debt: i64,
    assigned_reserve: i64,
    challenge_id: Vec<u8>,
) -> Result<Artifact, String> {
    let mut state = base.clone();
    state.debt = debt;
    state.assigned_reserve = assigned_reserve;
    let stack = compile_stack(base, savings)?;
    compile(
        "contracts/challenged-position.sil",
        &challenged_position_args(&state, &stack.base, challenge_id),
    )
}

pub fn compile_base_governance_state(
    base: &ProtocolParams,
    savings: &SavingsParams,
    governance: &GovernanceParams,
    active_proposal_id: Vec<u8>,
) -> Result<Artifact, String> {
    if active_proposal_id.len() != 32 {
        return Err("active_proposal_id: 32 bytes required".into());
    }
    let stack = compile_full_stack(base, savings, governance)?;
    compile(
        "contracts/governance.sil",
        &governance_args(
            base,
            governance,
            &stack.base_governance.proposal,
            active_proposal_id,
            &stack.savings.base,
        ),
    )
}

pub fn compile_base_proposal_state(
    base: &ProtocolParams,
    savings: &SavingsParams,
    governance: &GovernanceParams,
    activated: bool,
) -> Result<Artifact, String> {
    let stack = compile_full_stack(base, savings, governance)?;
    compile(
        "contracts/module-proposal.sil",
        &proposal_args_with_activation(base, governance, &stack.savings.base, activated),
    )
}

pub fn compile_base_kps_state(
    base: &ProtocolParams,
    savings: &SavingsParams,
    owner: Vec<u8>,
    amount: i64,
    identifier_type: u8,
    is_minter: bool,
) -> Result<Artifact, String> {
    let stack = compile_stack(base, savings)?;
    compile(
        "contracts/kps.sil",
        &crate::protocol::kps_args(
            base,
            &stack.base.delegation,
            owner,
            amount,
            identifier_type,
            is_minter,
        ),
    )
}

pub fn base_root_init_call(
    base: &ProtocolParams,
    savings: &SavingsParams,
    governance: &GovernanceParams,
    current_authority: Vec<u8>,
    next_asset_id: Vec<u8>,
    root_allocation: i64,
) -> Result<Vec<u8>, String> {
    let stack = compile_full_stack(base, savings, governance)?;
    let mut previous = base.clone();
    previous.asset_id = vec![0; 32];
    let src = source("contracts/root-issuance.sil")?;
    let contract = compile_for_call(
        "RootIssuance",
        &src,
        &root_args_with_state(
            &previous,
            &stack.base_governance,
            false,
            0,
            0,
            current_authority.clone(),
            0,
        ),
    )?;
    let next = struct_object(
        "State",
        vec![
            ("assetId", Expr::bytes(next_asset_id)),
            ("initialized", Expr::bool(true)),
            ("moduleNonce", Expr::int(0)),
            ("remainingAllocation", Expr::int(root_allocation)),
            ("authorityIdentifier", Expr::bytes(current_authority)),
            ("authorityType", Expr::byte(0)),
        ],
    );
    covenant_call(&contract, "initPolicy", vec![next], true)
}

#[allow(clippy::too_many_arguments)]
pub fn base_root_bootstrap_module_call(
    base: &ProtocolParams,
    savings: &SavingsParams,
    governance: &GovernanceParams,
    current_authority: Vec<u8>,
    authority_signature: Vec<u8>,
    root_id: Vec<u8>,
    module_id: Vec<u8>,
    previous_module_nonce: i64,
    previous_remaining_allocation: i64,
    module_output_index: u8,
) -> Result<Vec<u8>, String> {
    let stack = compile_full_stack(base, savings, governance)?;
    let src = source("contracts/root-issuance.sil")?;
    let contract = compile_for_call(
        "RootIssuance",
        &src,
        &root_args_with_state(
            base,
            &stack.base_governance,
            true,
            previous_module_nonce,
            previous_remaining_allocation,
            current_authority.clone(),
            0,
        ),
    )?;
    let next_root = struct_object(
        "State",
        vec![
            ("assetId", Expr::bytes(base.asset_id.clone())),
            ("initialized", Expr::bool(true)),
            ("moduleNonce", Expr::int(previous_module_nonce + 1)),
            (
                "remainingAllocation",
                Expr::int(previous_remaining_allocation - base.module_remaining_mint),
            ),
            ("authorityIdentifier", Expr::bytes(current_authority)),
            ("authorityType", Expr::byte(0)),
        ],
    );
    covenant_call(
        &contract,
        "createModulePolicy",
        vec![
            next_root,
            Expr::bytes(authority_signature),
            Expr::byte(module_output_index),
            Expr::dynamic_bytes(stack.savings.base.module.prefix.clone()),
            Expr::dynamic_bytes(stack.savings.base.module.suffix.clone()),
            module_state_expr(base, "ModuleState"),
            governance_state_expr(base, governance, vec![0; 32], "GovernanceState"),
            token_state_expr(root_id, 2, 0, true, "KCC20State"),
            token_state_expr(module_id, 2, 0, true, "KCC20State"),
        ],
        true,
    )
}

pub fn base_module_open_call(
    base: &ProtocolParams,
    savings: &SavingsParams,
    position_id: Vec<u8>,
    position_output_index: u8,
) -> Result<Vec<u8>, String> {
    if position_id.len() != 32 {
        return Err("position_id: 32 bytes required".into());
    }
    let stack = compile_stack(base, savings)?;
    let src = source("contracts/minting-module.sil")?;
    let contract = compile_for_call("MintingModule", &src, &module_args(base, &stack.base))?;
    let quote = crate::fees::MintTerms {
        reserve_contribution_ppm: base.reserve_contribution_ppm,
        annual_interest_ppm: base.risk_premium_ppm,
        daa_per_year: base.daa_per_year,
    }
    .quote(base.debt, base.module_expiration_daa - base.current_daa)
    .map_err(str::to_string)?;
    let contribution = quote.assigned_reserve_kusd;
    let fee = quote.equity_fee_kusd;
    let usable = base
        .debt
        .checked_sub(contribution)
        .and_then(|v| v.checked_sub(fee))
        .ok_or("underflow mint utilisable")?;
    if contribution != base.assigned_reserve || usable <= 0 {
        return Err("assignedReserve incompatible".into());
    }
    let mut next = base.clone();
    next.module_remaining_mint -= base.debt;
    next.position_nonce += 1;
    covenant_call(
        &contract,
        "openPolicy",
        vec![
            module_state_expr(&next, "State"),
            Expr::byte(position_output_index),
            Expr::dynamic_bytes(stack.base.position.prefix.clone()),
            Expr::dynamic_bytes(stack.base.position.suffix.clone()),
            position_state_for_module(base, &stack.base, position_id.clone(), "PositionState"),
            token_state_expr(base.module_id.clone(), 2, 0, true, "KCC20State"),
            token_state_expr(position_id.clone(), 2, 0, true, "KCC20State"),
            token_state_expr(base.owner.clone(), 0, usable, false, "KCC20State"),
            token_state_expr(position_id, 2, contribution, false, "KCC20State"),
            token_state_expr(base.reserve_id.clone(), 2, fee, false, "KCC20State"),
        ],
        true,
    )
}

pub fn base_reserve_initialize_call(
    base: &ProtocolParams,
    savings: &SavingsParams,
    reserve_id: Vec<u8>,
    depositor: Vec<u8>,
    deposit_amount: i64,
) -> Result<Vec<u8>, String> {
    let stack = compile_stack(base, savings)?;
    let mut previous = base.clone();
    previous.kps_id = vec![0; 32];
    previous.reserve_kusd = 0;
    previous.total_kps = 0;
    previous.reserve_collateral_sompi = 0;
    let src = source("contracts/equity-reserve.sil")?;
    let contract = compile_for_call(
        "EquityReserve",
        &src,
        &reserve_args(
            &previous,
            savings,
            &stack.base.kcc,
            &stack.base.kps,
            &stack.base.auction,
            &stack.account,
            &stack.controller,
            &stack.registry,
        ),
    )?;
    covenant_call(
        &contract,
        "initializePolicy",
        vec![
            reserve_state_expr(base, &stack, "State"),
            Expr::bytes(depositor.clone()),
            Expr::int(deposit_amount),
            token_state_expr(reserve_id.clone(), 2, deposit_amount, false, "TokenState"),
            token_state_expr(reserve_id, 2, 0, true, "TokenState"),
            token_state_expr(depositor, 0, deposit_amount, false, "TokenState"),
        ],
        true,
    )
}

pub fn base_governance_propose_call(
    base: &ProtocolParams,
    savings: &SavingsParams,
    governance: &GovernanceParams,
    proposal_id: Vec<u8>,
    proposal_output_index: u8,
) -> Result<Vec<u8>, String> {
    let stack = compile_full_stack(base, savings, governance)?;
    let src = source("contracts/governance.sil")?;
    let contract = compile_for_call(
        "Governance",
        &src,
        &governance_args(
            base,
            governance,
            &stack.base_governance.proposal,
            vec![0; 32],
            &stack.savings.base,
        ),
    )?;
    let mut next = governance.clone();
    next.proposal_nonce += 1;
    covenant_call(
        &contract,
        "proposeModulePolicy",
        vec![
            governance_state_expr(base, &next, proposal_id.clone(), "State"),
            Expr::byte(proposal_output_index),
            module_proposal_state_expr(base, &next, false, "ProposalState"),
            token_state_expr(
                proposal_id,
                2,
                governance.proposal_fee_kusd,
                false,
                "TokenState",
            ),
        ],
        true,
    )
}

pub fn base_proposal_activate_call(
    base: &ProtocolParams,
    savings: &SavingsParams,
    governance: &GovernanceParams,
) -> Result<Vec<u8>, String> {
    let stack = compile_full_stack(base, savings, governance)?;
    let src = source("contracts/module-proposal.sil")?;
    let contract = compile_for_call(
        "ModuleProposal",
        &src,
        &proposal_args_with_activation(base, governance, &stack.savings.base, false),
    )?;
    covenant_call(
        &contract,
        "activatePolicy",
        vec![module_proposal_state_expr(base, governance, true, "State")],
        true,
    )
}

pub fn base_governance_execute_call(
    base: &ProtocolParams,
    savings: &SavingsParams,
    governance: &GovernanceParams,
    active_proposal_id: Vec<u8>,
) -> Result<Vec<u8>, String> {
    let stack = compile_full_stack(base, savings, governance)?;
    let src = source("contracts/governance.sil")?;
    let contract = compile_for_call(
        "Governance",
        &src,
        &governance_args(
            base,
            governance,
            &stack.base_governance.proposal,
            active_proposal_id,
            &stack.savings.base,
        ),
    )?;
    let mut next = governance.clone();
    next.execution_nonce += 1;
    covenant_call(
        &contract,
        "executeModulePolicy",
        vec![governance_state_expr(base, &next, vec![0; 32], "State")],
        true,
    )
}

pub fn base_proposal_execute_call(
    base: &ProtocolParams,
    savings: &SavingsParams,
    governance: &GovernanceParams,
    refund_output: u8,
    fee_input: u8,
    fee_output: u8,
) -> Result<Vec<u8>, String> {
    let stack = compile_full_stack(base, savings, governance)?;
    let src = source("contracts/module-proposal.sil")?;
    let contract = compile_for_call(
        "ModuleProposal",
        &src,
        &proposal_args_with_activation(base, governance, &stack.savings.base, true),
    )?;
    entry_call(
        &contract,
        "executePolicy",
        vec![
            Expr::byte(refund_output),
            token_state_expr(
                governance.proposer.clone(),
                0,
                governance.proposal_fee_kusd,
                false,
                "TokenState",
            ),
            Expr::byte(fee_input),
            Expr::byte(fee_output),
        ],
    )
}

pub fn base_root_handover_call(
    base: &ProtocolParams,
    savings: &SavingsParams,
    governance: &GovernanceParams,
    current_authority: Vec<u8>,
    signature: Vec<u8>,
    module_nonce: i64,
    remaining: i64,
) -> Result<Vec<u8>, String> {
    let stack = compile_full_stack(base, savings, governance)?;
    let src = source("contracts/root-issuance.sil")?;
    let contract = compile_for_call(
        "RootIssuance",
        &src,
        &root_args(
            base,
            &stack.base_governance,
            module_nonce,
            remaining,
            current_authority,
            0,
        ),
    )?;
    covenant_call(
        &contract,
        "handoverPolicy",
        vec![
            root_state_expr(base, governance, module_nonce, remaining, "State"),
            Expr::bytes(signature),
            Expr::bytes(governance.governance_id.clone()),
            Expr::byte(4),
        ],
        true,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn base_root_execute_module_call(
    base: &ProtocolParams,
    savings: &SavingsParams,
    governance: &GovernanceParams,
    root_id: Vec<u8>,
    module_id: Vec<u8>,
    previous_nonce: i64,
    previous_remaining: i64,
    module_output: u8,
) -> Result<Vec<u8>, String> {
    let stack = compile_full_stack(base, savings, governance)?;
    let src = source("contracts/root-issuance.sil")?;
    let contract = compile_for_call(
        "RootIssuance",
        &src,
        &root_args(
            base,
            &stack.base_governance,
            previous_nonce,
            previous_remaining,
            governance.governance_id.clone(),
            4,
        ),
    )?;
    let remaining = previous_remaining
        .checked_sub(base.module_remaining_mint)
        .ok_or("Root allocation underflow")?;
    let mut executed = governance.clone();
    executed.execution_nonce += 1;
    covenant_call(
        &contract,
        "createModulePolicy",
        vec![
            root_state_expr(base, governance, previous_nonce + 1, remaining, "State"),
            Expr::bytes(vec![0; 65]),
            Expr::byte(module_output),
            Expr::dynamic_bytes(stack.savings.base.module.prefix.clone()),
            Expr::dynamic_bytes(stack.savings.base.module.suffix.clone()),
            module_state_expr(base, "ModuleState"),
            governance_state_expr(base, &executed, vec![0; 32], "GovernanceState"),
            token_state_expr(root_id, 2, 0, true, "KCC20State"),
            token_state_expr(module_id, 2, 0, true, "KCC20State"),
        ],
        true,
    )
}
