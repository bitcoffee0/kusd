use clap::{Args, Parser, Subcommand};
use kaspa_kusd::protocol::{
    GovernanceParams, GovernanceVote, ProtocolParams, TokenOutput, compile_delegation_state,
    compile_governance_stack, compile_governance_state, compile_kcc_state, compile_kps_state,
    compile_module_state, compile_position_state, compile_proposal_state, compile_reserve_state,
    compile_root_state, delegation_mature_call, delegation_redelegate_call, delegation_unlock_call,
    delegation_veto_call, governance_finish_call, governance_propose_module_call,
    kcc_transfer_call, kps_lock_delegation_call, kps_locked_transfer_call, kps_transfer_call,
    kps_unlock_delegation_call, module_open_call, proposal_activate_call, proposal_finish_call,
    proposal_veto_call, reserve_collect_proposal_fee_call, reserve_governance_checkpoint_call,
    reserve_initialize_call, root_bootstrap_module_call, root_execute_module_call,
    root_handover_to_governance_call, root_init_call,
};
use serde_json::json;

const ZERO: &str = "0000000000000000000000000000000000000000000000000000000000000000";

#[derive(Parser)]
#[command(about = "Base protocol governance artifacts and transaction scripts")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Clone, Args)]
struct Economic {
    #[arg(long)]
    owner: String,
    #[arg(long, default_value = ZERO)]
    challenger: String,
    #[arg(long)]
    asset_id: String,
    #[arg(long)]
    kps_id: String,
    #[arg(long)]
    reserve_id: String,
    #[arg(long, default_value = ZERO)]
    module_id: String,
    #[arg(long, default_value = ZERO)]
    position_id: String,
    #[arg(long, default_value_t = 1_500_000_000)]
    debt: i64,
    #[arg(long, default_value_t = 150_000_000)]
    assigned_reserve: i64,
    #[arg(long, default_value_t = 100_000)]
    reserve_contribution_ppm: i64,
    #[arg(long, default_value_t = 20_000)]
    risk_premium_ppm: i64,
    #[arg(long, default_value_t = 31_536_000)]
    daa_per_year: i64,
    #[arg(long, default_value_t = 3_500_000)]
    liquidation_price: i64,
    #[arg(long, default_value_t = 3_600)]
    challenge_period_daa: i64,
    #[arg(long, default_value_t = 3_600)]
    auction_duration_daa: i64,
    #[arg(long, default_value_t = 10_000)]
    challenge_reward_ppm: i64,
    #[arg(long, default_value_t = 100_000_000_000)]
    collateral_sompi: i64,
    #[arg(long, default_value_t = 100_000_000)]
    minimum_collateral_sompi: i64,
    #[arg(long, default_value_t = 1)]
    current_daa: i64,
    #[arg(long, default_value_t = 2_000_000_000)]
    reserve_kusd: i64,
    #[arg(long, default_value_t = 2_000_000_000)]
    total_kps: i64,
    #[arg(long, default_value_t = 0)]
    reserve_collateral_sompi: i64,
    #[arg(long, default_value_t = 90)]
    minimum_kps_holding_daa: i64,
    #[arg(long, default_value_t = 4)]
    max_kps_vote_weight: i64,
    #[arg(long, default_value_t = 100_000_000_000)]
    module_remaining_mint: i64,
    #[arg(long, default_value_t = 2_000_000_000)]
    max_debt_per_position: i64,
    #[arg(long)]
    module_expiration_daa: i64,
    #[arg(long, default_value_t = 0)]
    position_nonce: i64,
}

#[derive(Clone, Args)]
struct Governance {
    #[arg(long)]
    proposer: String,
    #[arg(long)]
    governance_id: String,
    #[arg(long)]
    root_id: String,
    #[arg(long, default_value_t = 0)]
    proposal_nonce: i64,
    #[arg(long, default_value_t = 0)]
    execution_nonce: i64,
    #[arg(long, default_value_t = 86_400)]
    voting_delay_daa: i64,
    #[arg(long, default_value_t = 604_800)]
    execution_window_daa: i64,
    #[arg(long, default_value_t = 20_000)]
    veto_threshold_ppm: i64,
    #[arg(long, default_value_t = 100_000_000)]
    proposal_deposit_sompi: i64,
    #[arg(long, default_value_t = 100_000_000)]
    proposal_fee_kusd: i64,
    #[arg(long, default_value_t = 1_000_000)]
    min_module_allocation: i64,
    #[arg(long, default_value_t = 1_000_000_000_000)]
    max_module_allocation: i64,
    #[arg(
        long = "governance-max-debt-per-position",
        default_value_t = 100_000_000_000
    )]
    governance_max_debt_per_position: i64,
    #[arg(long, default_value_t = 100_000_000)]
    min_collateral_sompi: i64,
    #[arg(long, default_value_t = 100_000_000_000_000)]
    max_collateral_sompi: i64,
    #[arg(long, default_value_t = 100)]
    min_module_duration_daa: i64,
    #[arg(long, default_value_t = 100_000_000)]
    max_module_duration_daa: i64,
    #[arg(long, default_value_t = 10)]
    min_challenge_period_daa: i64,
    #[arg(long, default_value_t = 1_000_000)]
    max_challenge_period_daa: i64,
    #[arg(long, default_value_t = 10)]
    min_auction_duration_daa: i64,
    #[arg(long, default_value_t = 1_000_000)]
    max_auction_duration_daa: i64,
    #[arg(long, default_value_t = 200_000)]
    max_risk_premium_ppm: i64,
    #[arg(long, default_value_t = 10_000)]
    min_reserve_contribution_ppm: i64,
    #[arg(long, default_value_t = 500_000)]
    max_reserve_contribution_ppm: i64,
    #[arg(long, default_value_t = 1_000_000_000_000)]
    root_remaining_allocation: i64,
}

#[derive(Subcommand)]
enum Command {
    KccState {
        #[arg(long)]
        owner: String,
        #[arg(long)]
        amount: i64,
        #[arg(long)]
        identifier: u8,
        #[arg(long, action = clap::ArgAction::Set)]
        minter: bool,
    },
    KpsState {
        #[command(flatten)]
        economic: Economic,
        #[arg(long)]
        token_owner: String,
        #[arg(long)]
        amount: i64,
        #[arg(long)]
        identifier: u8,
        #[arg(long, action = clap::ArgAction::Set)]
        minter: bool,
    },
    ReserveState {
        #[command(flatten)]
        economic: Economic,
        #[arg(long)]
        state_reserve_kusd: i64,
        #[arg(long)]
        state_total_kps: i64,
        #[arg(long)]
        state_collateral_sompi: i64,
    },
    RootState {
        #[command(flatten)]
        economic: Economic,
        #[command(flatten)]
        governance: Governance,
        #[arg(long, action = clap::ArgAction::Set)]
        initialized: bool,
        #[arg(long)]
        module_nonce: i64,
        #[arg(long)]
        remaining_allocation: i64,
        #[arg(long)]
        authority: String,
        #[arg(long)]
        authority_type: u8,
    },
    ModuleState {
        #[command(flatten)]
        economic: Economic,
        #[arg(long)]
        remaining_mint: i64,
        #[arg(long)]
        state_position_nonce: i64,
    },
    PositionState {
        #[command(flatten)]
        economic: Economic,
        #[arg(long)]
        state_debt: i64,
        #[arg(long)]
        state_assigned_reserve: i64,
        #[arg(long, default_value = ZERO)]
        challenge_id: String,
    },
    Stack {
        #[command(flatten)]
        economic: Economic,
        #[command(flatten)]
        governance: Governance,
    },
    GovernanceState {
        #[command(flatten)]
        economic: Economic,
        #[command(flatten)]
        governance: Governance,
        #[arg(long, default_value = ZERO)]
        active_proposal_id: String,
    },
    ProposalState {
        #[command(flatten)]
        economic: Economic,
        #[command(flatten)]
        governance: Governance,
        #[arg(long, action = clap::ArgAction::Set)]
        activated: bool,
    },
    DelegationState {
        #[command(flatten)]
        economic: Economic,
        #[arg(long)]
        delegate: String,
        #[arg(long)]
        amount: i64,
        #[arg(long)]
        weight: i64,
    },
    ProposeModuleCall {
        #[command(flatten)]
        economic: Economic,
        #[command(flatten)]
        governance: Governance,
        #[arg(long)]
        proposal_id: String,
        #[arg(long, default_value_t = 1)]
        proposal_output: u8,
    },
    ActivateProposalCall {
        #[command(flatten)]
        economic: Economic,
        #[command(flatten)]
        governance: Governance,
    },
    GovernanceFinishCall {
        #[command(flatten)]
        economic: Economic,
        #[command(flatten)]
        governance: Governance,
        #[arg(long)]
        active_proposal_id: String,
        #[arg(long)]
        action: String,
    },
    ProposalFinishCall {
        #[command(flatten)]
        economic: Economic,
        #[command(flatten)]
        governance: Governance,
        #[arg(long)]
        action: String,
        #[arg(long)]
        refund_output: u8,
        #[arg(long)]
        fee_input: u8,
        #[arg(long)]
        fee_output: u8,
    },
    RootExecuteModuleCall {
        #[command(flatten)]
        economic: Economic,
        #[command(flatten)]
        governance: Governance,
        #[arg(long)]
        active_proposal_id: String,
        #[arg(long)]
        previous_module_nonce: i64,
        #[arg(long)]
        previous_remaining_allocation: i64,
        #[arg(long, default_value_t = 4)]
        module_output: u8,
    },
    RootHandoverCall {
        #[command(flatten)]
        economic: Economic,
        #[command(flatten)]
        governance: Governance,
        #[arg(long)]
        current_authority: String,
        #[arg(long)]
        authority_signature: String,
        #[arg(long, default_value_t = 0)]
        module_nonce: i64,
        #[arg(long)]
        remaining_allocation: i64,
    },
    RootInitCall {
        #[command(flatten)]
        economic: Economic,
        #[command(flatten)]
        governance: Governance,
        #[arg(long)]
        current_authority: String,
        #[arg(long)]
        next_asset_id: String,
        #[arg(long)]
        root_allocation: i64,
    },
    RootBootstrapModuleCall {
        #[command(flatten)]
        economic: Economic,
        #[command(flatten)]
        governance: Governance,
        #[arg(long)]
        current_authority: String,
        #[arg(long)]
        authority_signature: String,
        #[arg(long)]
        output_module_id: String,
        #[arg(long)]
        previous_module_nonce: i64,
        #[arg(long)]
        previous_remaining_allocation: i64,
        #[arg(long)]
        module_output: u8,
    },
    ModuleOpenCall {
        #[command(flatten)]
        economic: Economic,
        #[arg(long)]
        output_position_id: String,
        #[arg(long)]
        position_output: u8,
    },
    ReserveInitializeCall {
        #[command(flatten)]
        economic: Economic,
        #[arg(long)]
        output_reserve_id: String,
        #[arg(long)]
        depositor: String,
        #[arg(long)]
        deposit_amount: i64,
    },
    KccTransferCall {
        #[arg(long)]
        current_owner: String,
        #[arg(long)]
        current_amount: i64,
        #[arg(long)]
        current_identifier: u8,
        #[arg(long, action = clap::ArgAction::Set)]
        current_minter: bool,
        #[arg(long, default_value = "[]")]
        outputs_json: String,
        #[arg(long, default_value = "")]
        signature: String,
        #[arg(long, default_value_t = 0)]
        witness: u8,
        #[arg(long, action = clap::ArgAction::Set)]
        leader: bool,
    },
    KpsTransferCall {
        #[command(flatten)]
        economic: Economic,
        #[arg(long)]
        current_owner: String,
        #[arg(long)]
        current_amount: i64,
        #[arg(long)]
        current_identifier: u8,
        #[arg(long, action = clap::ArgAction::Set)]
        current_minter: bool,
        #[arg(long, default_value = "[]")]
        outputs_json: String,
        #[arg(long, default_value = "")]
        signature: String,
        #[arg(long, default_value_t = 0)]
        witness: u8,
        #[arg(long, action = clap::ArgAction::Set)]
        leader: bool,
    },
    LockDelegationCall {
        #[command(flatten)]
        economic: Economic,
        #[arg(long)]
        delegate: String,
        #[arg(long)]
        amount: i64,
        #[arg(long)]
        delegation_id: String,
        #[arg(long)]
        owner_signature: String,
        #[arg(long, default_value_t = 0)]
        witness: u8,
        #[arg(long)]
        delegation_output: u8,
        #[arg(long)]
        kps_output: u8,
    },
    ProposalVetoCall {
        #[command(flatten)]
        economic: Economic,
        #[command(flatten)]
        governance: Governance,
        #[arg(long)]
        votes_json: String,
        #[arg(long)]
        reserve_input: u8,
        #[arg(long)]
        refund_output: u8,
    },
    DelegationVetoCall {
        #[command(flatten)]
        economic: Economic,
        #[arg(long)]
        delegate: String,
        #[arg(long)]
        delegation_id: String,
        #[arg(long)]
        proposal_id: String,
        #[arg(long)]
        amount: i64,
        #[arg(long)]
        weight: i64,
        #[arg(long)]
        delegate_signature: String,
        #[arg(long)]
        kps_input: u8,
        #[arg(long)]
        kps_output: u8,
    },
    ReserveCheckpointCall {
        #[command(flatten)]
        economic: Economic,
    },
    ReserveCollectProposalFeeCall {
        #[command(flatten)]
        economic: Economic,
        #[arg(long)]
        proposal_id: String,
        #[arg(long)]
        fee_amount: i64,
    },
    KpsLockedTransferCall {
        #[command(flatten)]
        economic: Economic,
        #[arg(long)]
        delegation_id: String,
        #[arg(long)]
        amount: i64,
    },
    KpsUnlockCall {
        #[command(flatten)]
        economic: Economic,
        #[arg(long)]
        delegate: String,
        #[arg(long)]
        delegation_id: String,
        #[arg(long)]
        amount: i64,
        #[arg(long)]
        weight: i64,
        #[arg(long)]
        delegation_input: u8,
        #[arg(long)]
        kps_output: u8,
    },
    MatureDelegationCall {
        #[command(flatten)]
        economic: Economic,
        #[arg(long)]
        delegate: String,
        #[arg(long)]
        amount: i64,
        #[arg(long)]
        weight: i64,
        #[arg(long)]
        delegation_id: String,
        #[arg(long)]
        kps_input: u8,
        #[arg(long)]
        kps_output: u8,
    },
    RedelegateCall {
        #[command(flatten)]
        economic: Economic,
        #[arg(long)]
        old_delegate: String,
        #[arg(long)]
        new_delegate: String,
        #[arg(long)]
        amount: i64,
        #[arg(long)]
        weight: i64,
        #[arg(long)]
        delegation_id: String,
        #[arg(long)]
        owner_signature: String,
        #[arg(long)]
        kps_input: u8,
        #[arg(long)]
        kps_output: u8,
    },
    UnlockDelegationCall {
        #[command(flatten)]
        economic: Economic,
        #[arg(long)]
        delegate: String,
        #[arg(long)]
        amount: i64,
        #[arg(long)]
        weight: i64,
        #[arg(long)]
        owner_signature: String,
        #[arg(long)]
        kps_input: u8,
        #[arg(long)]
        kps_output: u8,
    },
}

fn b32(value: &str) -> Result<Vec<u8>, String> {
    let bytes = hex::decode(value).map_err(|e| e.to_string())?;
    if bytes.len() != 32 {
        return Err("32 bytes required".into());
    }
    Ok(bytes)
}

fn params(value: &Economic) -> Result<ProtocolParams, String> {
    Ok(ProtocolParams {
        owner: b32(&value.owner)?,
        challenger: b32(&value.challenger)?,
        asset_id: b32(&value.asset_id)?,
        kps_id: b32(&value.kps_id)?,
        reserve_id: b32(&value.reserve_id)?,
        module_id: b32(&value.module_id)?,
        position_id: b32(&value.position_id)?,
        debt: value.debt,
        assigned_reserve: value.assigned_reserve,
        reserve_contribution_ppm: value.reserve_contribution_ppm,
        risk_premium_ppm: value.risk_premium_ppm,
        daa_per_year: value.daa_per_year,
        liquidation_price: value.liquidation_price,
        challenge_period_daa: value.challenge_period_daa,
        auction_duration_daa: value.auction_duration_daa,
        challenge_reward_ppm: value.challenge_reward_ppm,
        collateral_sompi: value.collateral_sompi,
        minimum_collateral_sompi: value.minimum_collateral_sompi,
        current_daa: value.current_daa,
        reserve_kusd: value.reserve_kusd,
        total_kps: value.total_kps,
        reserve_collateral_sompi: value.reserve_collateral_sompi,
        minimum_kps_holding_daa: value.minimum_kps_holding_daa,
        max_kps_vote_weight: value.max_kps_vote_weight,
        module_remaining_mint: value.module_remaining_mint,
        max_debt_per_position: value.max_debt_per_position,
        module_expiration_daa: value.module_expiration_daa,
        position_nonce: value.position_nonce,
    })
}

fn governance(value: &Governance) -> Result<GovernanceParams, String> {
    Ok(GovernanceParams {
        proposer: b32(&value.proposer)?,
        governance_id: b32(&value.governance_id)?,
        root_id: b32(&value.root_id)?,
        proposal_nonce: value.proposal_nonce,
        execution_nonce: value.execution_nonce,
        voting_delay_daa: value.voting_delay_daa,
        execution_window_daa: value.execution_window_daa,
        veto_threshold_ppm: value.veto_threshold_ppm,
        proposal_deposit_sompi: value.proposal_deposit_sompi,
        proposal_fee_kusd: value.proposal_fee_kusd,
        min_module_allocation: value.min_module_allocation,
        max_module_allocation: value.max_module_allocation,
        max_debt_per_position: value.governance_max_debt_per_position,
        min_collateral_sompi: value.min_collateral_sompi,
        max_collateral_sompi: value.max_collateral_sompi,
        min_module_duration_daa: value.min_module_duration_daa,
        max_module_duration_daa: value.max_module_duration_daa,
        min_challenge_period_daa: value.min_challenge_period_daa,
        max_challenge_period_daa: value.max_challenge_period_daa,
        min_auction_duration_daa: value.min_auction_duration_daa,
        max_auction_duration_daa: value.max_auction_duration_daa,
        max_risk_premium_ppm: value.max_risk_premium_ppm,
        min_reserve_contribution_ppm: value.min_reserve_contribution_ppm,
        max_reserve_contribution_ppm: value.max_reserve_contribution_ppm,
        root_remaining_allocation: value.root_remaining_allocation,
    })
}

fn artifact(value: kaspa_kusd::artifact::Artifact) -> Result<(), String> {
    println!(
        "{}",
        serde_json::to_string(&value).map_err(|e| e.to_string())?
    );
    Ok(())
}

fn script(value: Result<Vec<u8>, String>) -> Result<(), String> {
    println!("{}", hex::encode(value?));
    Ok(())
}

fn token_outputs(raw: &str) -> Result<Vec<TokenOutput>, String> {
    let values: Vec<(String, u8, i64, bool)> =
        serde_json::from_str(raw).map_err(|e| e.to_string())?;
    values
        .into_iter()
        .map(|(owner, identifier_type, amount, is_minter)| {
            Ok(TokenOutput {
                owner: b32(&owner)?,
                identifier_type,
                amount,
                is_minter,
            })
        })
        .collect()
}

fn main() -> Result<(), String> {
    match Cli::parse().command {
        Command::KccState {
            owner,
            amount,
            identifier,
            minter,
        } => artifact(compile_kcc_state(b32(&owner)?, amount, identifier, minter)?),
        Command::KpsState {
            economic,
            token_owner,
            amount,
            identifier,
            minter,
        } => artifact(compile_kps_state(
            &params(&economic)?,
            b32(&token_owner)?,
            amount,
            identifier,
            minter,
        )?),
        Command::ReserveState {
            economic,
            state_reserve_kusd,
            state_total_kps,
            state_collateral_sompi,
        } => artifact(compile_reserve_state(
            &params(&economic)?,
            state_reserve_kusd,
            state_total_kps,
            state_collateral_sompi,
        )?),
        Command::RootState {
            economic,
            governance: gv,
            initialized,
            module_nonce,
            remaining_allocation,
            authority,
            authority_type,
        } => artifact(compile_root_state(
            &params(&economic)?,
            &governance(&gv)?,
            initialized,
            module_nonce,
            remaining_allocation,
            b32(&authority)?,
            authority_type,
        )?),
        Command::ModuleState {
            economic,
            remaining_mint,
            state_position_nonce,
        } => artifact(compile_module_state(
            &params(&economic)?,
            remaining_mint,
            state_position_nonce,
        )?),
        Command::PositionState {
            economic,
            state_debt,
            state_assigned_reserve,
            challenge_id,
        } => artifact(compile_position_state(
            &params(&economic)?,
            state_debt,
            state_assigned_reserve,
            b32(&challenge_id)?,
        )?),
        Command::Stack {
            economic,
            governance: gv,
        } => {
            let stack = compile_governance_stack(&params(&economic)?, &governance(&gv)?)?;
            println!(
                "{}",
                json!({
                    "kcc": hex::encode(stack.base.kcc.template_hash),
                    "kps": hex::encode(stack.base.kps.template_hash),
                    "delegation": hex::encode(stack.base.delegation.template_hash),
                    "reserve": hex::encode(stack.base.reserve.template_hash),
                    "auction": hex::encode(stack.base.auction.template_hash),
                    "challenge": hex::encode(stack.base.challenge.template_hash),
                    "position": hex::encode(stack.base.position.template_hash),
                    "module": hex::encode(stack.base.module.template_hash),
                    "proposal": hex::encode(stack.proposal.template_hash),
                    "governance": hex::encode(stack.governance.template_hash),
                    "root": hex::encode(stack.root.template_hash),
                })
            );
            Ok(())
        }
        Command::GovernanceState {
            economic,
            governance: gv,
            active_proposal_id,
        } => artifact(compile_governance_state(
            &params(&economic)?,
            &governance(&gv)?,
            b32(&active_proposal_id)?,
        )?),
        Command::ProposalState {
            economic,
            governance: gv,
            activated,
        } => artifact(compile_proposal_state(
            &params(&economic)?,
            &governance(&gv)?,
            activated,
        )?),
        Command::DelegationState {
            economic,
            delegate,
            amount,
            weight,
        } => {
            let p = params(&economic)?;
            artifact(compile_delegation_state(
                &p,
                p.owner.clone(),
                b32(&delegate)?,
                amount,
                weight,
            )?)
        }
        Command::ProposeModuleCall {
            economic,
            governance: gv,
            proposal_id,
            proposal_output,
        } => script(governance_propose_module_call(
            &params(&economic)?,
            &governance(&gv)?,
            b32(&proposal_id)?,
            proposal_output,
        )),
        Command::ActivateProposalCall {
            economic,
            governance: gv,
        } => script(proposal_activate_call(
            &params(&economic)?,
            &governance(&gv)?,
        )),
        Command::GovernanceFinishCall {
            economic,
            governance: gv,
            active_proposal_id,
            action,
        } => script(governance_finish_call(
            &params(&economic)?,
            &governance(&gv)?,
            b32(&active_proposal_id)?,
            &action,
        )),
        Command::ProposalFinishCall {
            economic,
            governance: gv,
            action,
            refund_output,
            fee_input,
            fee_output,
        } => script(proposal_finish_call(
            &params(&economic)?,
            &governance(&gv)?,
            &action,
            refund_output,
            fee_input,
            fee_output,
        )),
        Command::RootExecuteModuleCall {
            economic,
            governance: gv,
            active_proposal_id,
            previous_module_nonce,
            previous_remaining_allocation,
            module_output,
        } => script(root_execute_module_call(
            &params(&economic)?,
            &governance(&gv)?,
            b32(&active_proposal_id)?,
            b32(&gv.root_id)?,
            params(&economic)?.module_id,
            previous_module_nonce,
            previous_remaining_allocation,
            module_output,
        )),
        Command::RootHandoverCall {
            economic,
            governance: gv,
            current_authority,
            authority_signature,
            module_nonce,
            remaining_allocation,
        } => script(root_handover_to_governance_call(
            &params(&economic)?,
            &governance(&gv)?,
            b32(&current_authority)?,
            hex::decode(authority_signature).map_err(|e| e.to_string())?,
            module_nonce,
            remaining_allocation,
        )),
        Command::RootInitCall {
            economic,
            governance: gv,
            current_authority,
            next_asset_id,
            root_allocation,
        } => script(root_init_call(
            &params(&economic)?,
            &governance(&gv)?,
            b32(&current_authority)?,
            b32(&next_asset_id)?,
            root_allocation,
        )),
        Command::RootBootstrapModuleCall {
            economic,
            governance: gv,
            current_authority,
            authority_signature,
            output_module_id,
            previous_module_nonce,
            previous_remaining_allocation,
            module_output,
        } => script(root_bootstrap_module_call(
            &params(&economic)?,
            &governance(&gv)?,
            b32(&current_authority)?,
            hex::decode(authority_signature).map_err(|e| e.to_string())?,
            b32(&gv.root_id)?,
            b32(&output_module_id)?,
            previous_module_nonce,
            previous_remaining_allocation,
            module_output,
        )),
        Command::ModuleOpenCall {
            economic,
            output_position_id,
            position_output,
        } => script(module_open_call(
            &params(&economic)?,
            b32(&output_position_id)?,
            position_output,
        )),
        Command::ReserveInitializeCall {
            economic,
            output_reserve_id,
            depositor,
            deposit_amount,
        } => script(reserve_initialize_call(
            &params(&economic)?,
            b32(&output_reserve_id)?,
            b32(&depositor)?,
            deposit_amount,
        )),
        Command::KccTransferCall {
            current_owner,
            current_amount,
            current_identifier,
            current_minter,
            outputs_json,
            signature,
            witness,
            leader,
        } => script(kcc_transfer_call(
            b32(&current_owner)?,
            current_amount,
            current_identifier,
            current_minter,
            &token_outputs(&outputs_json)?,
            hex::decode(signature).map_err(|e| e.to_string())?,
            witness,
            leader,
        )),
        Command::KpsTransferCall {
            economic,
            current_owner,
            current_amount,
            current_identifier,
            current_minter,
            outputs_json,
            signature,
            witness,
            leader,
        } => script(kps_transfer_call(
            &params(&economic)?,
            b32(&current_owner)?,
            current_amount,
            current_identifier,
            current_minter,
            &token_outputs(&outputs_json)?,
            hex::decode(signature).map_err(|e| e.to_string())?,
            witness,
            leader,
        )),
        Command::LockDelegationCall {
            economic,
            delegate,
            amount,
            delegation_id,
            owner_signature,
            witness,
            delegation_output,
            kps_output,
        } => script(kps_lock_delegation_call(
            &params(&economic)?,
            hex::decode(owner_signature).map_err(|e| e.to_string())?,
            witness,
            b32(&delegate)?,
            amount,
            b32(&delegation_id)?,
            delegation_output,
            kps_output,
        )),
        Command::ProposalVetoCall {
            economic,
            governance: gv,
            votes_json,
            reserve_input,
            refund_output,
        } => {
            let raw: Vec<(String, String, String, i64, i64, u8)> =
                serde_json::from_str(&votes_json).map_err(|e| e.to_string())?;
            let votes = raw
                .into_iter()
                .map(
                    |(owner, delegate, delegation_id, amount, weight, delegation_input_index)| {
                        Ok(GovernanceVote {
                            owner: b32(&owner)?,
                            delegate: b32(&delegate)?,
                            delegation_id: b32(&delegation_id)?,
                            amount,
                            weight,
                            delegation_input_index,
                        })
                    },
                )
                .collect::<Result<Vec<_>, String>>()?;
            script(proposal_veto_call(
                &params(&economic)?,
                &governance(&gv)?,
                &votes,
                reserve_input,
                refund_output,
            ))
        }
        Command::DelegationVetoCall {
            economic,
            delegate,
            delegation_id,
            proposal_id,
            amount,
            weight,
            delegate_signature,
            kps_input,
            kps_output,
        } => {
            let p = params(&economic)?;
            let vote = GovernanceVote {
                owner: p.owner.clone(),
                delegate: b32(&delegate)?,
                delegation_id: b32(&delegation_id)?,
                amount,
                weight,
                delegation_input_index: 0,
            };
            script(delegation_veto_call(
                &p,
                &vote,
                b32(&proposal_id)?,
                hex::decode(delegate_signature).map_err(|e| e.to_string())?,
                kps_input,
                kps_output,
            ))
        }
        Command::ReserveCheckpointCall { economic } => {
            script(reserve_governance_checkpoint_call(&params(&economic)?))
        }
        Command::ReserveCollectProposalFeeCall {
            economic,
            proposal_id,
            fee_amount,
        } => script(reserve_collect_proposal_fee_call(
            &params(&economic)?,
            b32(&proposal_id)?,
            fee_amount,
        )),
        Command::KpsLockedTransferCall {
            economic,
            delegation_id,
            amount,
        } => script(kps_locked_transfer_call(
            &params(&economic)?,
            b32(&delegation_id)?,
            amount,
        )),
        Command::KpsUnlockCall {
            economic,
            delegate,
            delegation_id,
            amount,
            weight,
            delegation_input,
            kps_output,
        } => {
            let p = params(&economic)?;
            script(kps_unlock_delegation_call(
                &p,
                p.owner.clone(),
                b32(&delegate)?,
                b32(&delegation_id)?,
                amount,
                weight,
                delegation_input,
                kps_output,
            ))
        }
        Command::MatureDelegationCall {
            economic,
            delegate,
            amount,
            weight,
            delegation_id,
            kps_input,
            kps_output,
        } => {
            let p = params(&economic)?;
            script(delegation_mature_call(
                &p,
                p.owner.clone(),
                b32(&delegate)?,
                amount,
                weight,
                b32(&delegation_id)?,
                kps_input,
                kps_output,
            ))
        }
        Command::RedelegateCall {
            economic,
            old_delegate,
            new_delegate,
            amount,
            weight,
            delegation_id,
            owner_signature,
            kps_input,
            kps_output,
        } => {
            let p = params(&economic)?;
            script(delegation_redelegate_call(
                &p,
                p.owner.clone(),
                b32(&old_delegate)?,
                b32(&new_delegate)?,
                amount,
                weight,
                b32(&delegation_id)?,
                hex::decode(owner_signature).map_err(|e| e.to_string())?,
                kps_input,
                kps_output,
            ))
        }
        Command::UnlockDelegationCall {
            economic,
            delegate,
            amount,
            weight,
            owner_signature,
            kps_input,
            kps_output,
        } => {
            let p = params(&economic)?;
            script(delegation_unlock_call(
                &p,
                p.owner.clone(),
                b32(&delegate)?,
                amount,
                weight,
                hex::decode(owner_signature).map_err(|e| e.to_string())?,
                kps_input,
                kps_output,
            ))
        }
    }
}
