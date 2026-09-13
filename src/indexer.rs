use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Outpoint {
    pub transaction_id: String,
    pub index: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleEconomicBounds {
    pub min_module_allocation: i64,
    pub max_module_allocation: i64,
    pub max_debt_per_position: i64,
    pub min_collateral_sompi: u64,
    pub max_collateral_sompi: u64,
    pub min_module_duration_daa: i64,
    pub max_module_duration_daa: i64,
    pub min_challenge_period_daa: i64,
    pub max_challenge_period_daa: i64,
    pub min_auction_duration_daa: i64,
    pub max_auction_duration_daa: i64,
    pub max_risk_premium_ppm: i64,
    pub min_reserve_contribution_ppm: i64,
    pub max_reserve_contribution_ppm: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Entity {
    Governance {
        proposal_nonce: i64,
        execution_nonce: i64,
        active_proposal_id: Option<String>,
        voting_delay_daa: i64,
        execution_window_daa: i64,
        veto_threshold_ppm: i64,
        proposal_deposit_sompi: u64,
        #[serde(default)]
        proposal_fee_kusd: i64,
        #[serde(default)]
        daa_per_year: i64,
        #[serde(default)]
        economic_bounds: Option<ModuleEconomicBounds>,
    },
    ModuleProposal {
        governance_id: String,
        proposer: String,
        proposal_nonce: i64,
        activated: bool,
        allocation: i64,
        max_debt_per_position: i64,
        #[serde(default)]
        minimum_collateral_sompi: u64,
        liquidation_price: i64,
        expiration_daa: i64,
        challenge_period_daa: i64,
        #[serde(default)]
        auction_duration_daa: i64,
        challenge_reward_ppm: i64,
        reserve_contribution_ppm: i64,
        #[serde(default)]
        risk_premium_ppm: i64,
        #[serde(default)]
        daa_per_year: i64,
        deposit_sompi: u64,
    },
    KpsDelegation {
        owner: String,
        delegate: String,
        amount: i64,
        weight: i64,
    },
    Root {
        remaining_allocation: i64,
        module_nonce: i64,
    },
    Module {
        remaining_mint: i64,
        position_nonce: i64,
        expiration_daa: i64,
    },
    Position {
        collateral_sompi: u64,
        debt: i64,
        challenge_id: Option<String>,
    },
    ChallengedPosition {
        collateral_sompi: u64,
        debt: i64,
        challenge_id: String,
    },
    ChallengeAnchor {
        collateral_sompi: u64,
        debt: i64,
    },
    Challenge {
        collateral_sompi: u64,
        debt: i64,
    },
    Auction {
        collateral_sompi: u64,
        debt: i64,
        reward: i64,
    },
    Reserve {
        kusd_balance: i64,
        total_kps: i64,
        collateral_sompi: u64,
    },
    SavingsRegistry {
        controller_id: Option<String>,
        initialized: bool,
    },
    SavingsGovernor {
        proposal_nonce: i64,
        execution_nonce: i64,
        active_proposal_id: Option<String>,
    },
    SavingsProposal {
        activated: bool,
        rate_ppm: i64,
        interest_delay_daa: i64,
        max_accrual_daa: i64,
    },
    SavingsController {
        enabled: bool,
        series_nonce: i64,
        account_nonce: i64,
        total_saved: i64,
        current_rate_ppm: i64,
        interest_delay_daa: i64,
        max_accrual_daa: i64,
    },
    SavingsAccount {
        owner: String,
        saved: i64,
        rate_ppm: i64,
        delay_remaining_daa: i64,
        remaining_accrual_daa: i64,
        referral_fee_ppm: i64,
    },
    Token {
        amount: i64,
        is_minter: bool,
    },
    EquityToken {
        amount: i64,
        is_minter: bool,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TrackedEntity {
    pub address: String,
    pub outpoint: Outpoint,
    pub covenant_id: String,
    pub entity: Entity,
}

/// Minimal RPC proof for a live UTXO. The address commits to the complete script
/// (template + SilverScript state), while Covenant ID commits to its lineage.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveUtxo {
    pub outpoint: Outpoint,
    pub address: String,
    pub covenant_id: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DeploymentManifest {
    pub network: String,
    #[serde(default)]
    pub protocol: Option<String>,
    #[serde(default)]
    pub history_scope: Option<String>,
    #[serde(default)]
    pub tracked: Vec<TrackedEntity>,
    #[serde(default)]
    pub history: Vec<ManifestTransaction>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ManifestCreated {
    pub outpoint: Outpoint,
    pub entity: Entity,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ManifestTransaction {
    pub transaction_id: String,
    #[serde(default)]
    pub governance_action: Option<GovernanceAction>,
    #[serde(default)]
    pub consumed: Vec<Outpoint>,
    #[serde(default)]
    pub created: Vec<ManifestCreated>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GovernanceAction {
    RootHandover,
    ProposeModule,
    ActivateProposal,
    ExecuteModule,
    VetoProposal,
    CancelExpiredProposal,
    LockDelegation,
    MatureDelegation,
    Redelegate,
    UnlockDelegation,
    ExecuteSavingsSeries,
    InitializeSavingsRegistry,
    OpenSavingsAccount,
    RefreshSavingsAccount,
    CloseSavingsAccount,
    ProposeSavings,
    ActivateSavingsProposal,
}

impl DeploymentManifest {
    pub fn load(path: &std::path::Path) -> Result<Self, Box<dyn std::error::Error>> {
        let manifest: Self = serde_json::from_str(&std::fs::read_to_string(path)?)?;
        if manifest.network != "testnet-10" {
            return Err(format!("unsupported manifest network: {}", manifest.network).into());
        }
        Ok(manifest)
    }
}

/// Validates RPC data before replaying history.
/// Unrelated UTXOs at the same addresses are ignored.
pub fn validate_live_utxos(
    manifest: &DeploymentManifest,
    rpc_entries: &[LiveUtxo],
) -> Result<std::collections::BTreeSet<Outpoint>, String> {
    let tracked = manifest
        .tracked
        .iter()
        .map(|value| (value.outpoint.clone(), value))
        .collect::<BTreeMap<_, _>>();
    if tracked.len() != manifest.tracked.len() {
        return Err("duplicate outpoint in manifest".into());
    }

    let mut live = std::collections::BTreeSet::new();
    for entry in rpc_entries {
        let Some(expected) = tracked.get(&entry.outpoint) else {
            continue;
        };
        if entry.address != expected.address {
            return Err(format!(
                "RPC address mismatch for {}:{}",
                entry.outpoint.transaction_id, entry.outpoint.index
            ));
        }
        if entry.covenant_id.as_deref() != Some(expected.covenant_id.as_str()) {
            return Err(format!(
                "RPC Covenant ID mismatch for {}:{}",
                entry.outpoint.transaction_id, entry.outpoint.index
            ));
        }
        if !live.insert(entry.outpoint.clone()) {
            return Err(format!(
                "duplicate RPC UTXO: {}:{}",
                entry.outpoint.transaction_id, entry.outpoint.index
            ));
        }
    }
    Ok(live)
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    #[serde(with = "outpoint_entity_map")]
    pub entities: BTreeMap<Outpoint, Entity>,
    pub circulating_supply: i64,
    pub position_debt: i64,
    #[serde(default)]
    pub kps_supply: i64,
    #[serde(default)]
    pub reserve_kusd: i64,
    #[serde(default)]
    pub reserve_total_kps: i64,
    #[serde(default)]
    pub reserve_collateral_sompi: u64,
    #[serde(default)]
    pub active_governance_proposals: u64,
    #[serde(default)]
    pub proposed_module_allocation: i64,
    #[serde(default)]
    pub delegated_kps: i64,
    #[serde(default)]
    pub weighted_kps_votes: i64,
    #[serde(default)]
    pub savings_balance: i64,
    #[serde(default)]
    pub savings_accounts: u64,
    #[serde(default)]
    pub savings_enabled: bool,
    #[serde(default)]
    pub savings_rate_ppm: i64,
    #[serde(default)]
    pub challenged_positions: u64,
    #[serde(default)]
    pub active_challenges: u64,
    #[serde(default)]
    pub active_auctions: u64,
}

mod outpoint_entity_map {
    use super::{Entity, Outpoint};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::collections::BTreeMap;

    pub fn serialize<S>(
        value: &BTreeMap<Outpoint, Entity>,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        value.iter().collect::<Vec<_>>().serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<BTreeMap<Outpoint, Entity>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let values = Vec::<(Outpoint, Entity)>::deserialize(deserializer)?;
        Ok(values.into_iter().collect())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppliedTransaction {
    pub transaction_id: String,
    pub consumed: Vec<(Outpoint, Entity)>,
    pub created: Vec<Outpoint>,
    pub previous_supply: i64,
    pub previous_debt: i64,
    #[serde(default)]
    pub previous_kps_supply: i64,
    #[serde(default)]
    pub previous_active_governance_proposals: u64,
    #[serde(default)]
    pub previous_proposed_module_allocation: i64,
    #[serde(default)]
    pub previous_delegated_kps: i64,
    #[serde(default)]
    pub previous_weighted_kps_votes: i64,
    #[serde(default)]
    pub previous_savings_balance: i64,
    #[serde(default)]
    pub previous_savings_accounts: u64,
    #[serde(default)]
    pub previous_savings_enabled: bool,
    #[serde(default)]
    pub previous_savings_rate_ppm: i64,
    #[serde(default)]
    pub previous_challenged_positions: u64,
    #[serde(default)]
    pub previous_active_challenges: u64,
    #[serde(default)]
    pub previous_active_auctions: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppliedBlock {
    pub hash: String,
    pub parent: String,
    pub daa_score: u64,
    pub transaction_count: usize,
}

#[derive(Default, Serialize, Deserialize)]
pub struct Indexer {
    snapshot: Snapshot,
    journal: Vec<AppliedTransaction>,
    #[serde(default)]
    blocks: Vec<AppliedBlock>,
}

impl Indexer {
    pub fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }

    pub fn tip(&self) -> Option<&AppliedBlock> {
        self.blocks.last()
    }

    pub fn apply_block(
        &mut self,
        hash: impl Into<String>,
        parent: impl Into<String>,
        daa_score: u64,
        transactions: Vec<(String, Vec<Outpoint>, Vec<(Outpoint, Entity)>)>,
    ) -> Result<(), String> {
        let hash = hash.into();
        let parent = parent.into();
        if let Some(tip) = self.tip() {
            if tip.hash != parent {
                return Err(format!(
                    "unexpected parent: {parent}, current tip: {}",
                    tip.hash
                ));
            }
            if daa_score <= tip.daa_score {
                return Err("DAA score must increase".into());
            }
        }
        let checkpoint = self.journal.len();
        for (txid, consumed, created) in transactions {
            if let Err(error) = self.apply(txid, &consumed, created) {
                while self.journal.len() > checkpoint {
                    let txid = self.journal.last().unwrap().transaction_id.clone();
                    self.rollback_tip(&txid)?;
                }
                return Err(error);
            }
        }
        self.blocks.push(AppliedBlock {
            hash,
            parent,
            daa_score,
            transaction_count: self.journal.len() - checkpoint,
        });
        Ok(())
    }

    pub fn rollback_block(&mut self, expected_hash: &str) -> Result<(), String> {
        let block = self.blocks.pop().ok_or("empty block journal")?;
        if block.hash != expected_hash {
            self.blocks.push(block);
            return Err("out-of-order block rollback rejected".into());
        }
        for _ in 0..block.transaction_count {
            let txid = self
                .journal
                .last()
                .ok_or("incomplete transaction journal")?
                .transaction_id
                .clone();
            self.rollback_tip(&txid)?;
        }
        Ok(())
    }

    pub fn load(path: &std::path::Path) -> Result<Self, Box<dyn std::error::Error>> {
        if !path.exists() {
            return Ok(Self::default());
        }
        Ok(serde_json::from_str(&std::fs::read_to_string(path)?)?)
    }

    pub fn rebuild_from_manifest(
        &mut self,
        manifest: &DeploymentManifest,
        live: &std::collections::BTreeSet<Outpoint>,
    ) -> Result<(), String> {
        let mut entities = BTreeMap::new();
        for tracked in &manifest.tracked {
            if live.contains(&tracked.outpoint) {
                if entities
                    .insert(tracked.outpoint.clone(), tracked.entity.clone())
                    .is_some()
                {
                    return Err("duplicate outpoint in manifest".into());
                }
            }
        }
        self.snapshot.entities = entities;
        self.journal.clear();
        self.blocks.clear();
        self.recalculate()
    }

    pub fn rebuild_history_from_manifest(
        &mut self,
        manifest: &DeploymentManifest,
        live: &std::collections::BTreeSet<Outpoint>,
    ) -> Result<(), String> {
        if manifest.history.is_empty() {
            return self.rebuild_from_manifest(manifest, live);
        }
        *self = Self::default();
        for tx in &manifest.history {
            self.validate_governance_action(tx)?;
            self.apply(
                tx.transaction_id.clone(),
                &tx.consumed,
                tx.created
                    .iter()
                    .map(|created| (created.outpoint.clone(), created.entity.clone()))
                    .collect(),
            )?;
        }
        let expected = manifest
            .tracked
            .iter()
            .filter(|tracked| live.contains(&tracked.outpoint))
            .map(|tracked| tracked.outpoint.clone())
            .collect::<std::collections::BTreeSet<_>>();
        let indexed = self
            .snapshot
            .entities
            .keys()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>();
        if indexed != expected {
            let missing = expected.difference(&indexed).collect::<Vec<_>>();
            let stale = indexed.difference(&expected).collect::<Vec<_>>();
            return Err(format!(
                "history/RPC mismatch; missing={missing:?}, spent={stale:?}"
            ));
        }
        Ok(())
    }

    fn validate_governance_action(&self, tx: &ManifestTransaction) -> Result<(), String> {
        let Some(action) = &tx.governance_action else {
            return Ok(());
        };
        let consumed = tx
            .consumed
            .iter()
            .filter_map(|outpoint| self.snapshot.entities.get(outpoint))
            .collect::<Vec<_>>();
        if consumed.len() != tx.consumed.len() {
            return Err(format!(
                "{}: governed action references an unknown outpoint",
                tx.transaction_id
            ));
        }
        let created = tx
            .created
            .iter()
            .map(|value| &value.entity)
            .collect::<Vec<_>>();
        let count = |values: &[&Entity], predicate: fn(&Entity) -> bool| {
            values.iter().filter(|value| predicate(value)).count()
        };
        let governance = |value: &Entity| matches!(value, Entity::Governance { .. });
        let proposal = |value: &Entity| matches!(value, Entity::ModuleProposal { .. });
        let root = |value: &Entity| matches!(value, Entity::Root { .. });
        let module = |value: &Entity| matches!(value, Entity::Module { .. });
        let reserve = |value: &Entity| matches!(value, Entity::Reserve { .. });
        let delegation = |value: &Entity| matches!(value, Entity::KpsDelegation { .. });
        let kps = |value: &Entity| matches!(value, Entity::EquityToken { .. });
        let savings_controller = |value: &Entity| matches!(value, Entity::SavingsController { .. });
        let savings_registry = |value: &Entity| matches!(value, Entity::SavingsRegistry { .. });
        let savings_account = |value: &Entity| matches!(value, Entity::SavingsAccount { .. });
        let savings_governor = |value: &Entity| matches!(value, Entity::SavingsGovernor { .. });
        let savings_proposal = |value: &Entity| matches!(value, Entity::SavingsProposal { .. });
        let token = |value: &Entity| matches!(value, Entity::Token { .. });
        let token_balance = |values: &[&Entity]| {
            values.iter().try_fold(0_i64, |total, value| match value {
                Entity::Token {
                    amount,
                    is_minter: false,
                } => total.checked_add(*amount),
                _ => Some(total),
            })
        };
        let token_minter = |value: &Entity| {
            matches!(
                value,
                Entity::Token {
                    is_minter: true,
                    ..
                }
            )
        };
        let reserve_balance = |values: &[&Entity]| {
            values.iter().find_map(|value| match value {
                Entity::Reserve { kusd_balance, .. } => Some(*kusd_balance),
                _ => None,
            })
        };
        let proposal_fee = consumed.iter().find_map(|value| match value {
            Entity::Governance {
                proposal_fee_kusd, ..
            } => Some(*proposal_fee_kusd),
            _ => None,
        });
        let valid = match action {
            GovernanceAction::RootHandover => {
                count(&consumed, root) == 1 && count(&created, root) == 1
            }
            GovernanceAction::ProposeModule => {
                count(&consumed, governance) == 1
                    && count(&consumed, reserve) == 0
                    && count(&consumed, token) == 1
                    && count(&created, governance) == 1
                    && count(&created, proposal) == 1
                    && count(&created, reserve) == 0
                    && count(&created, token) == 1
                    && matches!(
                        (token_balance(&consumed), token_balance(&created), proposal_fee),
                        (Some(before), Some(after), Some(fee)) if fee > 0 && before == fee && after == fee
                    )
            }
            GovernanceAction::ActivateProposal => {
                count(&consumed, proposal) == 1 && count(&created, proposal) == 1
            }
            GovernanceAction::ExecuteModule => {
                count(&consumed, governance) == 1
                    && count(&consumed, proposal) == 1
                    && count(&consumed, root) == 1
                    && count(&consumed, token) == 1
                    && count(&created, governance) == 1
                    && count(&created, root) == 1
                    && count(&created, module) == 1
                    && count(&created, token) == 3
                    && count(&created, token_minter) == 2
                    && token_balance(&consumed) == proposal_fee
                    && token_balance(&created) == proposal_fee
            }
            GovernanceAction::VetoProposal => {
                count(&consumed, governance) == 1
                    && count(&consumed, proposal) == 1
                    && count(&consumed, reserve) == 1
                    && count(&consumed, token) == 2
                    && count(&consumed, delegation) > 0
                    && count(&consumed, delegation) == count(&created, delegation)
                    && count(&consumed, kps) == count(&created, kps)
                    && count(&created, governance) == 1
                    && count(&created, reserve) == 1
                    && count(&created, token) == 1
                    && matches!(
                        (reserve_balance(&consumed), reserve_balance(&created), proposal_fee),
                        (Some(before), Some(after), Some(fee))
                            if fee > 0 && before.checked_add(fee) == Some(after)
                    )
                    && token_balance(&consumed) == token_balance(&created)
            }
            GovernanceAction::CancelExpiredProposal => {
                count(&consumed, governance) == 1
                    && count(&consumed, proposal) == 1
                    && count(&consumed, token) == 1
                    && count(&created, governance) == 1
                    && count(&created, proposal) == 0
                    && count(&created, token) == 1
                    && token_balance(&consumed) == proposal_fee
                    && token_balance(&created) == proposal_fee
            }
            GovernanceAction::LockDelegation => {
                count(&consumed, kps) == 1
                    && count(&created, kps) == 1
                    && count(&created, delegation) == 1
            }
            GovernanceAction::MatureDelegation | GovernanceAction::Redelegate => {
                count(&consumed, delegation) == 1
                    && count(&created, delegation) == 1
                    && count(&consumed, kps) == 1
                    && count(&created, kps) == 1
            }
            GovernanceAction::UnlockDelegation => {
                count(&consumed, delegation) == 1
                    && count(&created, delegation) == 0
                    && count(&consumed, kps) == 1
                    && count(&created, kps) == 1
            }
            GovernanceAction::ExecuteSavingsSeries => {
                count(&consumed, savings_governor) == 1
                    && count(&consumed, savings_proposal) == 1
                    && count(&consumed, savings_controller) == 1
                    && count(&created, savings_governor) == 1
                    && count(&created, savings_controller) == 1
            }
            GovernanceAction::ProposeSavings => {
                count(&consumed, savings_governor) == 1
                    && count(&created, savings_governor) == 1
                    && count(&created, savings_proposal) == 1
            }
            GovernanceAction::ActivateSavingsProposal => {
                count(&consumed, savings_proposal) == 1 && count(&created, savings_proposal) == 1
            }
            GovernanceAction::InitializeSavingsRegistry => {
                count(&consumed, savings_registry) == 1 && count(&created, savings_registry) == 1
            }
            GovernanceAction::OpenSavingsAccount => {
                count(&consumed, savings_controller) == 1
                    && count(&created, savings_controller) == 1
                    && count(&created, savings_account) == 1
                    && count(&consumed, token) == 1
                    && count(&created, token) == 1
            }
            GovernanceAction::RefreshSavingsAccount => {
                count(&consumed, savings_controller) == 1
                    && count(&consumed, savings_account) == 1
                    && count(&consumed, reserve) == 1
                    && count(&consumed, savings_registry) == 1
                    && count(&created, savings_controller) == 1
                    && count(&created, savings_account) == 1
                    && count(&created, reserve) == 1
                    && count(&created, savings_registry) == 1
            }
            GovernanceAction::CloseSavingsAccount => {
                count(&consumed, savings_controller) == 1
                    && count(&consumed, savings_account) == 1
                    && count(&created, savings_controller) == 1
                    && count(&created, savings_account) == 0
                    && count(&consumed, token) == 1
                    && count(&created, token) == 1
            }
        };
        if !valid {
            return Err(format!(
                "{}: invalid shape for governed action {action:?}",
                tx.transaction_id
            ));
        }
        Ok(())
    }

    pub fn save(&self, path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
        std::fs::write(path, serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }

    pub fn apply(
        &mut self,
        transaction_id: impl Into<String>,
        consumed: &[Outpoint],
        created: Vec<(Outpoint, Entity)>,
    ) -> Result<(), String> {
        let transaction_id = transaction_id.into();
        let checkpoint = self.snapshot.clone();
        let previous_supply = self.snapshot.circulating_supply;
        let previous_debt = self.snapshot.position_debt;
        let previous_kps_supply = self.snapshot.kps_supply;
        let previous_active_governance_proposals = self.snapshot.active_governance_proposals;
        let previous_proposed_module_allocation = self.snapshot.proposed_module_allocation;
        let previous_delegated_kps = self.snapshot.delegated_kps;
        let previous_weighted_kps_votes = self.snapshot.weighted_kps_votes;
        let previous_savings_balance = self.snapshot.savings_balance;
        let previous_savings_accounts = self.snapshot.savings_accounts;
        let previous_savings_enabled = self.snapshot.savings_enabled;
        let previous_savings_rate_ppm = self.snapshot.savings_rate_ppm;
        let previous_challenged_positions = self.snapshot.challenged_positions;
        let previous_active_challenges = self.snapshot.active_challenges;
        let previous_active_auctions = self.snapshot.active_auctions;
        let result = (|| {
            let mut removed = Vec::with_capacity(consumed.len());
            for outpoint in consumed {
                let entity = self.snapshot.entities.remove(outpoint).ok_or_else(|| {
                    format!(
                        "unknown outpoint: {}:{}",
                        outpoint.transaction_id, outpoint.index
                    )
                })?;
                removed.push((outpoint.clone(), entity));
            }
            let mut created_keys = Vec::with_capacity(created.len());
            for (outpoint, entity) in created {
                if outpoint.transaction_id != transaction_id {
                    return Err(format!(
                        "output {}:{} assigned to the wrong transaction {}",
                        outpoint.transaction_id, outpoint.index, transaction_id
                    ));
                }
                if self
                    .snapshot
                    .entities
                    .insert(outpoint.clone(), entity)
                    .is_some()
                {
                    return Err(format!(
                        "duplicate outpoint: {}:{}",
                        outpoint.transaction_id, outpoint.index
                    ));
                }
                created_keys.push(outpoint);
            }
            self.recalculate()?;
            Ok(AppliedTransaction {
                transaction_id,
                consumed: removed,
                created: created_keys,
                previous_supply,
                previous_debt,
                previous_kps_supply,
                previous_active_governance_proposals,
                previous_proposed_module_allocation,
                previous_delegated_kps,
                previous_weighted_kps_votes,
                previous_savings_balance,
                previous_savings_accounts,
                previous_savings_enabled,
                previous_savings_rate_ppm,
                previous_challenged_positions,
                previous_active_challenges,
                previous_active_auctions,
            })
        })();
        match result {
            Ok(applied) => {
                self.journal.push(applied);
                Ok(())
            }
            Err(error) => {
                self.snapshot = checkpoint;
                Err(error)
            }
        }
    }

    pub fn rollback_tip(&mut self, expected_transaction_id: &str) -> Result<(), String> {
        let applied = self.journal.pop().ok_or("empty journal")?;
        if applied.transaction_id != expected_transaction_id {
            self.journal.push(applied);
            return Err("out-of-order rollback rejected".into());
        }
        for outpoint in &applied.created {
            self.snapshot.entities.remove(outpoint);
        }
        for (outpoint, entity) in applied.consumed {
            self.snapshot.entities.insert(outpoint, entity);
        }
        self.snapshot.circulating_supply = applied.previous_supply;
        self.snapshot.position_debt = applied.previous_debt;
        self.snapshot.kps_supply = applied.previous_kps_supply;
        self.snapshot.active_governance_proposals = applied.previous_active_governance_proposals;
        self.snapshot.proposed_module_allocation = applied.previous_proposed_module_allocation;
        self.snapshot.delegated_kps = applied.previous_delegated_kps;
        self.snapshot.weighted_kps_votes = applied.previous_weighted_kps_votes;
        self.snapshot.savings_balance = applied.previous_savings_balance;
        self.snapshot.savings_accounts = applied.previous_savings_accounts;
        self.snapshot.savings_enabled = applied.previous_savings_enabled;
        self.snapshot.savings_rate_ppm = applied.previous_savings_rate_ppm;
        self.snapshot.challenged_positions = applied.previous_challenged_positions;
        self.snapshot.active_challenges = applied.previous_active_challenges;
        self.snapshot.active_auctions = applied.previous_active_auctions;
        // Derived aggregates, including those introduced by newer
        // indexer implementations, must always be restored from
        // entities rather than depend on historical journal formats.
        self.recalculate()
    }

    fn recalculate(&mut self) -> Result<(), String> {
        let mut supply = 0_i64;
        let mut debt = 0_i64;
        let mut kps_supply = 0_i64;
        let mut reserve = None;
        let mut active_governance_proposals = 0_u64;
        let mut proposed_module_allocation = 0_i64;
        let mut delegated_kps = 0_i64;
        let mut weighted_kps_votes = 0_i64;
        let mut savings_balance = 0_i64;
        let mut savings_accounts = 0_u64;
        let mut savings_controller = None;
        let mut savings_registry = None;
        let mut challenged_positions = 0_u64;
        let mut active_challenges = 0_u64;
        let mut active_auctions = 0_u64;
        for entity in self.snapshot.entities.values() {
            match entity {
                Entity::Token {
                    amount,
                    is_minter: false,
                } => supply = supply.checked_add(*amount).ok_or("supply overflow")?,
                Entity::Position { debt: value, .. } => {
                    debt = debt.checked_add(*value).ok_or("debt overflow")?
                }
                Entity::ChallengedPosition { debt: value, .. } => {
                    debt = debt.checked_add(*value).ok_or("debt overflow")?;
                    challenged_positions = challenged_positions
                        .checked_add(1)
                        .ok_or("challenged position count overflow")?;
                }
                Entity::Challenge { .. } => {
                    active_challenges = active_challenges
                        .checked_add(1)
                        .ok_or("challenge count overflow")?;
                }
                Entity::Auction { .. } => {
                    active_auctions = active_auctions
                        .checked_add(1)
                        .ok_or("auction count overflow")?;
                }
                Entity::EquityToken {
                    amount,
                    is_minter: false,
                } => {
                    kps_supply = kps_supply
                        .checked_add(*amount)
                        .ok_or("KPS supply overflow")?
                }
                Entity::Reserve {
                    kusd_balance,
                    total_kps,
                    collateral_sompi,
                } => {
                    if reserve.is_some() {
                        return Err("more than one live Reserve".into());
                    }
                    if *kusd_balance < 0 || *total_kps < 0 {
                        return Err("invalid Reserve state".into());
                    }
                    reserve = Some((*kusd_balance, *total_kps, *collateral_sompi));
                }
                Entity::ModuleProposal {
                    allocation,
                    activated,
                    ..
                } => {
                    proposed_module_allocation = proposed_module_allocation
                        .checked_add(*allocation)
                        .ok_or("proposed allocation overflow")?;
                    if *activated {
                        active_governance_proposals = active_governance_proposals
                            .checked_add(1)
                            .ok_or("active proposal count overflow")?;
                    }
                }
                Entity::KpsDelegation { amount, weight, .. } => {
                    if *amount <= 0 || *weight < 0 {
                        return Err("invalid KPS delegation state".into());
                    }
                    delegated_kps = delegated_kps
                        .checked_add(*amount)
                        .ok_or("delegated KPS overflow")?;
                    weighted_kps_votes = weighted_kps_votes
                        .checked_add(amount.checked_mul(*weight).ok_or("weighted KPS overflow")?)
                        .ok_or("weighted KPS total overflow")?;
                }
                Entity::SavingsAccount { saved, .. } => {
                    if *saved <= 0 {
                        return Err("invalid Savings account balance".into());
                    }
                    savings_balance = savings_balance
                        .checked_add(*saved)
                        .ok_or("Savings balance overflow")?;
                    savings_accounts = savings_accounts
                        .checked_add(1)
                        .ok_or("Savings account count overflow")?;
                }
                Entity::SavingsController {
                    enabled,
                    total_saved,
                    current_rate_ppm,
                    ..
                } => {
                    if savings_controller.is_some() {
                        return Err("more than one live SavingsController".into());
                    }
                    if *total_saved < 0 || !(0..=1_000_000).contains(current_rate_ppm) {
                        return Err("invalid SavingsController state".into());
                    }
                    savings_controller = Some((*enabled, *total_saved, *current_rate_ppm));
                }
                Entity::SavingsRegistry {
                    controller_id,
                    initialized,
                } => {
                    if savings_registry.is_some() {
                        return Err("more than one live SavingsRegistry".into());
                    }
                    if *initialized != controller_id.is_some() {
                        return Err("invalid SavingsRegistry state".into());
                    }
                    savings_registry = Some((controller_id.clone(), *initialized));
                }
                _ => {}
            }
        }
        if active_governance_proposals > 1 {
            return Err("more than one activated Governance proposal".into());
        }
        self.snapshot.circulating_supply = supply;
        self.snapshot.position_debt = debt;
        self.snapshot.kps_supply = kps_supply;
        let (reserve_kusd, reserve_total_kps, reserve_collateral_sompi) =
            reserve.unwrap_or_default();
        self.snapshot.reserve_kusd = reserve_kusd;
        self.snapshot.reserve_total_kps = reserve_total_kps;
        self.snapshot.reserve_collateral_sompi = reserve_collateral_sompi;
        self.snapshot.active_governance_proposals = active_governance_proposals;
        self.snapshot.proposed_module_allocation = proposed_module_allocation;
        self.snapshot.delegated_kps = delegated_kps;
        self.snapshot.weighted_kps_votes = weighted_kps_votes;
        if let Some((enabled, declared_total, rate_ppm)) = savings_controller {
            if declared_total != savings_balance {
                return Err(format!(
                    "divergent Savings totalSaved: controller={declared_total}, accounts={savings_balance}"
                ));
            }
            self.snapshot.savings_enabled = enabled;
            self.snapshot.savings_rate_ppm = rate_ppm;
        } else {
            self.snapshot.savings_enabled = false;
            self.snapshot.savings_rate_ppm = 0;
        }
        self.snapshot.savings_balance = savings_balance;
        self.snapshot.savings_accounts = savings_accounts;
        self.snapshot.challenged_positions = challenged_positions;
        self.snapshot.active_challenges = active_challenges;
        self.snapshot.active_auctions = active_auctions;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn op(tx: &str, index: u32) -> Outpoint {
        Outpoint {
            transaction_id: tx.into(),
            index,
        }
    }

    #[test]
    fn indexes_and_rolls_back_position_mint() {
        let mut indexer = Indexer::default();
        indexer
            .apply(
                "open",
                &[],
                vec![
                    (
                        op("open", 0),
                        Entity::Position {
                            collateral_sompi: 100_000_000_000,
                            debt: 1_500_000_000,
                            challenge_id: None,
                        },
                    ),
                    (
                        op("open", 1),
                        Entity::Token {
                            amount: 1_500_000_000,
                            is_minter: false,
                        },
                    ),
                ],
            )
            .unwrap();
        assert_eq!(indexer.snapshot().circulating_supply, 1_500_000_000);
        assert_eq!(indexer.snapshot().position_debt, 1_500_000_000);
        indexer.rollback_tip("open").unwrap();
        assert_eq!(indexer.snapshot(), &Snapshot::default());
    }

    #[test]
    fn refuses_out_of_order_reorg() {
        let mut indexer = Indexer::default();
        indexer
            .apply(
                "a",
                &[],
                vec![(
                    op("a", 0),
                    Entity::Root {
                        remaining_allocation: 10,
                        module_nonce: 0,
                    },
                )],
            )
            .unwrap();
        assert!(indexer.rollback_tip("b").is_err());
        assert!(indexer.snapshot().entities.contains_key(&op("a", 0)));
    }

    #[test]
    fn replaces_a_confirmed_branch_after_reorg() {
        let mut indexer = Indexer::default();
        indexer
            .apply_block(
                "b1",
                "genesis",
                10,
                vec![(
                    "root".into(),
                    vec![],
                    vec![(
                        op("root", 0),
                        Entity::Root {
                            remaining_allocation: 100,
                            module_nonce: 0,
                        },
                    )],
                )],
            )
            .unwrap();
        indexer
            .apply_block(
                "mint-branch",
                "b1",
                11,
                vec![(
                    "mint".into(),
                    vec![op("root", 0)],
                    vec![
                        (
                            op("mint", 0),
                            Entity::Root {
                                remaining_allocation: 70,
                                module_nonce: 1,
                            },
                        ),
                        (
                            op("mint", 1),
                            Entity::Module {
                                remaining_mint: 30,
                                position_nonce: 0,
                                expiration_daa: 20,
                            },
                        ),
                    ],
                )],
            )
            .unwrap();
        assert!(indexer.snapshot.entities.contains_key(&op("mint", 1)));

        indexer.rollback_block("mint-branch").unwrap();
        indexer
            .apply_block(
                "handover-branch",
                "b1",
                12,
                vec![(
                    "handover".into(),
                    vec![op("root", 0)],
                    vec![(
                        op("handover", 0),
                        Entity::Root {
                            remaining_allocation: 100,
                            module_nonce: 0,
                        },
                    )],
                )],
            )
            .unwrap();
        assert!(!indexer.snapshot.entities.contains_key(&op("mint", 1)));
        assert!(indexer.snapshot.entities.contains_key(&op("handover", 0)));
        assert_eq!(indexer.tip().unwrap().hash, "handover-branch");
    }

    #[test]
    fn indexes_reserve_backstop_without_counting_kps_as_kusd_supply() {
        let mut indexer = Indexer::default();
        indexer
            .apply(
                "fund-reserve",
                &[],
                vec![
                    (
                        op("fund-reserve", 0),
                        Entity::Reserve {
                            kusd_balance: 2_000_000_000,
                            total_kps: 2_000_000_000,
                            collateral_sompi: 0,
                        },
                    ),
                    (
                        op("fund-reserve", 1),
                        Entity::Token {
                            amount: 2_000_000_000,
                            is_minter: false,
                        },
                    ),
                    (
                        op("fund-reserve", 2),
                        Entity::Position {
                            collateral_sompi: 100_000_000_000,
                            debt: 1_500_000_000,
                            challenge_id: Some("challenge".into()),
                        },
                    ),
                    (
                        op("fund-reserve", 3),
                        Entity::EquityToken {
                            amount: 2_000_000_000,
                            is_minter: false,
                        },
                    ),
                ],
            )
            .unwrap();
        assert_eq!(indexer.snapshot().circulating_supply, 2_000_000_000);
        assert_eq!(indexer.snapshot().kps_supply, 2_000_000_000);
        assert_eq!(indexer.snapshot().position_debt, 1_500_000_000);
        assert_eq!(indexer.snapshot().reserve_kusd, 2_000_000_000);
        assert_eq!(indexer.snapshot().reserve_total_kps, 2_000_000_000);
        assert_eq!(indexer.snapshot().reserve_collateral_sompi, 0);

        indexer
            .apply(
                "backstop",
                &[
                    op("fund-reserve", 0),
                    op("fund-reserve", 1),
                    op("fund-reserve", 2),
                ],
                vec![
                    (
                        op("backstop", 0),
                        Entity::Reserve {
                            kusd_balance: 485_000_000,
                            total_kps: 2_000_000_000,
                            collateral_sompi: 100_000_000_000,
                        },
                    ),
                    (
                        op("backstop", 1),
                        Entity::Token {
                            amount: 485_000_000,
                            is_minter: false,
                        },
                    ),
                    (
                        op("backstop", 2),
                        Entity::Token {
                            amount: 15_000_000,
                            is_minter: false,
                        },
                    ),
                ],
            )
            .unwrap();
        assert_eq!(indexer.snapshot().circulating_supply, 500_000_000);
        assert_eq!(indexer.snapshot().kps_supply, 2_000_000_000);
        assert_eq!(indexer.snapshot().position_debt, 0);
        assert_eq!(indexer.snapshot().reserve_kusd, 485_000_000);
        assert_eq!(indexer.snapshot().reserve_total_kps, 2_000_000_000);
        assert_eq!(indexer.snapshot().reserve_collateral_sompi, 100_000_000_000);
    }

    fn governance_entity(
        active: Option<&str>,
        proposal_nonce: i64,
        execution_nonce: i64,
    ) -> Entity {
        Entity::Governance {
            proposal_nonce,
            execution_nonce,
            active_proposal_id: active.map(str::to_owned),
            voting_delay_daa: 100,
            execution_window_daa: 200,
            veto_threshold_ppm: 20_000,
            proposal_deposit_sompi: 100_000_000,
            proposal_fee_kusd: 100_000_000,
            daa_per_year: 31_536_000,
            economic_bounds: None,
        }
    }

    fn proposal_entity(activated: bool) -> Entity {
        Entity::ModuleProposal {
            governance_id: "governance".into(),
            proposer: "owner".into(),
            proposal_nonce: 1,
            activated,
            allocation: 100_000,
            max_debt_per_position: 10_000,
            minimum_collateral_sompi: 100_000_000,
            liquidation_price: 3_500_000,
            expiration_daa: 1_000_000,
            challenge_period_daa: 3_600,
            auction_duration_daa: 3_600,
            challenge_reward_ppm: 10_000,
            reserve_contribution_ppm: 100_000,
            risk_premium_ppm: 20_000,
            daa_per_year: 31_536_000,
            deposit_sompi: 100_000_000,
        }
    }

    #[test]
    fn replays_chained_governance_actions_and_derived_metrics() {
        let mut indexer = Indexer::default();
        indexer
            .apply(
                "genesis",
                &[],
                vec![
                    (op("genesis", 0), governance_entity(None, 0, 0)),
                    (
                        op("genesis", 1),
                        Entity::Root {
                            remaining_allocation: 1_000_000,
                            module_nonce: 0,
                        },
                    ),
                    (
                        op("genesis", 2),
                        Entity::Reserve {
                            kusd_balance: 1_000_000_000,
                            total_kps: 1_000_000_000,
                            collateral_sompi: 0,
                        },
                    ),
                    (
                        op("genesis", 3),
                        Entity::Token {
                            amount: 1_000_000_000,
                            is_minter: false,
                        },
                    ),
                    (
                        op("genesis", 4),
                        Entity::Token {
                            amount: 100_000_000,
                            is_minter: false,
                        },
                    ),
                ],
            )
            .unwrap();

        let propose = ManifestTransaction {
            transaction_id: "propose".into(),
            governance_action: Some(GovernanceAction::ProposeModule),
            consumed: vec![op("genesis", 0), op("genesis", 4)],
            created: vec![
                ManifestCreated {
                    outpoint: op("propose", 0),
                    entity: governance_entity(Some("proposal-id"), 1, 0),
                },
                ManifestCreated {
                    outpoint: op("propose", 1),
                    entity: proposal_entity(false),
                },
                ManifestCreated {
                    outpoint: op("propose", 2),
                    entity: Entity::Token {
                        amount: 100_000_000,
                        is_minter: false,
                    },
                },
            ],
        };
        indexer.validate_governance_action(&propose).unwrap();
        indexer
            .apply(
                &propose.transaction_id,
                &propose.consumed,
                propose
                    .created
                    .iter()
                    .map(|v| (v.outpoint.clone(), v.entity.clone()))
                    .collect(),
            )
            .unwrap();
        assert_eq!(indexer.snapshot().proposed_module_allocation, 100_000);
        assert_eq!(indexer.snapshot().active_governance_proposals, 0);

        let activate = ManifestTransaction {
            transaction_id: "activate".into(),
            governance_action: Some(GovernanceAction::ActivateProposal),
            consumed: vec![op("propose", 1)],
            created: vec![ManifestCreated {
                outpoint: op("activate", 0),
                entity: proposal_entity(true),
            }],
        };
        indexer.validate_governance_action(&activate).unwrap();
        indexer
            .apply(
                &activate.transaction_id,
                &activate.consumed,
                vec![(op("activate", 0), proposal_entity(true))],
            )
            .unwrap();
        assert_eq!(indexer.snapshot().active_governance_proposals, 1);

        let execute = ManifestTransaction {
            transaction_id: "execute".into(),
            governance_action: Some(GovernanceAction::ExecuteModule),
            consumed: vec![
                op("propose", 0),
                op("activate", 0),
                op("genesis", 1),
                op("propose", 2),
            ],
            created: vec![
                ManifestCreated {
                    outpoint: op("execute", 0),
                    entity: governance_entity(None, 1, 1),
                },
                ManifestCreated {
                    outpoint: op("execute", 1),
                    entity: Entity::Root {
                        remaining_allocation: 900_000,
                        module_nonce: 1,
                    },
                },
                ManifestCreated {
                    outpoint: op("execute", 2),
                    entity: Entity::Module {
                        remaining_mint: 100_000,
                        position_nonce: 0,
                        expiration_daa: 1_000_000,
                    },
                },
                ManifestCreated {
                    outpoint: op("execute", 3),
                    entity: Entity::Token {
                        amount: 0,
                        is_minter: true,
                    },
                },
                ManifestCreated {
                    outpoint: op("execute", 4),
                    entity: Entity::Token {
                        amount: 0,
                        is_minter: true,
                    },
                },
                ManifestCreated {
                    outpoint: op("execute", 5),
                    entity: Entity::Token {
                        amount: 100_000_000,
                        is_minter: false,
                    },
                },
            ],
        };
        indexer.validate_governance_action(&execute).unwrap();
        indexer
            .apply(
                &execute.transaction_id,
                &execute.consumed,
                execute
                    .created
                    .iter()
                    .map(|v| (v.outpoint.clone(), v.entity.clone()))
                    .collect(),
            )
            .unwrap();
        assert_eq!(indexer.snapshot().active_governance_proposals, 0);
        assert_eq!(indexer.snapshot().proposed_module_allocation, 0);
    }

    #[test]
    fn refuses_a_governance_action_with_missing_cross_contract_successor() {
        let mut indexer = Indexer::default();
        indexer
            .apply(
                "genesis",
                &[],
                vec![(op("genesis", 0), governance_entity(None, 0, 0))],
            )
            .unwrap();
        let forged = ManifestTransaction {
            transaction_id: "forged".into(),
            governance_action: Some(GovernanceAction::ProposeModule),
            consumed: vec![op("genesis", 0)],
            created: vec![ManifestCreated {
                outpoint: op("forged", 0),
                entity: governance_entity(Some("fake"), 1, 0),
            }],
        };
        assert!(indexer.validate_governance_action(&forged).is_err());
    }

    #[test]
    fn failed_governance_apply_is_atomic() {
        let mut indexer = Indexer::default();
        indexer
            .apply(
                "genesis",
                &[],
                vec![(op("genesis", 0), proposal_entity(true))],
            )
            .unwrap();
        let before = indexer.snapshot().clone();
        assert!(
            indexer
                .apply(
                    "second-active",
                    &[],
                    vec![(op("second-active", 0), proposal_entity(true))],
                )
                .is_err()
        );
        assert_eq!(indexer.snapshot(), &before);
    }
}
