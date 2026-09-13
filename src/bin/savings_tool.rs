use clap::{Parser, Subcommand};
use kaspa_kusd::artifact::Artifact;
use kaspa_kusd::protocol::GovernanceParams;
use kaspa_kusd::protocol::ProtocolParams;
use kaspa_kusd::savings_protocol::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Config {
    base: ProtocolParams,
    savings: SavingsParams,
    #[serde(default)]
    governance: Option<GovernanceParams>,
}

#[derive(Parser)]
#[command(about = "Full protocol Savings artifacts and transaction scripts")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    FullStack {
        #[arg(long)]
        config: PathBuf,
    },
    Stack {
        #[arg(long)]
        config: PathBuf,
    },
    ProposalState {
        #[arg(long)]
        config: PathBuf,
        #[arg(long, action = clap::ArgAction::Set)]
        activated: bool,
    },
    GovernorState {
        #[arg(long)]
        config: PathBuf,
    },
    ControllerState {
        #[arg(long)]
        config: PathBuf,
    },
    AccountState {
        #[arg(long)]
        config: PathBuf,
    },
    ReserveState {
        #[arg(long)]
        config: PathBuf,
    },
    RegistryState {
        #[arg(long)]
        config: PathBuf,
        #[arg(long, action = clap::ArgAction::Set)]
        initialized: bool,
    },
    RegistryInitializeCall {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        next_config: PathBuf,
        #[arg(long, default_value = "")]
        owner_signature: String,
    },
    RegistryPreserveCall {
        #[arg(long)]
        config: PathBuf,
    },
    BaseRootState {
        #[arg(long)]
        config: PathBuf,
        #[arg(long, action = clap::ArgAction::Set)]
        initialized: bool,
        #[arg(long)]
        module_nonce: i64,
        #[arg(long)]
        remaining_allocation: i64,
        #[arg(long)]
        authority: String,
        #[arg(long)]
        authority_type: i64,
    },
    BaseModuleState {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        remaining_mint: i64,
        #[arg(long)]
        position_nonce: i64,
    },
    BasePositionState {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        debt: i64,
        #[arg(long)]
        assigned_reserve: i64,
        #[arg(long)]
        challenge_id: String,
    },
    BaseChallengedPositionState {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        debt: i64,
        #[arg(long)]
        assigned_reserve: i64,
        #[arg(long)]
        challenge_id: String,
    },
    BaseAnchorState {
        #[arg(long)]
        config: PathBuf,
    },
    BaseChallengeState {
        #[arg(long)]
        config: PathBuf,
    },
    BaseAuctionState {
        #[arg(long)]
        config: PathBuf,
    },
    BaseGovernanceState {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        active_proposal_id: String,
    },
    BaseProposalState {
        #[arg(long)]
        config: PathBuf,
        #[arg(long, action = clap::ArgAction::Set)]
        activated: bool,
    },
    BaseKpsState {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        owner: String,
        #[arg(long)]
        amount: i64,
        #[arg(long)]
        identifier: i64,
        #[arg(long, action = clap::ArgAction::Set)]
        minter: bool,
    },
    BaseRootInitCall {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        current_authority: String,
        #[arg(long)]
        next_asset_id: String,
        #[arg(long)]
        root_allocation: i64,
    },
    BaseRootBootstrapCall {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        current_authority: String,
        #[arg(long, default_value = "")]
        signature: String,
        #[arg(long)]
        root_id: String,
        #[arg(long)]
        module_id: String,
        #[arg(long)]
        previous_nonce: i64,
        #[arg(long)]
        previous_remaining: i64,
        #[arg(long)]
        module_output: u8,
    },
    BaseModuleOpenCall {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        position_id: String,
        #[arg(long)]
        position_output: u8,
    },
    BaseReserveInitializeCall {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        reserve_id: String,
        #[arg(long)]
        depositor: String,
        #[arg(long)]
        deposit_amount: i64,
    },
    BaseGovernanceProposeCall {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        proposal_id: String,
        #[arg(long)]
        proposal_output: u8,
    },
    BaseProposalActivateCall {
        #[arg(long)]
        config: PathBuf,
    },
    BaseGovernanceExecuteCall {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        active_proposal_id: String,
    },
    BaseProposalExecuteCall {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        refund_output: u8,
        #[arg(long)]
        fee_input: u8,
        #[arg(long)]
        fee_output: u8,
    },
    BaseRootHandoverCall {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        current_authority: String,
        #[arg(long, default_value = "")]
        signature: String,
        #[arg(long)]
        module_nonce: i64,
        #[arg(long)]
        remaining: i64,
    },
    BaseRootExecuteCall {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        root_id: String,
        #[arg(long)]
        module_id: String,
        #[arg(long)]
        previous_nonce: i64,
        #[arg(long)]
        previous_remaining: i64,
        #[arg(long)]
        module_output: u8,
    },
    BaseStartChallengeCall {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        challenger: String,
        #[arg(long)]
        challenge_id: String,
        #[arg(long)]
        challenge_output: u8,
    },
    BaseAnchorInitCall {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        challenger: String,
        #[arg(long)]
        challenge_output: u8,
    },
    BaseChallengeActivateCall {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        challenger: String,
        #[arg(long)]
        auction_output: u8,
    },
    BasePositionSettleCall {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        challenger: String,
        #[arg(long)]
        challenge_id: String,
    },
    BasePositionRepayCall {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        repay_amount: i64,
    },
    BasePositionCloseCall {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        owner_signature: String,
    },
    BaseChallengedPositionAvertCall {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        challenger: String,
        #[arg(long)]
        challenge_id: String,
        #[arg(long)]
        active_position_output: u8,
    },
    BaseChallengeAvertCall {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        buyer: String,
        #[arg(long)]
        deposit_output: u8,
    },
    BaseAuctionSettleCall {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        bidder: String,
        #[arg(long)]
        bidder_collateral_output: u8,
        #[arg(long)]
        challenger_deposit_output: u8,
        #[arg(long)]
        elapsed_daa: i64,
    },
    BaseReserveRedeemCall {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        redeemer: String,
        #[arg(long)]
        burned_shares: i64,
        #[arg(long)]
        collateral_output: u8,
    },
    BaseReserveCollectProposalFeeCall {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        proposal_id: String,
        #[arg(long)]
        fee_amount: i64,
    },
    BaseAuctionBackstopCall {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        challenger: String,
        #[arg(long)]
        next_reserve_kusd: i64,
        #[arg(long)]
        next_collateral_sompi: i64,
        #[arg(long)]
        reserve_input: u8,
        #[arg(long)]
        challenger_deposit_output: u8,
    },
    BaseReserveBackstopCall {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        challenger: String,
        #[arg(long)]
        next_reserve_kusd: i64,
        #[arg(long)]
        next_collateral_sompi: i64,
        #[arg(long)]
        auction_input: u8,
    },
    ProposeCall {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        next_config: PathBuf,
        #[arg(long, default_value_t = 1)]
        proposal_output: u8,
    },
    ActivateCall {
        #[arg(long)]
        config: PathBuf,
    },
    GovernorExecuteCall {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        next_config: PathBuf,
        #[arg(long, default_value_t = 1)]
        proposal_input: u8,
    },
    ProposalExecuteCall {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        refund_output: u8,
    },
    ControllerExecuteCall {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        next_config: PathBuf,
        #[arg(long)]
        governor_input: u8,
        #[arg(long)]
        proposal_input: u8,
    },
    ControllerInitializeCall {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        next_config: PathBuf,
        #[arg(long, default_value = "")]
        owner_signature: String,
    },
    OpenCall {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        next_config: PathBuf,
        #[arg(long)]
        account_config: PathBuf,
        #[arg(long)]
        account_id: String,
        #[arg(long)]
        account_output: u8,
    },
    ControllerRefreshCall {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        next_config: PathBuf,
        #[arg(long)]
        account_config: PathBuf,
        #[arg(long)]
        account_input: u8,
    },
    AccountRefreshCall {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        next_config: PathBuf,
        #[arg(long, default_value = "")]
        owner_signature: String,
        #[arg(long)]
        accrual_daa: i64,
        #[arg(long)]
        controller_input: u8,
        #[arg(long)]
        reserve_input: u8,
        #[arg(long)]
        account_token_input: u8,
        #[arg(long)]
        reserve_token_input: u8,
        #[arg(long)]
        account_id: String,
        #[arg(long)]
        referral_amount: i64,
    },
    ReserveRefreshCall {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        next_config: PathBuf,
        #[arg(long)]
        account_config: PathBuf,
        #[arg(long)]
        registry_input: u8,
        #[arg(long)]
        controller_input: u8,
        #[arg(long)]
        account_input: u8,
        #[arg(long)]
        accrual_daa: i64,
        #[arg(long)]
        account_token_input: u8,
        #[arg(long)]
        reserve_token_input: u8,
    },
    ControllerCloseCall {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        next_config: PathBuf,
        #[arg(long)]
        account_input: u8,
    },
    AccountCloseCall {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        next_config: PathBuf,
        #[arg(long, default_value = "")]
        owner_signature: String,
        #[arg(long)]
        controller_input: u8,
        #[arg(long)]
        account_token_input: u8,
        #[arg(long)]
        kas_output: u8,
    },
}

fn load(path: &Path) -> Result<Config, String> {
    let raw = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&raw).map_err(|e| format!("{}: {e}", path.display()))
}

fn governance(config: &Config) -> Result<&GovernanceParams, String> {
    config
        .governance
        .as_ref()
        .ok_or_else(|| "missing governance section in configuration".into())
}

fn b32(value: &str) -> Result<Vec<u8>, String> {
    let value = hex::decode(value).map_err(|e| e.to_string())?;
    if value.len() != 32 {
        return Err("32 bytes required".into());
    }
    Ok(value)
}

fn signature(value: &str) -> Result<Vec<u8>, String> {
    if value.is_empty() {
        Ok(vec![])
    } else {
        hex::decode(value).map_err(|e| e.to_string())
    }
}

fn artifact(value: Artifact) -> Result<(), String> {
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

fn main() -> Result<(), String> {
    match Cli::parse().command {
        Command::FullStack { config } => {
            let c = load(&config)?;
            let s = compile_full_stack(&c.base, &c.savings, governance(&c)?)?;
            println!(
                "{}",
                json!({
                    "kcc": hex::encode(s.savings.base.kcc.template_hash),
                    "kps": hex::encode(s.savings.base.kps.template_hash),
                    "reserve": hex::encode(s.savings.reserve.template_hash),
                    "auction": hex::encode(s.savings.base.auction.template_hash),
                    "challenge": hex::encode(s.savings.base.challenge.template_hash),
                    "position": hex::encode(s.savings.base.position.template_hash),
                    "module": hex::encode(s.savings.base.module.template_hash),
                    "root": hex::encode(s.base_governance.root.template_hash),
                    "governance": hex::encode(s.base_governance.governance.template_hash),
                    "module_proposal": hex::encode(s.base_governance.proposal.template_hash),
                    "savings_proposal": hex::encode(s.savings.proposal.template_hash),
                    "savings_governor": hex::encode(s.savings.governor.template_hash),
                    "savings_controller": hex::encode(s.savings.controller.template_hash),
                    "savings_account": hex::encode(s.savings.account.template_hash),
                    "savings_registry": hex::encode(s.savings.registry.template_hash),
                })
            );
            Ok(())
        }
        Command::Stack { config } => {
            let c = load(&config)?;
            let s = compile_stack(&c.base, &c.savings)?;
            println!(
                "{}",
                json!({
                    "kcc": hex::encode(s.base.kcc.template_hash),
                    "kps": hex::encode(s.base.kps.template_hash),
                    "reserve": hex::encode(s.reserve.template_hash),
                    "auction": hex::encode(s.base.auction.template_hash),
                    "challenge": hex::encode(s.base.challenge.template_hash),
                    "position": hex::encode(s.base.position.template_hash),
                    "module": hex::encode(s.base.module.template_hash),
                    "proposal": hex::encode(s.proposal.template_hash),
                    "governor": hex::encode(s.governor.template_hash),
                    "controller": hex::encode(s.controller.template_hash),
                    "account": hex::encode(s.account.template_hash),
                    "registry": hex::encode(s.registry.template_hash),
                })
            );
            Ok(())
        }
        Command::ProposalState { config, activated } => {
            let c = load(&config)?;
            artifact(compile_proposal_state(&c.base, &c.savings, activated)?)
        }
        Command::GovernorState { config } => {
            let c = load(&config)?;
            artifact(compile_governor_state(&c.base, &c.savings)?)
        }
        Command::ControllerState { config } => {
            let c = load(&config)?;
            artifact(compile_controller_state(&c.base, &c.savings)?)
        }
        Command::AccountState { config } => {
            let c = load(&config)?;
            artifact(compile_account_state(&c.base, &c.savings)?)
        }
        Command::ReserveState { config } => {
            let c = load(&config)?;
            artifact(compile_reserve_state(&c.base, &c.savings)?)
        }
        Command::RegistryState {
            config,
            initialized,
        } => {
            let c = load(&config)?;
            artifact(compile_registry_state(&c.savings, initialized)?)
        }
        Command::RegistryInitializeCall {
            config,
            next_config,
            owner_signature,
        } => {
            let a = load(&config)?;
            let b = load(&next_config)?;
            script(registry_initialize_call(
                &a.savings,
                &b.savings,
                signature(&owner_signature)?,
            ))
        }
        Command::RegistryPreserveCall { config } => {
            let c = load(&config)?;
            script(registry_preserve_call(&c.savings))
        }
        Command::BaseRootState {
            config,
            initialized,
            module_nonce,
            remaining_allocation,
            authority,
            authority_type,
        } => {
            let c = load(&config)?;
            artifact(compile_base_root_state(
                &c.base,
                &c.savings,
                governance(&c)?,
                initialized,
                module_nonce,
                remaining_allocation,
                b32(&authority)?,
                u8::try_from(authority_type).map_err(|_| "authority_type hors plage u8")?,
            )?)
        }
        Command::BaseModuleState {
            config,
            remaining_mint,
            position_nonce,
        } => {
            let c = load(&config)?;
            artifact(compile_base_module_state(
                &c.base,
                &c.savings,
                remaining_mint,
                position_nonce,
            )?)
        }
        Command::BasePositionState {
            config,
            debt,
            assigned_reserve,
            challenge_id,
        } => {
            let c = load(&config)?;
            artifact(compile_base_position_state(
                &c.base,
                &c.savings,
                debt,
                assigned_reserve,
                b32(&challenge_id)?,
            )?)
        }
        Command::BaseChallengedPositionState {
            config,
            debt,
            assigned_reserve,
            challenge_id,
        } => {
            let c = load(&config)?;
            artifact(compile_base_challenged_position_state(
                &c.base,
                &c.savings,
                debt,
                assigned_reserve,
                b32(&challenge_id)?,
            )?)
        }
        Command::BaseAnchorState { config } => {
            let c = load(&config)?;
            artifact(compile_base_anchor_state(&c.base, &c.savings)?)
        }
        Command::BaseChallengeState { config } => {
            let c = load(&config)?;
            artifact(compile_base_challenge_state(&c.base, &c.savings)?)
        }
        Command::BaseAuctionState { config } => {
            let c = load(&config)?;
            artifact(compile_base_auction_state(&c.base, &c.savings)?)
        }
        Command::BaseGovernanceState {
            config,
            active_proposal_id,
        } => {
            let c = load(&config)?;
            artifact(compile_base_governance_state(
                &c.base,
                &c.savings,
                governance(&c)?,
                b32(&active_proposal_id)?,
            )?)
        }
        Command::BaseProposalState { config, activated } => {
            let c = load(&config)?;
            artifact(compile_base_proposal_state(
                &c.base,
                &c.savings,
                governance(&c)?,
                activated,
            )?)
        }
        Command::BaseKpsState {
            config,
            owner,
            amount,
            identifier,
            minter,
        } => {
            let c = load(&config)?;
            artifact(compile_base_kps_state(
                &c.base,
                &c.savings,
                b32(&owner)?,
                amount,
                u8::try_from(identifier).map_err(|_| "identifier hors plage u8")?,
                minter,
            )?)
        }
        Command::BaseRootInitCall {
            config,
            current_authority,
            next_asset_id,
            root_allocation,
        } => {
            let c = load(&config)?;
            script(base_root_init_call(
                &c.base,
                &c.savings,
                governance(&c)?,
                b32(&current_authority)?,
                b32(&next_asset_id)?,
                root_allocation,
            ))
        }
        Command::BaseRootBootstrapCall {
            config,
            current_authority,
            signature: sig,
            root_id,
            module_id,
            previous_nonce,
            previous_remaining,
            module_output,
        } => {
            let c = load(&config)?;
            script(base_root_bootstrap_module_call(
                &c.base,
                &c.savings,
                governance(&c)?,
                b32(&current_authority)?,
                signature(&sig)?,
                b32(&root_id)?,
                b32(&module_id)?,
                previous_nonce,
                previous_remaining,
                module_output,
            ))
        }
        Command::BaseModuleOpenCall {
            config,
            position_id,
            position_output,
        } => {
            let c = load(&config)?;
            script(base_module_open_call(
                &c.base,
                &c.savings,
                b32(&position_id)?,
                position_output,
            ))
        }
        Command::BaseReserveInitializeCall {
            config,
            reserve_id,
            depositor,
            deposit_amount,
        } => {
            let c = load(&config)?;
            script(base_reserve_initialize_call(
                &c.base,
                &c.savings,
                b32(&reserve_id)?,
                b32(&depositor)?,
                deposit_amount,
            ))
        }
        Command::BaseGovernanceProposeCall {
            config,
            proposal_id,
            proposal_output,
        } => {
            let c = load(&config)?;
            script(base_governance_propose_call(
                &c.base,
                &c.savings,
                governance(&c)?,
                b32(&proposal_id)?,
                proposal_output,
            ))
        }
        Command::BaseProposalActivateCall { config } => {
            let c = load(&config)?;
            script(base_proposal_activate_call(
                &c.base,
                &c.savings,
                governance(&c)?,
            ))
        }
        Command::BaseGovernanceExecuteCall {
            config,
            active_proposal_id,
        } => {
            let c = load(&config)?;
            script(base_governance_execute_call(
                &c.base,
                &c.savings,
                governance(&c)?,
                b32(&active_proposal_id)?,
            ))
        }
        Command::BaseProposalExecuteCall {
            config,
            refund_output,
            fee_input,
            fee_output,
        } => {
            let c = load(&config)?;
            script(base_proposal_execute_call(
                &c.base,
                &c.savings,
                governance(&c)?,
                refund_output,
                fee_input,
                fee_output,
            ))
        }
        Command::BaseRootHandoverCall {
            config,
            current_authority,
            signature: sig,
            module_nonce,
            remaining,
        } => {
            let c = load(&config)?;
            script(base_root_handover_call(
                &c.base,
                &c.savings,
                governance(&c)?,
                b32(&current_authority)?,
                signature(&sig)?,
                module_nonce,
                remaining,
            ))
        }
        Command::BaseRootExecuteCall {
            config,
            root_id,
            module_id,
            previous_nonce,
            previous_remaining,
            module_output,
        } => {
            let c = load(&config)?;
            script(base_root_execute_module_call(
                &c.base,
                &c.savings,
                governance(&c)?,
                b32(&root_id)?,
                b32(&module_id)?,
                previous_nonce,
                previous_remaining,
                module_output,
            ))
        }
        Command::BaseStartChallengeCall {
            config,
            challenger,
            challenge_id,
            challenge_output,
        } => {
            let c = load(&config)?;
            script(base_start_challenge_call(
                &c.base,
                &c.savings,
                b32(&challenger)?,
                b32(&challenge_id)?,
                challenge_output,
            ))
        }
        Command::BaseAnchorInitCall {
            config,
            challenger,
            challenge_output,
        } => {
            let c = load(&config)?;
            script(base_anchor_init_call(
                &c.base,
                &c.savings,
                b32(&challenger)?,
                challenge_output,
            ))
        }
        Command::BaseChallengeActivateCall {
            config,
            challenger,
            auction_output,
        } => {
            let c = load(&config)?;
            script(base_challenge_activate_call(
                &c.base,
                &c.savings,
                b32(&challenger)?,
                auction_output,
            ))
        }
        Command::BasePositionSettleCall {
            config,
            challenger,
            challenge_id,
        } => {
            let c = load(&config)?;
            script(base_position_settle_auction_call(
                &c.base,
                &c.savings,
                b32(&challenger)?,
                b32(&challenge_id)?,
            ))
        }
        Command::BasePositionRepayCall {
            config,
            repay_amount,
        } => {
            let c = load(&config)?;
            script(base_position_repay_call(&c.base, &c.savings, repay_amount))
        }
        Command::BasePositionCloseCall {
            config,
            owner_signature,
        } => {
            let c = load(&config)?;
            script(base_position_close_call(
                &c.base,
                &c.savings,
                signature(&owner_signature)?,
            ))
        }
        Command::BaseChallengedPositionAvertCall {
            config,
            challenger,
            challenge_id,
            active_position_output,
        } => {
            let c = load(&config)?;
            script(base_challenged_position_avert_call(
                &c.base,
                &c.savings,
                b32(&challenger)?,
                b32(&challenge_id)?,
                active_position_output,
            ))
        }
        Command::BaseChallengeAvertCall {
            config,
            buyer,
            deposit_output,
        } => {
            let c = load(&config)?;
            script(base_challenge_avert_call(
                &c.base,
                &c.savings,
                b32(&buyer)?,
                deposit_output,
            ))
        }
        Command::BaseAuctionSettleCall {
            config,
            bidder,
            bidder_collateral_output,
            challenger_deposit_output,
            elapsed_daa,
        } => {
            let c = load(&config)?;
            script(base_auction_settle_call(
                &c.base,
                &c.savings,
                b32(&bidder)?,
                bidder_collateral_output,
                challenger_deposit_output,
                elapsed_daa,
            ))
        }
        Command::BaseReserveRedeemCall {
            config,
            redeemer,
            burned_shares,
            collateral_output,
        } => {
            let c = load(&config)?;
            script(base_reserve_redeem_call(
                &c.base,
                &c.savings,
                b32(&redeemer)?,
                burned_shares,
                collateral_output,
            ))
        }
        Command::BaseReserveCollectProposalFeeCall {
            config,
            proposal_id,
            fee_amount,
        } => {
            let c = load(&config)?;
            script(base_reserve_collect_proposal_fee_call(
                &c.base,
                &c.savings,
                b32(&proposal_id)?,
                fee_amount,
            ))
        }
        Command::BaseAuctionBackstopCall {
            config,
            challenger,
            next_reserve_kusd,
            next_collateral_sompi,
            reserve_input,
            challenger_deposit_output,
        } => {
            let c = load(&config)?;
            script(base_auction_backstop_call(
                &c.base,
                &c.savings,
                b32(&challenger)?,
                next_reserve_kusd,
                next_collateral_sompi,
                reserve_input,
                challenger_deposit_output,
            ))
        }
        Command::BaseReserveBackstopCall {
            config,
            challenger,
            next_reserve_kusd,
            next_collateral_sompi,
            auction_input,
        } => {
            let c = load(&config)?;
            script(base_reserve_backstop_call(
                &c.base,
                &c.savings,
                b32(&challenger)?,
                next_reserve_kusd,
                next_collateral_sompi,
                auction_input,
            ))
        }
        Command::ProposeCall {
            config,
            next_config,
            proposal_output,
        } => {
            let a = load(&config)?;
            let b = load(&next_config)?;
            script(governor_propose_call(
                &a.base,
                &a.savings,
                &b.savings,
                proposal_output,
            ))
        }
        Command::ActivateCall { config } => {
            let c = load(&config)?;
            script(proposal_activate_call(&c.base, &c.savings))
        }
        Command::GovernorExecuteCall {
            config,
            next_config,
            proposal_input,
        } => {
            let a = load(&config)?;
            let b = load(&next_config)?;
            script(governor_execute_call(
                &a.base,
                &a.savings,
                &b.savings,
                proposal_input,
            ))
        }
        Command::ProposalExecuteCall {
            config,
            refund_output,
        } => {
            let c = load(&config)?;
            script(proposal_execute_call(&c.base, &c.savings, refund_output))
        }
        Command::ControllerExecuteCall {
            config,
            next_config,
            governor_input,
            proposal_input,
        } => {
            let a = load(&config)?;
            let b = load(&next_config)?;
            script(controller_execute_call(
                &a.base,
                &a.savings,
                &b.savings,
                governor_input,
                proposal_input,
            ))
        }
        Command::ControllerInitializeCall {
            config,
            next_config,
            owner_signature,
        } => {
            let a = load(&config)?;
            let b = load(&next_config)?;
            script(controller_initialize_governance_call(
                &a.base,
                &a.savings,
                &b.savings,
                signature(&owner_signature)?,
            ))
        }
        Command::OpenCall {
            config,
            next_config,
            account_config,
            account_id,
            account_output,
        } => {
            let a = load(&config)?;
            let b = load(&next_config)?;
            let account = load(&account_config)?;
            script(controller_open_call(
                &a.base,
                &a.savings,
                &b.savings,
                &account.savings,
                account_output,
                b32(&account_id)?,
            ))
        }
        Command::ControllerRefreshCall {
            config,
            next_config,
            account_config,
            account_input,
        } => {
            let a = load(&config)?;
            let b = load(&next_config)?;
            let account = load(&account_config)?;
            script(controller_refresh_call(
                &a.base,
                &a.savings,
                &b.savings,
                &account.savings,
                account_input,
            ))
        }
        Command::AccountRefreshCall {
            config,
            next_config,
            owner_signature,
            accrual_daa,
            controller_input,
            reserve_input,
            account_token_input,
            reserve_token_input,
            account_id,
            referral_amount,
        } => {
            let a = load(&config)?;
            let b = load(&next_config)?;
            script(account_refresh_call(
                &a.base,
                &a.savings,
                &b.savings,
                signature(&owner_signature)?,
                accrual_daa,
                controller_input,
                reserve_input,
                account_token_input,
                reserve_token_input,
                b32(&account_id)?,
                referral_amount,
            ))
        }
        Command::ReserveRefreshCall {
            config,
            next_config,
            account_config,
            registry_input,
            controller_input,
            account_input,
            accrual_daa,
            account_token_input,
            reserve_token_input,
        } => {
            let a = load(&config)?;
            let b = load(&next_config)?;
            let account = load(&account_config)?;
            script(reserve_refresh_call(
                &a.base,
                &a.savings,
                &b.base,
                &account.savings,
                registry_input,
                controller_input,
                account_input,
                accrual_daa,
                account_token_input,
                reserve_token_input,
            ))
        }
        Command::ControllerCloseCall {
            config,
            next_config,
            account_input,
        } => {
            let a = load(&config)?;
            let b = load(&next_config)?;
            script(controller_close_call(
                &a.base,
                &a.savings,
                &b.savings,
                account_input,
            ))
        }
        Command::AccountCloseCall {
            config,
            next_config,
            owner_signature,
            controller_input,
            account_token_input,
            kas_output,
        } => {
            let a = load(&config)?;
            let b = load(&next_config)?;
            script(account_close_call(
                &a.base,
                &a.savings,
                &b.savings,
                signature(&owner_signature)?,
                controller_input,
                account_token_input,
                kas_output,
            ))
        }
    }
}
