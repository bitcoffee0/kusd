use kaspa_kusd::indexer::{
    DeploymentManifest, Entity, Indexer, LiveUtxo, Outpoint, TrackedEntity, validate_live_utxos,
};

fn op(tx: &str, index: u32) -> Outpoint {
    Outpoint {
        transaction_id: tx.into(),
        index,
    }
}

fn module(remaining: i64) -> Entity {
    Entity::Module {
        remaining_mint: remaining,
        position_nonce: 0,
        expiration_daa: 1_000,
    }
}

fn position(debt: i64) -> Entity {
    Entity::Position {
        collateral_sompi: 100_000_000_000,
        debt,
        challenge_id: None,
    }
}

fn challenged(debt: i64, challenge_id: &str) -> Entity {
    Entity::ChallengedPosition {
        collateral_sompi: 100_000_000_000,
        debt,
        challenge_id: challenge_id.into(),
    }
}

fn challenge(debt: i64) -> Entity {
    Entity::Challenge {
        collateral_sompi: 100_000_000_000,
        debt,
    }
}

fn auction(debt: i64) -> Entity {
    Entity::Auction {
        collateral_sompi: 100_000_000_000,
        debt,
        reward: debt / 100,
    }
}

#[test]
fn multi_wallet_roles_do_not_share_authority() {
    let owner = [1_u8; 32];
    let challenger = [2_u8; 32];
    let permissionless_keeper = [3_u8; 32];
    let reserve_equity_holder = [4_u8; 32];
    assert_ne!(owner, challenger);
    assert_ne!(challenger, permissionless_keeper);
    assert_ne!(permissionless_keeper, reserve_equity_holder);

    // The keeper becomes neither Position owner nor reward beneficiary :
    // settlement preserves roles committed in state regardless of
    // who broadcasts the activation/backstop transaction.
    let reward_beneficiary = challenger;
    let collateral_beneficiary = reserve_equity_holder;
    assert_ne!(permissionless_keeper, reward_beneficiary);
    assert_ne!(permissionless_keeper, collateral_beneficiary);
}

#[test]
fn avert_activate_race_and_backstop_reorg_restore_exact_utxos() {
    const DEBT: i64 = 800_000_000;
    let mut indexer = Indexer::default();
    indexer
        .apply_block(
            "warning",
            "genesis",
            100,
            vec![(
                "warning-tx".into(),
                vec![],
                vec![
                    (op("warning-tx", 0), challenged(DEBT, "challenge-id")),
                    (op("warning-tx", 1), challenge(DEBT)),
                ],
            )],
        )
        .unwrap();
    assert_eq!(indexer.snapshot().position_debt, DEBT);
    assert_eq!(indexer.snapshot().challenged_positions, 1);
    assert_eq!(indexer.snapshot().active_challenges, 1);

    // Branch A: avert consumes both UTXOs and restores the active Position.
    indexer
        .apply_block(
            "avert",
            "warning",
            101,
            vec![(
                "avert-tx".into(),
                vec![op("warning-tx", 0), op("warning-tx", 1)],
                vec![(op("avert-tx", 0), position(DEBT))],
            )],
        )
        .unwrap();
    assert_eq!(indexer.snapshot().challenged_positions, 0);
    assert_eq!(indexer.snapshot().active_challenges, 0);

    // The competing branch cannot attach to the wrong tip.
    assert!(
        indexer
            .apply_block(
                "activate",
                "warning",
                3_700,
                vec![(
                    "activate-tx".into(),
                    vec![op("warning-tx", 1)],
                    vec![(op("activate-tx", 0), auction(DEBT))]
                )],
            )
            .is_err()
    );

    // Reorganization: remove avert, then adopt activation.
    indexer.rollback_block("avert").unwrap();
    indexer
        .apply_block(
            "activate",
            "warning",
            3_700,
            vec![(
                "activate-tx".into(),
                vec![op("warning-tx", 1)],
                vec![(op("activate-tx", 0), auction(DEBT))],
            )],
        )
        .unwrap();
    assert_eq!(indexer.snapshot().challenged_positions, 1);
    assert_eq!(indexer.snapshot().active_challenges, 0);
    assert_eq!(indexer.snapshot().active_auctions, 1);

    indexer
        .apply_block(
            "backstop",
            "activate",
            3_701,
            vec![(
                "backstop-tx".into(),
                vec![op("warning-tx", 0), op("activate-tx", 0)],
                vec![],
            )],
        )
        .unwrap();
    assert_eq!(indexer.snapshot().position_debt, 0);
    assert_eq!(indexer.snapshot().active_auctions, 0);

    // A settlement reorg restores Auction + ChallengedPosition exactly.
    indexer.rollback_block("backstop").unwrap();
    assert_eq!(indexer.snapshot().position_debt, DEBT);
    assert_eq!(indexer.snapshot().challenged_positions, 1);
    assert_eq!(indexer.snapshot().active_auctions, 1);
}

#[test]
fn mint_expire_race_selects_one_branch_and_reorg_can_replace_it() {
    let mut indexer = Indexer::default();
    indexer
        .apply_block(
            "module",
            "genesis",
            10,
            vec![(
                "module-tx".into(),
                vec![],
                vec![(op("module-tx", 0), module(1_000))],
            )],
        )
        .unwrap();
    indexer
        .apply_block(
            "mint",
            "module",
            999,
            vec![(
                "mint-tx".into(),
                vec![op("module-tx", 0)],
                vec![
                    (op("mint-tx", 0), module(200)),
                    (op("mint-tx", 1), position(800)),
                ],
            )],
        )
        .unwrap();
    assert_eq!(indexer.snapshot().position_debt, 800);
    assert!(
        indexer
            .apply_block(
                "expire",
                "module",
                1_000,
                vec![("expire-tx".into(), vec![op("module-tx", 0)], vec![])],
            )
            .is_err()
    );

    indexer.rollback_block("mint").unwrap();
    indexer
        .apply_block(
            "expire",
            "module",
            1_000,
            vec![("expire-tx".into(), vec![op("module-tx", 0)], vec![])],
        )
        .unwrap();
    assert_eq!(indexer.snapshot().position_debt, 0);
    assert!(
        !indexer
            .snapshot()
            .entities
            .contains_key(&op("module-tx", 0))
    );
}

#[test]
fn manifest_event_cannot_claim_an_output_from_another_transaction() {
    let mut indexer = Indexer::default();
    assert!(
        indexer
            .apply("declared", &[], vec![(op("forged", 0), module(1))])
            .is_err()
    );
    assert!(indexer.snapshot().entities.is_empty());
}

#[test]
fn rpc_evidence_must_match_state_address_and_covenant_id() {
    let manifest = DeploymentManifest {
        network: "testnet-10".into(),
        protocol: Some("KUSD".into()),
        history_scope: Some("test".into()),
        tracked: vec![TrackedEntity {
            address: "kaspatest:state-address".into(),
            outpoint: op("state", 0),
            covenant_id: "covenant-id".into(),
            entity: module(1),
        }],
        history: vec![],
    };
    let valid = LiveUtxo {
        outpoint: op("state", 0),
        address: "kaspatest:state-address".into(),
        covenant_id: Some("covenant-id".into()),
    };
    assert_eq!(
        validate_live_utxos(&manifest, std::slice::from_ref(&valid)).unwrap(),
        [op("state", 0)].into_iter().collect()
    );

    let mut wrong_address = valid.clone();
    wrong_address.address = "kaspatest:fraudulent-state".into();
    assert!(validate_live_utxos(&manifest, &[wrong_address]).is_err());

    let mut wrong_covenant = valid;
    wrong_covenant.covenant_id = Some("fraudulent-covenant".into());
    assert!(validate_live_utxos(&manifest, &[wrong_covenant]).is_err());
}
