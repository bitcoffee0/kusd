use kaspa_kusd::protocol::*;

fn setup() -> (ProtocolParams, GovernanceParams) {
    (
        ProtocolParams {
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
            minimum_kps_holding_daa: 90,
            max_kps_vote_weight: 4,
            module_remaining_mint: 100_000_000_000,
            max_debt_per_position: 2_000_000_000,
            module_expiration_daa: 400_000_000,
            position_nonce: 0,
        },
        GovernanceParams {
            proposer: vec![8; 32],
            governance_id: vec![9; 32],
            root_id: vec![10; 32],
            proposal_nonce: 1,
            execution_nonce: 0,
            voting_delay_daa: 100,
            execution_window_daa: 200,
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
            root_remaining_allocation: 1_000_000_000_000,
        },
    )
}

#[test]
fn all_governance_cli_scripts_are_buildable() {
    let (p, g) = setup();
    let proposal_id = vec![11; 32];
    let delegation_id = vec![12; 32];
    let vote = GovernanceVote {
        owner: p.owner.clone(),
        delegate: vec![13; 32],
        delegation_id: delegation_id.clone(),
        amount: 50_000_000,
        weight: 4,
        delegation_input_index: 3,
    };

    let scripts = vec![
        root_init_call(
            &p,
            &g,
            p.owner.clone(),
            p.asset_id.clone(),
            g.root_remaining_allocation,
        )
        .unwrap(),
        root_bootstrap_module_call(
            &p,
            &g,
            p.owner.clone(),
            vec![0; 65],
            g.root_id.clone(),
            p.module_id.clone(),
            0,
            g.root_remaining_allocation,
            4,
        )
        .unwrap(),
        module_open_call(&p, p.position_id.clone(), 6).unwrap(),
        reserve_initialize_call(&p, p.reserve_id.clone(), p.owner.clone(), p.reserve_kusd).unwrap(),
        kcc_transfer_call(
            p.module_id.clone(),
            0,
            2,
            true,
            &[TokenOutput {
                owner: p.owner.clone(),
                identifier_type: 0,
                amount: 1,
                is_minter: false,
            }],
            vec![0; 65],
            0,
            true,
        )
        .unwrap(),
        kps_transfer_call(
            &p,
            p.reserve_id.clone(),
            0,
            2,
            true,
            &[TokenOutput {
                owner: p.owner.clone(),
                identifier_type: 0,
                amount: 1,
                is_minter: false,
            }],
            vec![0; 65],
            0,
            true,
        )
        .unwrap(),
        governance_propose_module_call(&p, &g, proposal_id.clone(), 1).unwrap(),
        proposal_activate_call(&p, &g).unwrap(),
        governance_finish_call(&p, &g, proposal_id.clone(), "execute").unwrap(),
        governance_finish_call(&p, &g, proposal_id.clone(), "veto").unwrap(),
        governance_finish_call(&p, &g, proposal_id.clone(), "cancel").unwrap(),
        proposal_finish_call(&p, &g, "execute", 5, 4, 6).unwrap(),
        proposal_finish_call(&p, &g, "cancel", 1, 2, 2).unwrap(),
        root_handover_to_governance_call(
            &p,
            &g,
            p.owner.clone(),
            vec![0; 65],
            0,
            g.root_remaining_allocation,
        )
        .unwrap(),
        root_execute_module_call(
            &p,
            &g,
            proposal_id.clone(),
            g.root_id.clone(),
            p.module_id.clone(),
            0,
            g.root_remaining_allocation,
            4,
        )
        .unwrap(),
        kps_lock_delegation_call(
            &p,
            vec![0; 65],
            0,
            vote.delegate.clone(),
            vote.amount,
            delegation_id.clone(),
            0,
            1,
        )
        .unwrap(),
        delegation_mature_call(
            &p,
            p.owner.clone(),
            vote.delegate.clone(),
            vote.amount,
            0,
            delegation_id.clone(),
            1,
            1,
        )
        .unwrap(),
        delegation_redelegate_call(
            &p,
            p.owner.clone(),
            vote.delegate.clone(),
            vec![14; 32],
            vote.amount,
            1,
            delegation_id.clone(),
            vec![0; 65],
            1,
            1,
        )
        .unwrap(),
        delegation_unlock_call(
            &p,
            p.owner.clone(),
            vote.delegate.clone(),
            vote.amount,
            1,
            vec![0; 65],
            1,
            1,
        )
        .unwrap(),
        proposal_veto_call(&p, &g, std::slice::from_ref(&vote), 2, 4).unwrap(),
        reserve_governance_checkpoint_call(&p).unwrap(),
        delegation_veto_call(&p, &vote, proposal_id, vec![0; 65], 4, 3).unwrap(),
        kps_locked_transfer_call(&p, delegation_id.clone(), vote.amount).unwrap(),
        kps_unlock_delegation_call(
            &p,
            p.owner.clone(),
            vote.delegate,
            delegation_id,
            vote.amount,
            vote.weight,
            0,
            1,
        )
        .unwrap(),
    ];
    assert!(scripts.iter().all(|script| script.len() > 100));

    assert!(compile_governance_state(&p, &g, vec![0; 32]).is_ok());
    assert!(compile_proposal_state(&p, &g, false).is_ok());
    assert!(compile_delegation_state(&p, p.owner.clone(), vec![13; 32], 1, 0).is_ok());
    assert!(compile_kcc_state(p.owner.clone(), 1, 0, false).is_ok());
    assert!(compile_kps_state(&p, p.owner.clone(), 1, 0, false).is_ok());
    assert!(compile_reserve_state(&p, p.reserve_kusd, p.total_kps, 0).is_ok());
    assert!(compile_module_state(&p, p.module_remaining_mint, 0).is_ok());
    assert!(compile_position_state(&p, 0, 0, vec![0; 32]).is_ok());
    assert!(compile_root_state(&p, &g, false, 0, 0, p.owner.clone(), 0).is_ok());
}

#[test]
fn liquidation_price_has_no_economic_upper_bound() {
    let (mut p, g) = setup();
    p.liquidation_price = i64::MAX;

    governance_propose_module_call(&p, &g, vec![11; 32], 1)
        .expect("a positive liquidation price is not capped by governance");

    p.liquidation_price = 0;
    assert!(governance_propose_module_call(&p, &g, vec![11; 32], 1).is_err());
}

#[test]
fn builders_reject_ambiguous_actions_and_ids() {
    let (p, g) = setup();
    assert!(governance_finish_call(&p, &g, vec![1; 32], "unknown").is_err());
    assert!(proposal_finish_call(&p, &g, "veto", 0, 0, 0).is_err());
    assert!(governance_propose_module_call(&p, &g, vec![1; 31], 0).is_err());
    assert!(proposal_veto_call(&p, &g, &[], 0, 0).is_err());
}

#[test]
fn module_proposals_must_stay_inside_the_immutable_economic_bounds() {
    let (p, g) = setup();
    let mut cases = Vec::new();

    let mut value = p.clone();
    value.module_remaining_mint = g.max_module_allocation + 1;
    cases.push(value);
    let mut value = p.clone();
    value.max_debt_per_position = g.max_debt_per_position + 1;
    cases.push(value);
    let mut value = p.clone();
    value.minimum_collateral_sompi = g.min_collateral_sompi - 1;
    cases.push(value);
    let mut value = p.clone();
    value.liquidation_price = 0;
    cases.push(value);
    let mut value = p.clone();
    value.challenge_period_daa = g.max_challenge_period_daa + 1;
    cases.push(value);
    let mut value = p.clone();
    value.auction_duration_daa = g.max_auction_duration_daa + 1;
    cases.push(value);
    let mut value = p.clone();
    value.risk_premium_ppm = g.max_risk_premium_ppm + 1;
    cases.push(value);
    let mut value = p.clone();
    value.reserve_contribution_ppm = g.max_reserve_contribution_ppm + 1;
    cases.push(value);

    for invalid in cases {
        assert!(compile_governance_stack(&invalid, &g).is_err());
    }

    assert!(compile_governance_stack(&p, &g).is_ok());
}
