use kaspa_kusd::savings::{
    MAX_AMOUNT, MAX_REFERRAL_PPM, PPM, calculate_yield, mul_div_floor, validate_claim_window,
};

#[test]
fn one_year_at_two_percent_is_exact() {
    let y = calculate_yield(100_000_000_000, 20_000, 31_536_000, 31_536_000, 0).unwrap();
    assert_eq!(y.gross_interest, 2_000_000_000);
    assert_eq!(y.net_interest, 2_000_000_000);
}

#[test]
fn referral_is_deducted_only_from_interest() {
    let y = calculate_yield(
        100_000_000_000,
        20_000,
        31_536_000,
        31_536_000,
        MAX_REFERRAL_PPM,
    )
    .unwrap();
    assert_eq!(y.referral_fee, 500_000_000);
    assert_eq!(y.net_interest, 1_500_000_000);
}

#[test]
fn delay_and_remaining_term_bound_the_claim() {
    assert_eq!(
        validate_claim_window(259_200, 31_536_000, 86_400).unwrap(),
        345_600
    );
    assert!(validate_claim_window(259_200, 100, 101).is_err());
    assert!(validate_claim_window(0, 100, 0).is_err());
}

#[test]
fn numeric_domains_reject_invalid_rates_and_amounts() {
    assert!(calculate_yield(1, PPM + 1, 1, 1, 0).is_err());
    assert!(calculate_yield(1, 1, 1, 1, MAX_REFERRAL_PPM + 1).is_err());
    assert!(calculate_yield(MAX_AMOUNT + 1, 1, 1, 1, 0).is_err());
    assert!(mul_div_floor(-1, 1, 1).is_err());
}

#[test]
fn mul_div_matches_wide_reference_at_boundaries() {
    let cases = [
        (0, MAX_AMOUNT, 1),
        (MAX_AMOUNT, MAX_AMOUNT, MAX_AMOUNT),
        (MAX_AMOUNT, 1_000_000, MAX_AMOUNT),
        (MAX_AMOUNT, 250_000, 1_000_000),
    ];
    for (x, y, d) in cases {
        let expected = ((x as i128) * (y as i128) / d as i128) as i64;
        assert_eq!(mul_div_floor(x, y, d).unwrap(), expected);
    }
}
