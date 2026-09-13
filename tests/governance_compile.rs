use kaspa_kusd::protocol::{GovernanceParams, ProtocolParams, compile_governance_stack};

fn params() -> (ProtocolParams, GovernanceParams) {
    let base = ProtocolParams {
        owner: vec![1; 32],
        challenger: vec![2; 32],
        asset_id: vec![3; 32],
        kps_id: vec![4; 32],
        reserve_id: vec![5; 32],
        module_id: vec![6; 32],
        position_id: vec![7; 32],
        debt: 1_500_000_000,
        assigned_reserve: 150_000_000,
        reserve_contribution_ppm: 100_000,
        risk_premium_ppm: 20_000,
        daa_per_year: 31_536_000,
        liquidation_price: 3_500_000,
        challenge_period_daa: 3_600,
        auction_duration_daa: 3_600,
        challenge_reward_ppm: 10_000,
        collateral_sompi: 100_000_000_000,
        minimum_collateral_sompi: 1_000_000_000,
        current_daa: 399_000_000,
        reserve_kusd: 2_000_000_000,
        total_kps: 2_000_000_000,
        reserve_collateral_sompi: 0,
        minimum_kps_holding_daa: 86_400,
        max_kps_vote_weight: 4,
        module_remaining_mint: 100_000_000_000,
        max_debt_per_position: 2_000_000_000,
        module_expiration_daa: 400_000_000,
        position_nonce: 0,
    };
    let governance = GovernanceParams {
        proposer: vec![8; 32],
        governance_id: vec![9; 32],
        root_id: vec![10; 32],
        proposal_nonce: 1,
        execution_nonce: 0,
        voting_delay_daa: 604_800,
        execution_window_daa: 604_800,
        veto_threshold_ppm: 20_000,
        proposal_deposit_sompi: 100_000_000,
        proposal_fee_kusd: 100_000_000,
        min_module_allocation: 1_000_000,
        max_module_allocation: 1_000_000_000_000,
        max_debt_per_position: 100_000_000_000,
        min_collateral_sompi: 100_000_000,
        max_collateral_sompi: 100_000_000_000_000,
        min_module_duration_daa: 100,
        max_module_duration_daa: 100_000_000,
        min_challenge_period_daa: 10,
        max_challenge_period_daa: 1_000_000,
        min_auction_duration_daa: 10,
        max_auction_duration_daa: 1_000_000,
        max_risk_premium_ppm: 200_000,
        min_reserve_contribution_ppm: 10_000,
        max_reserve_contribution_ppm: 500_000,
        root_remaining_allocation: 100_000_000_000,
    };
    (base, governance)
}

#[test]
fn governance_proposal_and_root_compile_as_one_authenticated_stack() {
    let (base, governance) = params();
    let stack = compile_governance_stack(&base, &governance).unwrap();
    for artifact in [
        &stack.proposal,
        &stack.governance,
        &stack.root,
        &stack.base.module,
    ] {
        assert!(!artifact.bytecode.is_empty());
        assert_eq!(artifact.template_hash.len(), 32);
    }
}
