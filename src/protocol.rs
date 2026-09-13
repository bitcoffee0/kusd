use crate::artifact::Artifact;
use crate::silverscript::{
    CompileOptions, CompiledContract, CovenantDeclCallOptions, compile_contract, struct_object,
};
use kaspa_txscript::{EngineFlags, script_builder::ScriptBuilder};
use serde::{Deserialize, Serialize};
use silverscript_lang::ast::{Expr, parse_type_ref};
use std::fs;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProtocolParams {
    pub owner: Vec<u8>,
    pub challenger: Vec<u8>,
    pub asset_id: Vec<u8>,
    pub kps_id: Vec<u8>,
    pub reserve_id: Vec<u8>,
    pub module_id: Vec<u8>,
    pub position_id: Vec<u8>,
    pub debt: i64,
    pub assigned_reserve: i64,
    pub reserve_contribution_ppm: i64,
    pub risk_premium_ppm: i64,
    pub daa_per_year: i64,
    pub liquidation_price: i64,
    pub challenge_period_daa: i64,
    pub auction_duration_daa: i64,
    pub challenge_reward_ppm: i64,
    pub collateral_sompi: i64,
    pub minimum_collateral_sompi: i64,
    pub current_daa: i64,
    pub reserve_kusd: i64,
    pub total_kps: i64,
    pub reserve_collateral_sompi: i64,
    pub minimum_kps_holding_daa: i64,
    pub max_kps_vote_weight: i64,
    pub module_remaining_mint: i64,
    pub max_debt_per_position: i64,
    pub module_expiration_daa: i64,
    pub position_nonce: i64,
}

#[derive(Clone, Debug)]
pub struct ProtocolStack {
    pub kcc: Artifact,
    pub kps: Artifact,
    pub delegation: Artifact,
    pub reserve: Artifact,
    pub auction: Artifact,
    pub challenge: Artifact,
    pub anchor: Artifact,
    pub challenged_position: Artifact,
    pub position: Artifact,
    pub module: Artifact,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GovernanceParams {
    pub proposer: Vec<u8>,
    pub governance_id: Vec<u8>,
    pub root_id: Vec<u8>,
    pub proposal_nonce: i64,
    pub execution_nonce: i64,
    pub voting_delay_daa: i64,
    pub execution_window_daa: i64,
    pub veto_threshold_ppm: i64,
    pub proposal_deposit_sompi: i64,
    pub proposal_fee_kusd: i64,
    pub min_module_allocation: i64,
    pub max_module_allocation: i64,
    pub max_debt_per_position: i64,
    pub min_collateral_sompi: i64,
    pub max_collateral_sompi: i64,
    pub min_module_duration_daa: i64,
    pub max_module_duration_daa: i64,
    pub min_challenge_period_daa: i64,
    pub max_challenge_period_daa: i64,
    pub min_auction_duration_daa: i64,
    pub max_auction_duration_daa: i64,
    pub max_risk_premium_ppm: i64,
    pub min_reserve_contribution_ppm: i64,
    pub max_reserve_contribution_ppm: i64,
    pub root_remaining_allocation: i64,
}

#[derive(Clone, Debug)]
pub struct GovernanceStack {
    pub base: ProtocolStack,
    pub proposal: Artifact,
    pub governance: Artifact,
    pub root: Artifact,
}

pub(crate) fn source(path: &str) -> Result<String, String> {
    fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))
}

pub(crate) fn compile(path: &str, args: &[Expr<'_>]) -> Result<Artifact, String> {
    let source = source(path)?;
    compile_contract(&source, args, CompileOptions::default())
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

pub(crate) fn empty_artifact() -> Artifact {
    Artifact {
        bytecode: vec![],
        prefix: vec![],
        suffix: vec![],
        template_hash: vec![0; 32],
    }
}

pub(crate) fn covenant_call(
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

pub(crate) fn entry_call(
    contract: &CompiledContract<'_>,
    entry: &str,
    args: Vec<Expr<'_>>,
) -> Result<Vec<u8>, String> {
    let mut script = contract
        .build_sig_script(entry, args)
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

pub fn weighted_kps_votes(amount: i64, weight: i64, max_weight: i64) -> Result<i64, String> {
    if amount <= 0 || weight <= 0 || max_weight <= 0 || weight > max_weight || max_weight > 16 {
        return Err("invalid KPS amount or weight".into());
    }
    amount
        .checked_mul(weight)
        .ok_or_else(|| "KPS voting power overflow".into())
}

pub fn required_weighted_kps_votes(
    total_kps: i64,
    max_weight: i64,
    threshold_ppm: i64,
) -> Result<i64, String> {
    if total_kps <= 0
        || !(1..=16).contains(&max_weight)
        || !(1..=1_000_000).contains(&threshold_ppm)
    {
        return Err("invalid KPS threshold parameters".into());
    }
    let weighted_supply = total_kps
        .checked_mul(max_weight)
        .ok_or_else(|| "weighted KPS supply overflow".to_string())?;
    let quotient = weighted_supply / 1_000_000;
    let remainder = weighted_supply % 1_000_000;
    let floor = quotient
        .checked_mul(threshold_ppm)
        .and_then(|v| v.checked_add(remainder * threshold_ppm / 1_000_000))
        .ok_or_else(|| "KPS threshold overflow".to_string())?;
    Ok(floor + i64::from(remainder * threshold_ppm % 1_000_000 != 0))
}

fn validate(p: &ProtocolParams) -> Result<(), String> {
    for (name, value) in [
        ("owner", &p.owner),
        ("challenger", &p.challenger),
        ("asset_id", &p.asset_id),
        ("kps_id", &p.kps_id),
        ("reserve_id", &p.reserve_id),
        ("module_id", &p.module_id),
        ("position_id", &p.position_id),
    ] {
        if value.len() != 32 {
            return Err(format!("{name}: 32 bytes required"));
        }
    }
    if p.debt <= 0
        || p.assigned_reserve < 0
        || p.assigned_reserve > p.debt
        || p.reserve_contribution_ppm < 0
        || !(0..=1_000_000).contains(&p.risk_premium_ppm)
        || p.daa_per_year <= 0
        || p.reserve_contribution_ppm >= 1_000_000
        || p.liquidation_price <= 0
        || p.challenge_period_daa <= 0
        || p.auction_duration_daa <= 0
        || !(1..=100_000).contains(&p.challenge_reward_ppm)
        || p.collateral_sompi <= 0
        || p.minimum_collateral_sompi <= 0
        || p.collateral_sompi < p.minimum_collateral_sompi
        || p.current_daa <= 0
        || p.reserve_kusd < 0
        || p.total_kps < 0
        || p.reserve_collateral_sompi < 0
        || p.minimum_kps_holding_daa <= 0
        || !(1..=16).contains(&p.max_kps_vote_weight)
        || p.module_remaining_mint < p.debt
        || p.max_debt_per_position < p.debt
        || p.module_expiration_daa <= 0
        || p.position_nonce < 0
    {
        return Err("invalid economic parameters".into());
    }
    Ok(())
}

pub fn kcc_args(owner: Vec<u8>, amount: i64, kind: u8, minter: bool) -> Vec<Expr<'static>> {
    vec![
        Expr::bytes(owner),
        Expr::int(amount),
        Expr::byte(kind),
        Expr::bool(minter),
        Expr::int(8),
        Expr::int(8),
    ]
}

pub fn kps_args(
    p: &ProtocolParams,
    delegation: &Artifact,
    owner: Vec<u8>,
    amount: i64,
    kind: u8,
    minter: bool,
) -> Vec<Expr<'static>> {
    vec![
        Expr::bytes(owner),
        Expr::int(amount),
        Expr::byte(kind),
        Expr::bool(minter),
        Expr::int(8),
        Expr::int(8),
        Expr::int(p.minimum_kps_holding_daa),
        Expr::dynamic_bytes(delegation.prefix.clone()),
        Expr::dynamic_bytes(delegation.suffix.clone()),
        Expr::bytes(delegation.template_hash.clone()),
    ]
}

pub fn delegation_args(
    p: &ProtocolParams,
    owner: Vec<u8>,
    delegate: Vec<u8>,
    amount: i64,
    weight: i64,
) -> Vec<Expr<'static>> {
    vec![
        Expr::bytes(owner),
        Expr::bytes(delegate),
        Expr::bytes(p.kps_id.clone()),
        Expr::int(amount),
        Expr::int(weight),
        Expr::int(p.minimum_kps_holding_daa),
        Expr::int(p.max_kps_vote_weight),
    ]
}

pub fn reserve_args(
    p: &ProtocolParams,
    stack: &ProtocolStack,
    reserve_kusd: i64,
    total_kps: i64,
    collateral_sompi: i64,
) -> Vec<Expr<'static>> {
    vec![
        Expr::bytes(p.asset_id.clone()),
        Expr::bytes(p.kps_id.clone()),
        Expr::int(reserve_kusd),
        Expr::int(total_kps),
        Expr::int(collateral_sompi),
        Expr::int(stack.kcc.prefix.len() as i64),
        Expr::int(stack.kcc.suffix.len() as i64),
        Expr::bytes(stack.kcc.template_hash.clone()),
        Expr::int(stack.kps.prefix.len() as i64),
        Expr::int(stack.kps.suffix.len() as i64),
        Expr::bytes(stack.kps.template_hash.clone()),
        Expr::int(stack.auction.prefix.len() as i64),
        Expr::int(stack.auction.suffix.len() as i64),
        Expr::bytes(stack.auction.template_hash.clone()),
    ]
}

pub fn auction_args(p: &ProtocolParams, stack: &ProtocolStack) -> Vec<Expr<'static>> {
    vec![
        Expr::bytes(p.owner.clone()),
        Expr::bytes(p.challenger.clone()),
        Expr::bytes(p.position_id.clone()),
        Expr::bytes(p.asset_id.clone()),
        Expr::int(p.debt),
        Expr::int(p.assigned_reserve),
        Expr::bytes(p.reserve_id.clone()),
        Expr::int(p.liquidation_price),
        Expr::int(p.challenge_reward_ppm),
        Expr::int(p.auction_duration_daa),
        Expr::int(p.collateral_sompi),
        Expr::int(stack.kcc.prefix.len() as i64),
        Expr::int(stack.kcc.suffix.len() as i64),
        Expr::bytes(stack.kcc.template_hash.clone()),
        Expr::int(stack.reserve.prefix.len() as i64),
        Expr::int(stack.reserve.suffix.len() as i64),
        Expr::bytes(stack.reserve.template_hash.clone()),
    ]
}

pub fn challenge_args(p: &ProtocolParams, stack: &ProtocolStack) -> Vec<Expr<'static>> {
    vec![
        Expr::bytes(p.owner.clone()),
        Expr::bytes(p.challenger.clone()),
        Expr::bytes(p.position_id.clone()),
        Expr::bytes(p.asset_id.clone()),
        Expr::int(p.debt),
        Expr::int(p.assigned_reserve),
        Expr::bytes(p.reserve_id.clone()),
        Expr::int(p.liquidation_price),
        Expr::int(p.challenge_reward_ppm),
        Expr::int(p.challenge_period_daa),
        Expr::int(p.auction_duration_daa),
        Expr::int(p.collateral_sompi),
        Expr::int(stack.kcc.prefix.len() as i64),
        Expr::int(stack.kcc.suffix.len() as i64),
        Expr::bytes(stack.kcc.template_hash.clone()),
        Expr::dynamic_bytes(stack.auction.prefix.clone()),
        Expr::dynamic_bytes(stack.auction.suffix.clone()),
        Expr::bytes(stack.auction.template_hash.clone()),
    ]
}

pub fn anchor_args(p: &ProtocolParams, stack: &ProtocolStack) -> Vec<Expr<'static>> {
    let mut args = challenge_args(p, stack);
    args.truncate(15);
    args.extend([
        Expr::dynamic_bytes(stack.challenge.prefix.clone()),
        Expr::dynamic_bytes(stack.challenge.suffix.clone()),
        Expr::bytes(stack.challenge.template_hash.clone()),
    ]);
    args
}

pub fn position_args(
    p: &ProtocolParams,
    stack: &ProtocolStack,
    challenge_id: Vec<u8>,
) -> Vec<Expr<'static>> {
    vec![
        Expr::bytes(p.owner.clone()),
        Expr::bytes(p.asset_id.clone()),
        Expr::int(p.debt),
        Expr::int(p.assigned_reserve),
        Expr::bytes(p.reserve_id.clone()),
        Expr::int(p.reserve_contribution_ppm),
        Expr::int(p.liquidation_price),
        Expr::int(p.challenge_period_daa),
        Expr::int(p.auction_duration_daa),
        Expr::int(p.challenge_reward_ppm),
        Expr::bytes(challenge_id),
        Expr::int(stack.kcc.prefix.len() as i64),
        Expr::int(stack.kcc.suffix.len() as i64),
        Expr::bytes(stack.kcc.template_hash.clone()),
        Expr::int(stack.position.prefix.len() as i64),
        Expr::int(stack.position.suffix.len() as i64),
        Expr::bytes(stack.position.template_hash.clone()),
        Expr::dynamic_bytes(stack.anchor.prefix.clone()),
        Expr::dynamic_bytes(stack.anchor.suffix.clone()),
        Expr::bytes(stack.anchor.template_hash.clone()),
        Expr::dynamic_bytes(stack.challenged_position.prefix.clone()),
        Expr::dynamic_bytes(stack.challenged_position.suffix.clone()),
        Expr::bytes(stack.challenged_position.template_hash.clone()),
    ]
}

pub fn challenged_position_args(
    p: &ProtocolParams,
    stack: &ProtocolStack,
    challenge_id: Vec<u8>,
) -> Vec<Expr<'static>> {
    vec![
        Expr::bytes(p.owner.clone()),
        Expr::bytes(p.asset_id.clone()),
        Expr::int(p.debt),
        Expr::int(p.assigned_reserve),
        Expr::bytes(p.reserve_id.clone()),
        Expr::int(p.reserve_contribution_ppm),
        Expr::int(p.liquidation_price),
        Expr::int(p.challenge_period_daa),
        Expr::int(p.auction_duration_daa),
        Expr::int(p.challenge_reward_ppm),
        Expr::bytes(challenge_id),
        Expr::int(stack.kcc.prefix.len() as i64),
        Expr::int(stack.kcc.suffix.len() as i64),
        Expr::bytes(stack.kcc.template_hash.clone()),
        Expr::int(stack.position.prefix.len() as i64),
        Expr::int(stack.position.suffix.len() as i64),
        Expr::bytes(stack.position.template_hash.clone()),
        Expr::int(stack.challenge.prefix.len() as i64),
        Expr::int(stack.challenge.suffix.len() as i64),
        Expr::bytes(stack.challenge.template_hash.clone()),
        Expr::int(stack.auction.prefix.len() as i64),
        Expr::int(stack.auction.suffix.len() as i64),
        Expr::bytes(stack.auction.template_hash.clone()),
    ]
}

pub fn module_args(p: &ProtocolParams, stack: &ProtocolStack) -> Vec<Expr<'static>> {
    vec![
        Expr::bytes(p.asset_id.clone()),
        Expr::int(p.module_remaining_mint),
        Expr::int(p.max_debt_per_position),
        Expr::int(p.minimum_collateral_sompi),
        Expr::int(p.liquidation_price),
        Expr::int(p.module_expiration_daa),
        Expr::int(p.challenge_period_daa),
        Expr::int(p.auction_duration_daa),
        Expr::int(p.challenge_reward_ppm),
        Expr::int(p.position_nonce),
        Expr::bytes(p.reserve_id.clone()),
        Expr::int(p.reserve_contribution_ppm),
        Expr::int(p.risk_premium_ppm),
        Expr::int(stack.kcc.prefix.len() as i64),
        Expr::int(stack.kcc.suffix.len() as i64),
        Expr::bytes(stack.kcc.template_hash.clone()),
        Expr::bytes(stack.position.template_hash.clone()),
    ]
}

fn validate_governance(p: &ProtocolParams, g: &GovernanceParams) -> Result<(), String> {
    for (name, value) in [
        ("proposer", &g.proposer),
        ("governance_id", &g.governance_id),
        ("root_id", &g.root_id),
    ] {
        if value.len() != 32 {
            return Err(format!("{name}: 32 bytes required"));
        }
    }
    let module_duration = p
        .module_expiration_daa
        .checked_sub(p.current_daa)
        .ok_or("invalid Module duration")?;
    if g.proposal_nonce < 0
        || g.execution_nonce < 0
        || g.voting_delay_daa <= 0
        || g.execution_window_daa <= 0
        || !(1..=1_000_000).contains(&g.veto_threshold_ppm)
        || g.proposal_deposit_sompi <= 0
        || g.proposal_fee_kusd <= 0
        || g.min_module_allocation <= 0
        || g.max_module_allocation < g.min_module_allocation
        || g.max_debt_per_position <= 0
        || g.min_collateral_sompi <= 0
        || g.max_collateral_sompi < g.min_collateral_sompi
        || g.min_module_duration_daa <= 0
        || g.max_module_duration_daa < g.min_module_duration_daa
        || g.min_challenge_period_daa <= 0
        || g.max_challenge_period_daa < g.min_challenge_period_daa
        || g.min_auction_duration_daa <= 0
        || g.max_auction_duration_daa < g.min_auction_duration_daa
        || !(0..=1_000_000).contains(&g.max_risk_premium_ppm)
        || g.min_reserve_contribution_ppm < 0
        || g.max_reserve_contribution_ppm < g.min_reserve_contribution_ppm
        || g.max_reserve_contribution_ppm >= 1_000_000
        || g.root_remaining_allocation < p.module_remaining_mint
        || p.module_remaining_mint < g.min_module_allocation
        || p.module_remaining_mint > g.max_module_allocation
        || p.max_debt_per_position > g.max_debt_per_position
        || p.minimum_collateral_sompi < g.min_collateral_sompi
        || p.minimum_collateral_sompi > g.max_collateral_sompi
        || p.liquidation_price <= 0
        || module_duration < g.min_module_duration_daa
        || module_duration > g.max_module_duration_daa
        || p.challenge_period_daa < g.min_challenge_period_daa
        || p.challenge_period_daa > g.max_challenge_period_daa
        || p.auction_duration_daa < g.min_auction_duration_daa
        || p.auction_duration_daa > g.max_auction_duration_daa
        || p.risk_premium_ppm > g.max_risk_premium_ppm
        || p.reserve_contribution_ppm < g.min_reserve_contribution_ppm
        || p.reserve_contribution_ppm > g.max_reserve_contribution_ppm
    {
        return Err("invalid governance parameters".into());
    }
    Ok(())
}

pub fn proposal_args(
    p: &ProtocolParams,
    g: &GovernanceParams,
    stack: &ProtocolStack,
) -> Vec<Expr<'static>> {
    proposal_args_with_activation(p, g, stack, false)
}

pub fn proposal_args_with_activation(
    p: &ProtocolParams,
    g: &GovernanceParams,
    stack: &ProtocolStack,
    activated: bool,
) -> Vec<Expr<'static>> {
    vec![
        Expr::bytes(g.proposer.clone()),
        Expr::bytes(g.governance_id.clone()),
        Expr::bytes(g.root_id.clone()),
        Expr::bytes(p.kps_id.clone()),
        Expr::bytes(p.reserve_id.clone()),
        Expr::int(g.proposal_nonce),
        Expr::bool(activated),
        Expr::int(g.voting_delay_daa),
        Expr::int(g.execution_window_daa),
        Expr::int(g.veto_threshold_ppm),
        Expr::int(g.proposal_deposit_sompi),
        Expr::bytes(p.asset_id.clone()),
        Expr::int(p.module_remaining_mint),
        Expr::int(p.max_debt_per_position),
        Expr::int(p.minimum_collateral_sompi),
        Expr::int(p.liquidation_price),
        Expr::int(p.module_expiration_daa),
        Expr::int(p.challenge_period_daa),
        Expr::int(p.auction_duration_daa),
        Expr::int(p.challenge_reward_ppm),
        Expr::int(p.reserve_contribution_ppm),
        Expr::int(p.risk_premium_ppm),
        Expr::int(g.proposal_fee_kusd),
        Expr::int(stack.kcc.prefix.len() as i64),
        Expr::int(stack.kcc.suffix.len() as i64),
        Expr::bytes(stack.kcc.template_hash.clone()),
        Expr::int(stack.kps.prefix.len() as i64),
        Expr::int(stack.kps.suffix.len() as i64),
        Expr::bytes(stack.kps.template_hash.clone()),
        Expr::int(stack.delegation.prefix.len() as i64),
        Expr::int(stack.delegation.suffix.len() as i64),
        Expr::bytes(stack.delegation.template_hash.clone()),
        Expr::int(p.max_kps_vote_weight),
        Expr::int(stack.reserve.prefix.len() as i64),
        Expr::int(stack.reserve.suffix.len() as i64),
        Expr::bytes(stack.reserve.template_hash.clone()),
    ]
}

pub fn governance_args(
    p: &ProtocolParams,
    g: &GovernanceParams,
    proposal: &Artifact,
    active_proposal_id: Vec<u8>,
    base: &ProtocolStack,
) -> Vec<Expr<'static>> {
    vec![
        Expr::bytes(g.root_id.clone()),
        Expr::bytes(p.asset_id.clone()),
        Expr::bytes(p.kps_id.clone()),
        Expr::bytes(p.reserve_id.clone()),
        Expr::int(g.proposal_nonce),
        Expr::int(g.execution_nonce),
        Expr::bytes(active_proposal_id),
        Expr::int(g.voting_delay_daa),
        Expr::int(g.execution_window_daa),
        Expr::int(g.veto_threshold_ppm),
        Expr::int(g.proposal_deposit_sompi),
        Expr::int(g.proposal_fee_kusd),
        Expr::int(g.min_module_allocation),
        Expr::int(g.max_module_allocation),
        Expr::int(g.max_debt_per_position),
        Expr::int(g.min_collateral_sompi),
        Expr::int(g.max_collateral_sompi),
        Expr::int(g.min_module_duration_daa),
        Expr::int(g.max_module_duration_daa),
        Expr::int(g.min_challenge_period_daa),
        Expr::int(g.max_challenge_period_daa),
        Expr::int(g.min_auction_duration_daa),
        Expr::int(g.max_auction_duration_daa),
        Expr::int(g.max_risk_premium_ppm),
        Expr::int(g.min_reserve_contribution_ppm),
        Expr::int(g.max_reserve_contribution_ppm),
        Expr::dynamic_bytes(proposal.prefix.clone()),
        Expr::dynamic_bytes(proposal.suffix.clone()),
        Expr::bytes(proposal.template_hash.clone()),
        Expr::int(base.kcc.prefix.len() as i64),
        Expr::int(base.kcc.suffix.len() as i64),
        Expr::bytes(base.kcc.template_hash.clone()),
    ]
}

pub fn root_args(
    p: &ProtocolParams,
    stack: &GovernanceStack,
    module_nonce: i64,
    remaining_allocation: i64,
    authority: Vec<u8>,
    authority_type: u8,
) -> Vec<Expr<'static>> {
    root_args_with_state(
        p,
        stack,
        true,
        module_nonce,
        remaining_allocation,
        authority,
        authority_type,
    )
}

pub fn root_args_with_state(
    p: &ProtocolParams,
    stack: &GovernanceStack,
    initialized: bool,
    module_nonce: i64,
    remaining_allocation: i64,
    authority: Vec<u8>,
    authority_type: u8,
) -> Vec<Expr<'static>> {
    vec![
        Expr::bytes(p.asset_id.clone()),
        Expr::bool(initialized),
        Expr::int(module_nonce),
        Expr::int(remaining_allocation),
        Expr::bytes(authority),
        Expr::byte(authority_type),
        Expr::int(stack.base.kcc.prefix.len() as i64),
        Expr::int(stack.base.kcc.suffix.len() as i64),
        Expr::bytes(stack.base.kcc.template_hash.clone()),
        Expr::bytes(stack.base.module.template_hash.clone()),
        Expr::int(stack.governance.prefix.len() as i64),
        Expr::int(stack.governance.suffix.len() as i64),
        Expr::bytes(stack.governance.template_hash.clone()),
        Expr::int(stack.proposal.prefix.len() as i64),
        Expr::int(stack.proposal.suffix.len() as i64),
        Expr::bytes(stack.proposal.template_hash.clone()),
    ]
}

pub fn compile_stack(p: &ProtocolParams) -> Result<ProtocolStack, String> {
    validate(p)?;
    let kcc = compile("contracts/kcc20.sil", &kcc_args(vec![0; 32], 0, 2, true))?;
    let delegation = compile(
        "contracts/kps-delegation.sil",
        &delegation_args(p, vec![0; 32], vec![0; 32], 1, 0),
    )?;
    let kps = compile(
        "contracts/kps.sil",
        &kps_args(p, &delegation, vec![0; 32], 0, 2, true),
    )?;
    let mut stack = ProtocolStack {
        kcc,
        kps,
        delegation,
        reserve: empty_artifact(),
        auction: empty_artifact(),
        challenge: empty_artifact(),
        anchor: empty_artifact(),
        challenged_position: empty_artifact(),
        position: empty_artifact(),
        module: empty_artifact(),
    };
    // First pass: Auction probes the Reserve identity. Auction references are
    // Reserve state fields, so they do not alter its template hash. This closes
    // the template cycle without assuming an identity.
    let auction_probe = compile("contracts/auction.sil", &auction_args(p, &stack))?;
    stack.auction = auction_probe;
    let reserve_probe = compile(
        "contracts/equity-reserve-base.sil",
        &reserve_args(
            p,
            &stack,
            p.reserve_kusd,
            p.total_kps,
            p.reserve_collateral_sompi,
        ),
    )?;
    stack.reserve = reserve_probe.clone();
    stack.auction = compile("contracts/auction.sil", &auction_args(p, &stack))?;
    let final_reserve = compile(
        "contracts/equity-reserve-base.sil",
        &reserve_args(
            p,
            &stack,
            p.reserve_kusd,
            p.total_kps,
            p.reserve_collateral_sompi,
        ),
    )?;
    if final_reserve.template_hash != reserve_probe.template_hash
        || final_reserve.prefix != reserve_probe.prefix
        || final_reserve.suffix != reserve_probe.suffix
    {
        return Err("the Reserve/Auction template cycle is not stable".into());
    }
    stack.reserve = final_reserve;
    let final_auction = compile("contracts/auction.sil", &auction_args(p, &stack))?;
    if final_auction.template_hash != stack.auction.template_hash
        || final_auction.prefix != stack.auction.prefix
        || final_auction.suffix != stack.auction.suffix
    {
        return Err("the second Auction compilation pass changed its identity".into());
    }
    stack.auction = final_auction;
    stack.challenge = compile("contracts/challenge.sil", &challenge_args(p, &stack))?;
    stack.anchor = compile("contracts/challenge-anchor.sil", &anchor_args(p, &stack))?;
    compile_position_pair(p, &mut stack)?;
    stack.module = compile("contracts/minting-module.sil", &module_args(p, &stack))?;
    Ok(stack)
}

pub(crate) fn compile_position_pair(
    p: &ProtocolParams,
    stack: &mut ProtocolStack,
) -> Result<(), String> {
    let challenged_probe = compile(
        "contracts/challenged-position.sil",
        &challenged_position_args(p, stack, vec![0; 32]),
    )?;
    stack.challenged_position = challenged_probe.clone();
    let position_probe = compile(
        "contracts/position.sil",
        &position_args(p, stack, vec![0; 32]),
    )?;
    stack.position = position_probe.clone();
    let position = compile(
        "contracts/position.sil",
        &position_args(p, stack, vec![0; 32]),
    )?;
    if position.prefix != position_probe.prefix
        || position.suffix != position_probe.suffix
        || position.template_hash != position_probe.template_hash
    {
        return Err("the self-referential Position identity is not stable".into());
    }
    stack.position = position;
    let challenged = compile(
        "contracts/challenged-position.sil",
        &challenged_position_args(p, stack, vec![0; 32]),
    )?;
    if challenged.prefix != challenged_probe.prefix
        || challenged.suffix != challenged_probe.suffix
        || challenged.template_hash != challenged_probe.template_hash
    {
        return Err("the Position/ChallengedPosition template cycle is not stable".into());
    }
    stack.challenged_position = challenged;
    Ok(())
}

pub fn compile_governance_stack(
    p: &ProtocolParams,
    g: &GovernanceParams,
) -> Result<GovernanceStack, String> {
    let base = compile_stack(p)?;
    compile_governance_stack_with_base(p, g, base)
}

pub fn compile_governance_stack_with_base(
    p: &ProtocolParams,
    g: &GovernanceParams,
    base: ProtocolStack,
) -> Result<GovernanceStack, String> {
    validate_governance(p, g)?;
    let proposal = compile("contracts/module-proposal.sil", &proposal_args(p, g, &base))?;
    let governance = compile(
        "contracts/governance.sil",
        &governance_args(p, g, &proposal, vec![0; 32], &base),
    )?;
    let provisional = GovernanceStack {
        base,
        proposal,
        governance,
        root: empty_artifact(),
    };
    let root = compile(
        "contracts/root-issuance.sil",
        &root_args(
            p,
            &provisional,
            0,
            g.root_remaining_allocation,
            g.governance_id.clone(),
            4,
        ),
    )?;
    Ok(GovernanceStack {
        root,
        ..provisional
    })
}

pub(crate) fn governance_state_expr(
    p: &ProtocolParams,
    g: &GovernanceParams,
    active_proposal_id: Vec<u8>,
    name: &'static str,
) -> Expr<'static> {
    struct_object(
        name,
        vec![
            ("rootId", Expr::bytes(g.root_id.clone())),
            ("assetId", Expr::bytes(p.asset_id.clone())),
            ("reserveId", Expr::bytes(p.reserve_id.clone())),
            ("proposalNonce", Expr::int(g.proposal_nonce)),
            ("executionNonce", Expr::int(g.execution_nonce)),
            ("activeProposalId", Expr::bytes(active_proposal_id)),
        ],
    )
}

pub(crate) fn proposal_state_expr(
    p: &ProtocolParams,
    g: &GovernanceParams,
    activated: bool,
    name: &'static str,
) -> Expr<'static> {
    struct_object(
        name,
        vec![
            ("proposer", Expr::bytes(g.proposer.clone())),
            ("governanceId", Expr::bytes(g.governance_id.clone())),
            ("proposalNonce", Expr::int(g.proposal_nonce)),
            ("activated", Expr::bool(activated)),
            ("votingDelayDaa", Expr::int(g.voting_delay_daa)),
            ("executionWindowDaa", Expr::int(g.execution_window_daa)),
            ("vetoThresholdPpm", Expr::int(g.veto_threshold_ppm)),
            ("depositSompi", Expr::int(g.proposal_deposit_sompi)),
            ("assetId", Expr::bytes(p.asset_id.clone())),
            ("allocation", Expr::int(p.module_remaining_mint)),
            ("maxDebtPerPosition", Expr::int(p.max_debt_per_position)),
            (
                "minimumCollateralSompi",
                Expr::int(p.minimum_collateral_sompi),
            ),
            ("liquidationPrice", Expr::int(p.liquidation_price)),
            ("expirationDaa", Expr::int(p.module_expiration_daa)),
            ("challengePeriodDaa", Expr::int(p.challenge_period_daa)),
            ("auctionDurationDaa", Expr::int(p.auction_duration_daa)),
            ("challengeRewardPpm", Expr::int(p.challenge_reward_ppm)),
            (
                "reserveContributionPpm",
                Expr::int(p.reserve_contribution_ppm),
            ),
            ("riskPremiumPpm", Expr::int(p.risk_premium_ppm)),
        ],
    )
}

fn delegation_state_expr(
    p: &ProtocolParams,
    owner: Vec<u8>,
    delegate: Vec<u8>,
    amount: i64,
    weight: i64,
    name: &'static str,
) -> Expr<'static> {
    struct_object(
        name,
        vec![
            ("owner", Expr::bytes(owner)),
            ("delegate", Expr::bytes(delegate)),
            ("kpsId", Expr::bytes(p.kps_id.clone())),
            ("amount", Expr::int(amount)),
            ("weight", Expr::int(weight)),
        ],
    )
}

pub(crate) fn token_state_expr(
    owner: Vec<u8>,
    identifier_type: u8,
    amount: i64,
    is_minter: bool,
    name: &'static str,
) -> Expr<'static> {
    struct_object(
        name,
        vec![
            ("ownerIdentifier", Expr::bytes(owner)),
            ("identifierType", Expr::byte(identifier_type)),
            ("amount", Expr::int(amount)),
            ("isMinter", Expr::bool(is_minter)),
        ],
    )
}

pub fn compile_governance_state(
    p: &ProtocolParams,
    g: &GovernanceParams,
    active_proposal_id: Vec<u8>,
) -> Result<Artifact, String> {
    validate_governance(p, g)?;
    if active_proposal_id.len() != 32 {
        return Err("active_proposal_id: 32 bytes required".into());
    }
    let stack = compile_governance_stack(p, g)?;
    compile(
        "contracts/governance.sil",
        &governance_args(p, g, &stack.proposal, active_proposal_id, &stack.base),
    )
}

pub fn compile_proposal_state(
    p: &ProtocolParams,
    g: &GovernanceParams,
    activated: bool,
) -> Result<Artifact, String> {
    validate_governance(p, g)?;
    let stack = compile_governance_stack(p, g)?;
    compile(
        "contracts/module-proposal.sil",
        &proposal_args_with_activation(p, g, &stack.base, activated),
    )
}

pub fn compile_delegation_state(
    p: &ProtocolParams,
    owner: Vec<u8>,
    delegate: Vec<u8>,
    amount: i64,
    weight: i64,
) -> Result<Artifact, String> {
    if owner.len() != 32 || delegate.len() != 32 || amount <= 0 {
        return Err("invalid delegation state".into());
    }
    compile(
        "contracts/kps-delegation.sil",
        &delegation_args(p, owner, delegate, amount, weight),
    )
}

/// Builds the Governance leader call that serializes a new proposal.
/// `g` describes the current state; the new proposal nonce is `g + 1`.
pub fn governance_propose_module_call(
    p: &ProtocolParams,
    g: &GovernanceParams,
    proposal_id: Vec<u8>,
    proposal_output_index: u8,
) -> Result<Vec<u8>, String> {
    if proposal_id.len() != 32 {
        return Err("proposal_id: 32 bytes required".into());
    }
    let stack = compile_governance_stack(p, g)?;
    let src = source("contracts/governance.sil")?;
    let contract = compile_contract(
        &src,
        &governance_args(p, g, &stack.proposal, vec![0; 32], &stack.base),
        CompileOptions::default(),
    )
    .map_err(|e| e.to_string())?;
    let mut next = g.clone();
    next.proposal_nonce = next
        .proposal_nonce
        .checked_add(1)
        .ok_or("overflow proposal nonce")?;
    covenant_call(
        &contract,
        "proposeModulePolicy",
        vec![
            governance_state_expr(p, &next, proposal_id.clone(), "State"),
            Expr::byte(proposal_output_index),
            proposal_state_expr(p, &next, false, "ProposalState"),
            token_state_expr(proposal_id, 2, g.proposal_fee_kusd, false, "TokenState"),
        ],
        true,
    )
}

pub fn proposal_activate_call(p: &ProtocolParams, g: &GovernanceParams) -> Result<Vec<u8>, String> {
    let src = source("contracts/module-proposal.sil")?;
    let stack = compile_governance_stack(p, g)?;
    let contract = compile_contract(
        &src,
        &proposal_args_with_activation(p, g, &stack.base, false),
        CompileOptions::default(),
    )
    .map_err(|e| e.to_string())?;
    covenant_call(
        &contract,
        "activatePolicy",
        vec![proposal_state_expr(p, g, true, "State")],
        true,
    )
}

/// Governance follower call for execution, veto, or expiration.
/// `execute_module` increments the execution counter.
pub fn governance_finish_call(
    p: &ProtocolParams,
    g: &GovernanceParams,
    active_proposal_id: Vec<u8>,
    action: &str,
) -> Result<Vec<u8>, String> {
    if active_proposal_id.len() != 32 {
        return Err("active_proposal_id: 32 bytes required".into());
    }
    let policy = match action {
        "execute" => "executeModulePolicy",
        "veto" => "vetoModulePolicy",
        "cancel" => "cancelExpiredPolicy",
        _ => return Err("expected Governance action: execute, veto, or cancel".into()),
    };
    let stack = compile_governance_stack(p, g)?;
    let src = source("contracts/governance.sil")?;
    let contract = compile_contract(
        &src,
        &governance_args(p, g, &stack.proposal, active_proposal_id, &stack.base),
        CompileOptions::default(),
    )
    .map_err(|e| e.to_string())?;
    let mut next = g.clone();
    if action == "execute" {
        next.execution_nonce = next
            .execution_nonce
            .checked_add(1)
            .ok_or("overflow execution nonce")?;
    }
    covenant_call(
        &contract,
        policy,
        vec![governance_state_expr(p, &next, vec![0; 32], "State")],
        true,
    )
}

pub fn proposal_finish_call(
    p: &ProtocolParams,
    g: &GovernanceParams,
    action: &str,
    refund_output_index: u8,
    fee_input_index: u8,
    fee_output_index: u8,
) -> Result<Vec<u8>, String> {
    let (activated, entry) = match action {
        "execute" => (true, "executePolicy"),
        "cancel" => (true, "cancelExpiredPolicy"),
        _ => return Err("expected Proposal action: execute or cancel".into()),
    };
    let stack = compile_governance_stack(p, g)?;
    let src = source("contracts/module-proposal.sil")?;
    let contract = compile_contract(
        &src,
        &proposal_args_with_activation(p, g, &stack.base, activated),
        CompileOptions::default(),
    )
    .map_err(|e| e.to_string())?;
    entry_call(
        &contract,
        entry,
        vec![
            Expr::byte(refund_output_index),
            token_state_expr(
                g.proposer.clone(),
                0,
                g.proposal_fee_kusd,
                false,
                "TokenState",
            ),
            Expr::byte(fee_input_index),
            Expr::byte(fee_output_index),
        ],
    )
}

pub fn delegation_mature_call(
    p: &ProtocolParams,
    owner: Vec<u8>,
    delegate: Vec<u8>,
    amount: i64,
    weight: i64,
    delegation_id: Vec<u8>,
    kps_input_index: u8,
    kps_output_index: u8,
) -> Result<Vec<u8>, String> {
    let src = source("contracts/kps-delegation.sil")?;
    let contract = compile_contract(
        &src,
        &delegation_args(p, owner.clone(), delegate.clone(), amount, weight),
        CompileOptions::default(),
    )
    .map_err(|e| e.to_string())?;
    covenant_call(
        &contract,
        "maturePolicy",
        vec![
            delegation_state_expr(p, owner, delegate, amount, weight + 1, "State"),
            token_state_expr(delegation_id, 2, amount, false, "TokenState"),
            Expr::byte(kps_input_index),
            Expr::byte(kps_output_index),
        ],
        true,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn delegation_redelegate_call(
    p: &ProtocolParams,
    owner: Vec<u8>,
    old_delegate: Vec<u8>,
    new_delegate: Vec<u8>,
    amount: i64,
    weight: i64,
    delegation_id: Vec<u8>,
    owner_signature: Vec<u8>,
    kps_input_index: u8,
    kps_output_index: u8,
) -> Result<Vec<u8>, String> {
    let src = source("contracts/kps-delegation.sil")?;
    let contract = compile_contract(
        &src,
        &delegation_args(p, owner.clone(), old_delegate, amount, weight),
        CompileOptions::default(),
    )
    .map_err(|e| e.to_string())?;
    covenant_call(
        &contract,
        "redelegatePolicy",
        vec![
            delegation_state_expr(p, owner, new_delegate, amount, weight, "State"),
            Expr::bytes(owner_signature),
            token_state_expr(delegation_id, 2, amount, false, "TokenState"),
            Expr::byte(kps_input_index),
            Expr::byte(kps_output_index),
        ],
        true,
    )
}

pub fn delegation_unlock_call(
    p: &ProtocolParams,
    owner: Vec<u8>,
    delegate: Vec<u8>,
    amount: i64,
    weight: i64,
    owner_signature: Vec<u8>,
    kps_input_index: u8,
    kps_output_index: u8,
) -> Result<Vec<u8>, String> {
    let src = source("contracts/kps-delegation.sil")?;
    let contract = compile_contract(
        &src,
        &delegation_args(p, owner.clone(), delegate, amount, weight),
        CompileOptions::default(),
    )
    .map_err(|e| e.to_string())?;
    entry_call(
        &contract,
        "unlockPolicy",
        vec![
            Expr::bytes(owner_signature),
            token_state_expr(owner, 0, amount, false, "TokenState"),
            Expr::byte(kps_input_index),
            Expr::byte(kps_output_index),
        ],
    )
}

pub(crate) fn module_state_expr(p: &ProtocolParams, name: &'static str) -> Expr<'static> {
    struct_object(
        name,
        vec![
            ("assetId", Expr::bytes(p.asset_id.clone())),
            ("remainingMint", Expr::int(p.module_remaining_mint)),
            ("maxDebtPerPosition", Expr::int(p.max_debt_per_position)),
            (
                "minimumCollateralSompi",
                Expr::int(p.minimum_collateral_sompi),
            ),
            ("liquidationPrice", Expr::int(p.liquidation_price)),
            ("expirationDaa", Expr::int(p.module_expiration_daa)),
            ("challengePeriodDaa", Expr::int(p.challenge_period_daa)),
            ("auctionDurationDaa", Expr::int(p.auction_duration_daa)),
            ("challengeRewardPpm", Expr::int(p.challenge_reward_ppm)),
            ("positionNonce", Expr::int(p.position_nonce)),
            ("reserveId", Expr::bytes(p.reserve_id.clone())),
            (
                "reserveContributionPpm",
                Expr::int(p.reserve_contribution_ppm),
            ),
            ("riskPremiumPpm", Expr::int(p.risk_premium_ppm)),
        ],
    )
}

pub(crate) fn root_state_expr(
    p: &ProtocolParams,
    g: &GovernanceParams,
    module_nonce: i64,
    remaining_allocation: i64,
    name: &'static str,
) -> Expr<'static> {
    struct_object(
        name,
        vec![
            ("assetId", Expr::bytes(p.asset_id.clone())),
            ("initialized", Expr::bool(true)),
            ("moduleNonce", Expr::int(module_nonce)),
            ("remainingAllocation", Expr::int(remaining_allocation)),
            ("authorityIdentifier", Expr::bytes(g.governance_id.clone())),
            ("authorityType", Expr::byte(4)),
        ],
    )
}

/// Root call that executes an activated Governance proposal. No user signature
/// is required: authority belongs to the Governance covenant (type 4).
#[allow(clippy::too_many_arguments)]
pub fn root_execute_module_call(
    p: &ProtocolParams,
    g: &GovernanceParams,
    active_proposal_id: Vec<u8>,
    root_id: Vec<u8>,
    module_id: Vec<u8>,
    previous_module_nonce: i64,
    previous_remaining_allocation: i64,
    module_output_index: u8,
) -> Result<Vec<u8>, String> {
    if active_proposal_id.len() != 32 || root_id.len() != 32 || module_id.len() != 32 {
        return Err("Covenant ID: 32 bytes required".into());
    }
    let stack = compile_governance_stack(p, g)?;
    let src = source("contracts/root-issuance.sil")?;
    let contract = compile_contract(
        &src,
        &root_args(
            p,
            &stack,
            previous_module_nonce,
            previous_remaining_allocation,
            g.governance_id.clone(),
            4,
        ),
        CompileOptions::default(),
    )
    .map_err(|e| e.to_string())?;
    let remaining = previous_remaining_allocation
        .checked_sub(p.module_remaining_mint)
        .ok_or("Root allocation underflow")?;
    let mut executed = g.clone();
    executed.execution_nonce = executed
        .execution_nonce
        .checked_add(1)
        .ok_or("overflow execution nonce")?;
    covenant_call(
        &contract,
        "createModulePolicy",
        vec![
            root_state_expr(p, g, previous_module_nonce + 1, remaining, "State"),
            Expr::bytes(vec![0; 65]),
            Expr::byte(module_output_index),
            Expr::dynamic_bytes(stack.base.module.prefix.clone()),
            Expr::dynamic_bytes(stack.base.module.suffix.clone()),
            module_state_expr(p, "ModuleState"),
            governance_state_expr(p, &executed, vec![0; 32], "GovernanceState"),
            token_state_expr(root_id, 2, 0, true, "KCC20State"),
            token_state_expr(module_id, 2, 0, true, "KCC20State"),
        ],
        true,
    )
}

/// One-way transition that removes the Root bootstrap key and transfers
/// authority to the Governance singleton.
#[allow(clippy::too_many_arguments)]
pub fn root_handover_to_governance_call(
    p: &ProtocolParams,
    g: &GovernanceParams,
    current_authority: Vec<u8>,
    authority_signature: Vec<u8>,
    module_nonce: i64,
    remaining_allocation: i64,
) -> Result<Vec<u8>, String> {
    if current_authority.len() != 32 || authority_signature.is_empty() {
        return Err("invalid handover authority or signature".into());
    }
    let stack = compile_governance_stack(p, g)?;
    let src = source("contracts/root-issuance.sil")?;
    let contract = compile_contract(
        &src,
        &root_args(
            p,
            &stack,
            module_nonce,
            remaining_allocation,
            current_authority,
            0,
        ),
        CompileOptions::default(),
    )
    .map_err(|e| e.to_string())?;
    covenant_call(
        &contract,
        "handoverPolicy",
        vec![
            root_state_expr(p, g, module_nonce, remaining_allocation, "State"),
            Expr::bytes(authority_signature),
            Expr::bytes(g.governance_id.clone()),
            Expr::byte(4),
        ],
        true,
    )
}

pub fn kps_lock_delegation_call(
    p: &ProtocolParams,
    owner_signature: Vec<u8>,
    witness: u8,
    delegate: Vec<u8>,
    amount: i64,
    delegation_id: Vec<u8>,
    delegation_output_index: u8,
    kps_output_index: u8,
) -> Result<Vec<u8>, String> {
    if delegate.len() != 32 || delegation_id.len() != 32 || amount <= 0 {
        return Err("invalid KPS lock parameters".into());
    }
    let stack = compile_stack(p)?;
    let src = source("contracts/kps.sil")?;
    let contract = compile_contract(
        &src,
        &kps_args(p, &stack.delegation, p.owner.clone(), amount, 0, false),
        CompileOptions::default(),
    )
    .map_err(|e| e.to_string())?;
    entry_call(
        &contract,
        "lockDelegationPolicy",
        vec![
            Expr::bytes(owner_signature),
            Expr::byte(witness),
            delegation_state_expr(p, p.owner.clone(), delegate, amount, 0, "DelegationState"),
            token_state_expr(delegation_id, 2, amount, false, "State"),
            Expr::byte(delegation_output_index),
            Expr::byte(kps_output_index),
        ],
    )
}

#[derive(Clone, Debug)]
pub struct GovernanceVote {
    pub owner: Vec<u8>,
    pub delegate: Vec<u8>,
    pub delegation_id: Vec<u8>,
    pub amount: i64,
    pub weight: i64,
    pub delegation_input_index: u8,
}

pub(crate) fn reserve_state_expr(
    p: &ProtocolParams,
    stack: &ProtocolStack,
    name: &'static str,
) -> Expr<'static> {
    struct_object(
        name,
        vec![
            ("kusdAssetId", Expr::bytes(p.asset_id.clone())),
            ("kpsAssetId", Expr::bytes(p.kps_id.clone())),
            ("reserveKusd", Expr::int(p.reserve_kusd)),
            ("totalKps", Expr::int(p.total_kps)),
            ("collateralSompi", Expr::int(p.reserve_collateral_sompi)),
            (
                "auctionPrefixLenState",
                Expr::int(stack.auction.prefix.len() as i64),
            ),
            (
                "auctionSuffixLenState",
                Expr::int(stack.auction.suffix.len() as i64),
            ),
            (
                "auctionTemplateHashState",
                Expr::bytes(stack.auction.template_hash.clone()),
            ),
        ],
    )
}

pub fn proposal_veto_call(
    p: &ProtocolParams,
    g: &GovernanceParams,
    votes: &[GovernanceVote],
    reserve_input_index: u8,
    refund_output_index: u8,
) -> Result<Vec<u8>, String> {
    if votes.is_empty() || votes.len() > 8 {
        return Err("a veto requires between 1 and 8 delegations".into());
    }
    let stack = compile_governance_stack(p, g)?;
    let src = source("contracts/module-proposal.sil")?;
    let contract = compile_contract(
        &src,
        &proposal_args_with_activation(p, g, &stack.base, false),
        CompileOptions::default(),
    )
    .map_err(|e| e.to_string())?;
    let delegations = votes
        .iter()
        .map(|vote| {
            delegation_state_expr(
                p,
                vote.owner.clone(),
                vote.delegate.clone(),
                vote.amount,
                vote.weight,
                "DelegationState",
            )
        })
        .collect();
    let returned = votes
        .iter()
        .map(|vote| {
            token_state_expr(
                vote.delegation_id.clone(),
                2,
                vote.amount,
                false,
                "TokenState",
            )
        })
        .collect();
    let mut credited = p.clone();
    credited.reserve_kusd = credited
        .reserve_kusd
        .checked_add(g.proposal_fee_kusd)
        .ok_or("proposal fee overflow")?;
    entry_call(
        &contract,
        "vetoPolicy",
        vec![
            Expr::array(parse_type_ref("DelegationState[]").unwrap(), delegations),
            Expr::array(parse_type_ref("TokenState[]").unwrap(), returned),
            Expr::dynamic_bytes(
                votes
                    .iter()
                    .map(|vote| vote.delegation_input_index)
                    .collect(),
            ),
            Expr::byte(reserve_input_index),
            reserve_state_expr(&credited, &stack.base, "ReserveState"),
            token_state_expr(
                p.reserve_id.clone(),
                2,
                credited.reserve_kusd,
                false,
                "TokenState",
            ),
            Expr::byte(refund_output_index),
        ],
    )
}

pub fn reserve_governance_checkpoint_call(p: &ProtocolParams) -> Result<Vec<u8>, String> {
    let stack = compile_stack(p)?;
    let src = source("contracts/equity-reserve-base.sil")?;
    let contract = compile_contract(
        &src,
        &reserve_args(
            p,
            &stack,
            p.reserve_kusd,
            p.total_kps,
            p.reserve_collateral_sompi,
        ),
        CompileOptions::default(),
    )
    .map_err(|e| e.to_string())?;
    covenant_call(
        &contract,
        "governanceCheckpointPolicy",
        vec![reserve_state_expr(p, &stack, "State")],
        true,
    )
}

pub fn reserve_collect_proposal_fee_call(
    p: &ProtocolParams,
    proposal_id: Vec<u8>,
    fee_amount: i64,
) -> Result<Vec<u8>, String> {
    if proposal_id.len() != 32 || fee_amount <= 0 {
        return Err("invalid proposal ID or proposal fee".into());
    }
    let stack = compile_stack(p)?;
    let src = source("contracts/equity-reserve-base.sil")?;
    let contract = compile_contract(
        &src,
        &reserve_args(
            p,
            &stack,
            p.reserve_kusd,
            p.total_kps,
            p.reserve_collateral_sompi,
        ),
        CompileOptions::default(),
    )
    .map_err(|e| e.to_string())?;
    let mut next = p.clone();
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
            token_state_expr(
                p.reserve_id.clone(),
                2,
                next.reserve_kusd,
                false,
                "TokenState",
            ),
        ],
        false,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn delegation_veto_call(
    p: &ProtocolParams,
    vote: &GovernanceVote,
    proposal_id: Vec<u8>,
    delegate_signature: Vec<u8>,
    kps_input_index: u8,
    kps_output_index: u8,
) -> Result<Vec<u8>, String> {
    let src = source("contracts/kps-delegation.sil")?;
    let contract = compile_contract(
        &src,
        &delegation_args(
            p,
            vote.owner.clone(),
            vote.delegate.clone(),
            vote.amount,
            vote.weight,
        ),
        CompileOptions::default(),
    )
    .map_err(|e| e.to_string())?;
    entry_call(
        &contract,
        "vetoPolicy",
        vec![
            Expr::bytes(delegate_signature),
            Expr::bytes(proposal_id),
            delegation_state_expr(
                p,
                vote.owner.clone(),
                vote.delegate.clone(),
                vote.amount,
                vote.weight,
                "State",
            ),
            token_state_expr(
                vote.delegation_id.clone(),
                2,
                vote.amount,
                false,
                "TokenState",
            ),
            Expr::byte(kps_input_index),
            Expr::byte(kps_output_index),
        ],
    )
}

/// KPS companion for mature/redelegate/veto transitions: preserves exactly
/// keeps the share under the same Delegation covenant.
pub fn kps_locked_transfer_call(
    p: &ProtocolParams,
    delegation_id: Vec<u8>,
    amount: i64,
) -> Result<Vec<u8>, String> {
    let stack = compile_stack(p)?;
    let src = source("contracts/kps.sil")?;
    let contract = compile_contract(
        &src,
        &kps_args(
            p,
            &stack.delegation,
            delegation_id.clone(),
            amount,
            2,
            false,
        ),
        CompileOptions::default(),
    )
    .map_err(|e| e.to_string())?;
    covenant_call(
        &contract,
        "transferPolicy",
        vec![
            Expr::array(
                parse_type_ref("State[]").unwrap(),
                vec![token_state_expr(delegation_id, 2, amount, false, "State")],
            ),
            Expr::bytes(vec![0; 65]),
            Expr::byte(0),
        ],
        true,
    )
}

pub fn kps_unlock_delegation_call(
    p: &ProtocolParams,
    owner: Vec<u8>,
    delegate: Vec<u8>,
    delegation_id: Vec<u8>,
    amount: i64,
    weight: i64,
    delegation_input_index: u8,
    kps_output_index: u8,
) -> Result<Vec<u8>, String> {
    let stack = compile_stack(p)?;
    let src = source("contracts/kps.sil")?;
    let contract = compile_contract(
        &src,
        &kps_args(p, &stack.delegation, delegation_id, amount, 2, false),
        CompileOptions::default(),
    )
    .map_err(|e| e.to_string())?;
    entry_call(
        &contract,
        "unlockDelegationPolicy",
        vec![
            delegation_state_expr(
                p,
                owner.clone(),
                delegate,
                amount,
                weight,
                "DelegationState",
            ),
            token_state_expr(owner, 0, amount, false, "State"),
            Expr::byte(delegation_input_index),
            Expr::byte(kps_output_index),
        ],
    )
}

#[derive(Clone, Debug)]
pub struct TokenOutput {
    pub owner: Vec<u8>,
    pub identifier_type: u8,
    pub amount: i64,
    pub is_minter: bool,
}

pub fn compile_kcc_state(
    owner: Vec<u8>,
    amount: i64,
    identifier_type: u8,
    is_minter: bool,
) -> Result<Artifact, String> {
    if owner.len() != 32 || amount < 0 || identifier_type > 2 {
        return Err("invalid KCC state".into());
    }
    compile(
        "contracts/kcc20.sil",
        &kcc_args(owner, amount, identifier_type, is_minter),
    )
}

pub fn compile_kps_state(
    p: &ProtocolParams,
    owner: Vec<u8>,
    amount: i64,
    identifier_type: u8,
    is_minter: bool,
) -> Result<Artifact, String> {
    let stack = compile_stack(p)?;
    compile(
        "contracts/kps.sil",
        &kps_args(
            p,
            &stack.delegation,
            owner,
            amount,
            identifier_type,
            is_minter,
        ),
    )
}

pub fn compile_reserve_state(
    p: &ProtocolParams,
    reserve_kusd: i64,
    total_kps: i64,
    collateral_sompi: i64,
) -> Result<Artifact, String> {
    let stack = compile_stack(p)?;
    compile(
        "contracts/equity-reserve-base.sil",
        &reserve_args(p, &stack, reserve_kusd, total_kps, collateral_sompi),
    )
}

pub fn compile_module_state(
    p: &ProtocolParams,
    remaining_mint: i64,
    position_nonce: i64,
) -> Result<Artifact, String> {
    let mut state = p.clone();
    state.module_remaining_mint = remaining_mint;
    state.position_nonce = position_nonce;
    let stack = compile_stack(&state)?;
    compile("contracts/minting-module.sil", &module_args(&state, &stack))
}

pub fn compile_position_state(
    p: &ProtocolParams,
    debt: i64,
    assigned_reserve: i64,
    challenge_id: Vec<u8>,
) -> Result<Artifact, String> {
    let mut state = p.clone();
    // compile_stack requires positive debt to construct templates.
    // A repaid Position retains the same template, so compile its terminal
    // state directly with the stack derived from the supplied parameters.
    let stack = compile_stack(p)?;
    state.debt = debt;
    state.assigned_reserve = assigned_reserve;
    compile(
        "contracts/position.sil",
        &position_args(&state, &stack, challenge_id),
    )
}

#[allow(clippy::too_many_arguments)]
pub fn compile_root_state(
    p: &ProtocolParams,
    g: &GovernanceParams,
    initialized: bool,
    module_nonce: i64,
    remaining_allocation: i64,
    authority: Vec<u8>,
    authority_type: u8,
) -> Result<Artifact, String> {
    let stack = compile_governance_stack(p, g)?;
    compile(
        "contracts/root-issuance.sil",
        &root_args_with_state(
            p,
            &stack,
            initialized,
            module_nonce,
            remaining_allocation,
            authority,
            authority_type,
        ),
    )
}

#[allow(clippy::too_many_arguments)]
pub fn kcc_transfer_call(
    current_owner: Vec<u8>,
    current_amount: i64,
    current_identifier_type: u8,
    current_is_minter: bool,
    outputs: &[TokenOutput],
    signature: Vec<u8>,
    witness: u8,
    leader: bool,
) -> Result<Vec<u8>, String> {
    let signature = if signature.is_empty() {
        vec![0; 65]
    } else {
        signature
    };
    let src = source("contracts/kcc20.sil")?;
    let contract = compile_contract(
        &src,
        &kcc_args(
            current_owner,
            current_amount,
            current_identifier_type,
            current_is_minter,
        ),
        CompileOptions::default(),
    )
    .map_err(|e| e.to_string())?;
    let mut args = Vec::new();
    if leader {
        args.push(Expr::array(
            parse_type_ref("State[]").unwrap(),
            outputs
                .iter()
                .map(|output| {
                    token_state_expr(
                        output.owner.clone(),
                        output.identifier_type,
                        output.amount,
                        output.is_minter,
                        "State",
                    )
                })
                .collect(),
        ));
    }
    args.push(Expr::bytes(signature));
    args.push(Expr::byte(witness));
    covenant_call(&contract, "transferPolicy", args, leader)
}

pub fn root_init_call(
    p: &ProtocolParams,
    g: &GovernanceParams,
    current_authority: Vec<u8>,
    next_asset_id: Vec<u8>,
    root_allocation: i64,
) -> Result<Vec<u8>, String> {
    let stack = compile_governance_stack(p, g)?;
    let mut previous = p.clone();
    previous.asset_id = vec![0; 32];
    let src = source("contracts/root-issuance.sil")?;
    let contract = compile_contract(
        &src,
        &root_args_with_state(&previous, &stack, false, 0, 0, current_authority.clone(), 0),
        CompileOptions::default(),
    )
    .map_err(|e| e.to_string())?;
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
pub fn root_bootstrap_module_call(
    p: &ProtocolParams,
    g: &GovernanceParams,
    current_authority: Vec<u8>,
    authority_signature: Vec<u8>,
    root_id: Vec<u8>,
    module_id: Vec<u8>,
    previous_module_nonce: i64,
    previous_remaining_allocation: i64,
    module_output_index: u8,
) -> Result<Vec<u8>, String> {
    let stack = compile_governance_stack(p, g)?;
    let src = source("contracts/root-issuance.sil")?;
    let contract = compile_contract(
        &src,
        &root_args_with_state(
            p,
            &stack,
            true,
            previous_module_nonce,
            previous_remaining_allocation,
            current_authority.clone(),
            0,
        ),
        CompileOptions::default(),
    )
    .map_err(|e| e.to_string())?;
    let next_root = struct_object(
        "State",
        vec![
            ("assetId", Expr::bytes(p.asset_id.clone())),
            ("initialized", Expr::bool(true)),
            ("moduleNonce", Expr::int(previous_module_nonce + 1)),
            (
                "remainingAllocation",
                Expr::int(previous_remaining_allocation - p.module_remaining_mint),
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
            Expr::dynamic_bytes(stack.base.module.prefix.clone()),
            Expr::dynamic_bytes(stack.base.module.suffix.clone()),
            module_state_expr(p, "ModuleState"),
            governance_state_expr(p, g, vec![0; 32], "GovernanceState"),
            token_state_expr(root_id, 2, 0, true, "KCC20State"),
            token_state_expr(module_id, 2, 0, true, "KCC20State"),
        ],
        true,
    )
}

pub(crate) fn position_state_for_module(
    p: &ProtocolParams,
    stack: &ProtocolStack,
    position_id: Vec<u8>,
    name: &'static str,
) -> Expr<'static> {
    let _ = position_id; // KCC owners carry this ID; Position state does not.
    struct_object(
        name,
        vec![
            ("owner", Expr::bytes(p.owner.clone())),
            ("assetId", Expr::bytes(p.asset_id.clone())),
            ("debt", Expr::int(p.debt)),
            ("assignedReserve", Expr::int(p.assigned_reserve)),
            ("reserveId", Expr::bytes(p.reserve_id.clone())),
            (
                "reserveContributionPpm",
                Expr::int(p.reserve_contribution_ppm),
            ),
            ("liquidationPrice", Expr::int(p.liquidation_price)),
            ("challengePeriodDaa", Expr::int(p.challenge_period_daa)),
            ("auctionDurationDaa", Expr::int(p.auction_duration_daa)),
            ("challengeRewardPpm", Expr::int(p.challenge_reward_ppm)),
            ("challengeId", Expr::bytes(vec![0; 32])),
            ("kccPrefixLen", Expr::int(stack.kcc.prefix.len() as i64)),
            ("kccSuffixLen", Expr::int(stack.kcc.suffix.len() as i64)),
            (
                "kccTemplateHash",
                Expr::bytes(stack.kcc.template_hash.clone()),
            ),
            (
                "positionPrefixLen",
                Expr::int(stack.position.prefix.len() as i64),
            ),
            (
                "positionSuffixLen",
                Expr::int(stack.position.suffix.len() as i64),
            ),
            (
                "positionTemplateHash",
                Expr::bytes(stack.position.template_hash.clone()),
            ),
        ],
    )
}

pub fn module_open_call(
    p: &ProtocolParams,
    position_id: Vec<u8>,
    position_output_index: u8,
) -> Result<Vec<u8>, String> {
    if position_id.len() != 32 {
        return Err("position_id: 32 bytes required".into());
    }
    let stack = compile_stack(p)?;
    let src = source("contracts/minting-module.sil")?;
    let contract = compile_contract(&src, &module_args(p, &stack), CompileOptions::default())
        .map_err(|e| e.to_string())?;
    let quote = crate::fees::MintTerms {
        reserve_contribution_ppm: p.reserve_contribution_ppm,
        annual_interest_ppm: p.risk_premium_ppm,
        daa_per_year: p.daa_per_year,
    }
    .quote(p.debt, p.module_expiration_daa - p.current_daa)
    .map_err(str::to_string)?;
    let contribution = quote.assigned_reserve_kusd;
    let fee = quote.equity_fee_kusd;
    let usable = p
        .debt
        .checked_sub(contribution)
        .and_then(|v| v.checked_sub(fee))
        .ok_or("underflow mint utilisable")?;
    if contribution != p.assigned_reserve || usable <= 0 {
        return Err("assignedReserve is incompatible with Module rates".into());
    }
    let mut next = p.clone();
    next.module_remaining_mint -= p.debt;
    next.position_nonce += 1;
    covenant_call(
        &contract,
        "openPolicy",
        vec![
            module_state_expr(&next, "State"),
            Expr::byte(position_output_index),
            Expr::dynamic_bytes(stack.position.prefix.clone()),
            Expr::dynamic_bytes(stack.position.suffix.clone()),
            position_state_for_module(p, &stack, position_id.clone(), "PositionState"),
            token_state_expr(p.module_id.clone(), 2, 0, true, "KCC20State"),
            token_state_expr(position_id.clone(), 2, 0, true, "KCC20State"),
            token_state_expr(p.owner.clone(), 0, usable, false, "KCC20State"),
            token_state_expr(position_id, 2, contribution, false, "KCC20State"),
            token_state_expr(p.reserve_id.clone(), 2, fee, false, "KCC20State"),
        ],
        true,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn reserve_initialize_call(
    p: &ProtocolParams,
    reserve_id: Vec<u8>,
    depositor: Vec<u8>,
    deposit_amount: i64,
) -> Result<Vec<u8>, String> {
    let mut previous = p.clone();
    previous.kps_id = vec![0; 32];
    previous.reserve_kusd = 0;
    previous.total_kps = 0;
    previous.reserve_collateral_sompi = 0;
    let stack = compile_stack(p)?;
    let src = source("contracts/equity-reserve-base.sil")?;
    let contract = compile_contract(
        &src,
        &reserve_args(&previous, &stack, 0, 0, 0),
        CompileOptions::default(),
    )
    .map_err(|e| e.to_string())?;
    covenant_call(
        &contract,
        "initializePolicy",
        vec![
            reserve_state_expr(p, &stack, "State"),
            Expr::bytes(depositor.clone()),
            Expr::int(deposit_amount),
            token_state_expr(reserve_id.clone(), 2, deposit_amount, false, "TokenState"),
            token_state_expr(reserve_id, 2, 0, true, "TokenState"),
            token_state_expr(depositor, 0, deposit_amount, false, "TokenState"),
        ],
        true,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn kps_transfer_call(
    p: &ProtocolParams,
    current_owner: Vec<u8>,
    current_amount: i64,
    current_identifier_type: u8,
    current_is_minter: bool,
    outputs: &[TokenOutput],
    signature: Vec<u8>,
    witness: u8,
    leader: bool,
) -> Result<Vec<u8>, String> {
    let signature = if signature.is_empty() {
        vec![0; 65]
    } else {
        signature
    };
    let stack = compile_stack(p)?;
    let src = source("contracts/kps.sil")?;
    let contract = compile_contract(
        &src,
        &kps_args(
            p,
            &stack.delegation,
            current_owner,
            current_amount,
            current_identifier_type,
            current_is_minter,
        ),
        CompileOptions::default(),
    )
    .map_err(|e| e.to_string())?;
    let mut args = Vec::new();
    if leader {
        args.push(Expr::array(
            parse_type_ref("State[]").unwrap(),
            outputs
                .iter()
                .map(|output| {
                    token_state_expr(
                        output.owner.clone(),
                        output.identifier_type,
                        output.amount,
                        output.is_minter,
                        "State",
                    )
                })
                .collect(),
        ));
    }
    args.push(Expr::bytes(signature));
    args.push(Expr::byte(witness));
    covenant_call(&contract, "transferPolicy", args, leader)
}
