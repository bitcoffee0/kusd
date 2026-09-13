use kaspa_kusd::protocol::{required_weighted_kps_votes, weighted_kps_votes};

#[test]
fn weighted_threshold_rounds_up_and_matches_wide_reference() {
    for total in [1, 2, 999_999, 1_000_000, 2_000_000_000, (1_i64 << 55) - 1] {
        for max_weight in [1, 2, 4, 8, 16] {
            for ppm in [1, 19_999, 20_000, 999_999, 1_000_000] {
                let product = i128::from(total) * i128::from(max_weight) * i128::from(ppm);
                let expected = ((product + 999_999) / 1_000_000) as i64;
                assert_eq!(
                    required_weighted_kps_votes(total, max_weight, ppm).unwrap(),
                    expected
                );
            }
        }
    }
}

#[test]
fn weighted_vote_boundaries_reject_invalid_values_and_overflow() {
    assert_eq!(weighted_kps_votes(40_000_000, 4, 4).unwrap(), 160_000_000);
    assert!(weighted_kps_votes(1, 0, 4).is_err());
    assert!(weighted_kps_votes(1, 5, 4).is_err());
    assert!(weighted_kps_votes(i64::MAX, 2, 4).is_err());
    assert!(required_weighted_kps_votes(i64::MAX, 16, 20_000).is_err());
    assert_eq!(
        required_weighted_kps_votes(2_000_000_000, 4, 20_000).unwrap(),
        160_000_000
    );
}
