use kaspa_kusd::protocol::ProtocolParams;
use kaspa_kusd::savings_protocol::*;

fn setup() -> (ProtocolParams, SavingsParams) {
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
            reserve_kusd: 100_000_000_000,
            total_kps: 50_000_000_000,
            reserve_collateral_sompi: 0,
            minimum_kps_holding_daa: 90,
            max_kps_vote_weight: 4,
            module_remaining_mint: 100_000_000_000,
            max_debt_per_position: 2_000_000_000,
            module_expiration_daa: 400_000_000,
            position_nonce: 0,
        },
        SavingsParams {
            owner: vec![1; 32],
            referrer: vec![8; 32],
            proposer: vec![9; 32],
            controller_id: vec![10; 32],
            governor_id: vec![11; 32],
            registry_id: vec![14; 32],
            active_proposal_id: vec![0; 32],
            enabled: false,
            proposal_nonce: 0,
            execution_nonce: 0,
            series_nonce: 0,
            account_nonce: 0,
            total_saved: 0,
            saved: 100_000_000_000,
            rate_ppm: 20_000,
            current_rate_ppm: 0,
            interest_delay_daa: 50,
            delay_remaining_daa: 50,
            max_accrual_daa: 10_000,
            remaining_accrual_daa: 10_000,
            daa_per_year: 10_000,
            referral_fee_ppm: 100_000,
            voting_delay_daa: 100,
            execution_window_daa: 1_000,
            veto_threshold_ppm: 200_000,
            proposal_deposit_sompi: 100_000_000,
            account_value_sompi: 100_000_000,
        },
    )
}

#[test]
fn full_stack_and_state_builders_are_stable() {
    let (base, p) = setup();
    let stack = compile_stack(&base, &p).unwrap();
    for artifact in [
        &stack.base.kcc,
        &stack.base.kps,
        &stack.base.auction,
        &stack.base.challenge,
        &stack.base.position,
        &stack.base.module,
        &stack.proposal,
        &stack.governor,
        &stack.account,
        &stack.controller,
        &stack.reserve,
        &stack.registry,
    ] {
        assert!(!artifact.bytecode.is_empty());
        assert_eq!(artifact.template_hash.len(), 32);
    }
    assert_eq!(
        compile_proposal_state(&base, &p, false)
            .unwrap()
            .template_hash,
        stack.proposal.template_hash
    );
    assert_eq!(
        compile_governor_state(&base, &p).unwrap().template_hash,
        stack.governor.template_hash
    );
    assert_eq!(
        compile_controller_state(&base, &p).unwrap().template_hash,
        stack.controller.template_hash
    );
    assert_eq!(
        compile_account_state(&base, &p).unwrap().template_hash,
        stack.account.template_hash
    );
    assert_eq!(
        compile_reserve_state(&base, &p).unwrap().template_hash,
        stack.reserve.template_hash
    );
}

#[test]
fn registry_breaks_runtime_id_cycle_without_changing_base_templates() {
    let (base, initialized) = setup();
    let mut genesis = initialized.clone();
    genesis.controller_id = vec![0; 32];
    genesis.governor_id = vec![0; 32];
    let before = compile_stack(&base, &genesis).unwrap();
    let after = compile_stack(&base, &initialized).unwrap();
    assert_eq!(before.registry.template_hash, after.registry.template_hash);
    assert_eq!(before.reserve.template_hash, after.reserve.template_hash);
    assert_eq!(
        before.base.auction.template_hash,
        after.base.auction.template_hash
    );
    assert_eq!(
        before.base.module.template_hash,
        after.base.module.template_hash
    );
    assert!(compile_registry_state(&genesis, false).is_ok());
    assert!(compile_registry_state(&initialized, true).is_ok());
    assert!(registry_initialize_call(&genesis, &initialized, vec![0; 65]).is_ok());
    assert!(registry_preserve_call(&initialized).is_ok());
}

#[test]
fn governance_and_savings_lifecycle_calls_are_buildable() {
    let (base, genesis) = setup();
    let mut pending = genesis.clone();
    pending.proposal_nonce = 1;
    pending.active_proposal_id = vec![12; 32];
    let mut executed_governor = pending.clone();
    executed_governor.active_proposal_id = vec![0; 32];
    executed_governor.execution_nonce = 1;
    let mut enabled = genesis.clone();
    enabled.enabled = true;
    enabled.series_nonce = 1;
    enabled.current_rate_ppm = enabled.rate_ppm;

    assert!(
        governor_propose_call(&base, &genesis, &pending, 1)
            .unwrap()
            .len()
            > 100
    );
    assert!(proposal_activate_call(&base, &pending).unwrap().len() > 100);
    assert!(
        governor_execute_call(&base, &pending, &executed_governor, 1)
            .unwrap()
            .len()
            > 100
    );
    assert!(proposal_execute_call(&base, &pending, 2).unwrap().len() > 100);
    assert!(
        controller_execute_call(&base, &genesis, &enabled, 0, 1)
            .unwrap()
            .len()
            > 100
    );

    let mut opened_controller = enabled.clone();
    opened_controller.account_nonce = 1;
    opened_controller.total_saved = genesis.saved;
    let mut account = enabled.clone();
    account.total_saved = 0;
    assert!(
        controller_open_call(
            &base,
            &enabled,
            &opened_controller,
            &account,
            1,
            vec![13; 32]
        )
        .unwrap()
        .len()
            > 100
    );

    let mut refreshed_account = account.clone();
    refreshed_account.saved += 18_000_000;
    refreshed_account.delay_remaining_daa = 0;
    refreshed_account.remaining_accrual_daa -= 1_000;
    let mut refreshed_controller = opened_controller.clone();
    refreshed_controller.total_saved += 18_000_000;
    let mut next_reserve = base.clone();
    next_reserve.reserve_kusd -= 3_500_000;
    assert!(
        controller_refresh_call(
            &base,
            &opened_controller,
            &refreshed_controller,
            &refreshed_account,
            1
        )
        .unwrap()
        .len()
            > 100
    );
    assert!(
        account_refresh_call(
            &base,
            &account,
            &refreshed_account,
            vec![0; 65],
            1_000,
            0,
            2,
            3,
            4,
            vec![13; 32],
            2_000_000
        )
        .unwrap()
        .len()
            > 100
    );
    assert!(
        reserve_refresh_call(
            &base,
            &opened_controller,
            &next_reserve,
            &refreshed_account,
            0,
            1,
            1,
            1_000,
            3,
            4
        )
        .unwrap()
        .len()
            > 100
    );

    let mut closed = refreshed_controller.clone();
    closed.total_saved = 0;
    assert!(
        controller_close_call(&base, &refreshed_controller, &closed, 1)
            .unwrap()
            .len()
            > 100
    );
    assert!(
        account_close_call(&base, &refreshed_account, &closed, vec![0; 65], 0, 2, 2)
            .unwrap()
            .len()
            > 100
    );
}

#[test]
fn invalid_domains_are_rejected_before_building() {
    let (base, mut p) = setup();
    p.rate_ppm = PPM + 1;
    assert!(compile_stack(&base, &p).is_err());
    p.rate_ppm = 20_000;
    p.controller_id.pop();
    assert!(compile_stack(&base, &p).is_err());
}

#[test]
fn network_calls_are_buildable() {
    let (base, savings) = setup();
    for call in [
        base_position_repay_call(&base, &savings, base.debt).unwrap(),
        base_position_close_call(&base, &savings, vec![0; 65]).unwrap(),
        base_challenged_position_avert_call(
            &base,
            &savings,
            base.challenger.clone(),
            vec![12; 32],
            0,
        )
        .unwrap(),
        base_challenge_avert_call(&base, &savings, base.owner.clone(), 1).unwrap(),
        base_auction_settle_call(&base, &savings, vec![13; 32], 0, 1, 1_800).unwrap(),
        base_reserve_redeem_call(&base, &savings, base.owner.clone(), base.total_kps, 3).unwrap(),
    ] {
        assert!(call.len() > 100);
    }
    assert!(base_position_repay_call(&base, &savings, base.debt + 1).is_err());
    assert!(base_position_close_call(&base, &savings, vec![0; 64]).is_err());
    assert!(
        base_reserve_redeem_call(&base, &savings, base.owner.clone(), base.total_kps + 1, 3,)
            .is_err()
    );
}
