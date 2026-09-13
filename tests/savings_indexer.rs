use kaspa_kusd::indexer::{Entity, Indexer, Outpoint};

fn op(tx: &str, index: u32) -> Outpoint {
    Outpoint {
        transaction_id: tx.into(),
        index,
    }
}

fn controller(total_saved: i64, enabled: bool) -> Entity {
    Entity::SavingsController {
        enabled,
        series_nonce: i64::from(enabled),
        account_nonce: i64::from(total_saved > 0),
        total_saved,
        current_rate_ppm: if enabled { 20_000 } else { 0 },
        interest_delay_daa: 259_200,
        max_accrual_daa: 31_536_000,
    }
}

fn account(saved: i64) -> Entity {
    Entity::SavingsAccount {
        owner: "alice".into(),
        saved,
        rate_ppm: 20_000,
        delay_remaining_daa: 259_200,
        remaining_accrual_daa: 31_536_000,
        referral_fee_ppm: 0,
    }
}

#[test]
fn segregated_savings_is_counted_once_in_kusd_supply() {
    let mut indexer = Indexer::default();
    indexer
        .apply(
            "genesis",
            &[],
            vec![(op("genesis", 0), controller(0, false))],
        )
        .unwrap();
    indexer
        .apply(
            "activate",
            &[op("genesis", 0)],
            vec![(op("activate", 0), controller(0, true))],
        )
        .unwrap();
    indexer
        .apply(
            "open",
            &[op("activate", 0)],
            vec![
                (op("open", 0), controller(100_000_000_000, true)),
                (op("open", 1), account(100_000_000_000)),
                (
                    op("open", 2),
                    Entity::Token {
                        amount: 100_000_000_000,
                        is_minter: false,
                    },
                ),
            ],
        )
        .unwrap();
    let snapshot = indexer.snapshot();
    assert_eq!(snapshot.savings_balance, 100_000_000_000);
    assert_eq!(snapshot.savings_accounts, 1);
    assert_eq!(snapshot.circulating_supply, 100_000_000_000);
    assert!(snapshot.savings_enabled);
}

#[test]
fn divergent_controller_total_is_rejected_atomically() {
    let mut indexer = Indexer::default();
    indexer
        .apply(
            "genesis",
            &[],
            vec![(op("genesis", 0), controller(0, true))],
        )
        .unwrap();
    let before = indexer.snapshot().clone();
    assert!(
        indexer
            .apply(
                "forged-open",
                &[op("genesis", 0)],
                vec![
                    (op("forged-open", 0), controller(101, true)),
                    (op("forged-open", 1), account(100)),
                ],
            )
            .is_err()
    );
    assert_eq!(indexer.snapshot(), &before);
}
