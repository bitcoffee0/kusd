pub const PPM: i64 = 1_000_000;
pub const MAX_REFERRAL_PPM: i64 = 250_000;
pub const MAX_AMOUNT: i64 = 36_028_797_018_963_967;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SavingsYield {
    pub annual_interest: i64,
    pub gross_interest: i64,
    pub referral_fee: i64,
    pub net_interest: i64,
}

pub fn mul_div_floor(x: i64, y: i64, denominator: i64) -> Result<i64, String> {
    if x < 0 || y < 0 || denominator <= 0 {
        return Err("mulDivFloor requires positive operands".into());
    }
    if x > MAX_AMOUNT || y > MAX_AMOUNT || denominator > MAX_AMOUNT {
        return Err("Savings operand outside the consensus domain".into());
    }
    let value = (x as i128)
        .checked_mul(y as i128)
        .ok_or_else(|| "overflow mulDivFloor".to_string())?
        / denominator as i128;
    i64::try_from(value).map_err(|_| "mulDivFloor result exceeds i64 range".into())
}

pub fn calculate_yield(
    saved: i64,
    rate_ppm: i64,
    accrual_daa: i64,
    daa_per_year: i64,
    referral_fee_ppm: i64,
) -> Result<SavingsYield, String> {
    if saved <= 0 || saved > MAX_AMOUNT {
        return Err("invalid Savings balance".into());
    }
    if !(0..=PPM).contains(&rate_ppm) {
        return Err("invalid Savings rate".into());
    }
    if accrual_daa < 0 || daa_per_year <= 0 || accrual_daa > MAX_AMOUNT {
        return Err("invalid DAA duration".into());
    }
    if !(0..=MAX_REFERRAL_PPM).contains(&referral_fee_ppm) {
        return Err("invalid Savings referral".into());
    }
    let annual_interest = mul_div_floor(saved, rate_ppm, PPM)?;
    let gross_interest = mul_div_floor(annual_interest, accrual_daa, daa_per_year)?;
    let referral_fee = mul_div_floor(gross_interest, referral_fee_ppm, PPM)?;
    Ok(SavingsYield {
        annual_interest,
        gross_interest,
        referral_fee,
        net_interest: gross_interest - referral_fee,
    })
}

pub fn validate_claim_window(
    delay_remaining_daa: i64,
    remaining_accrual_daa: i64,
    claimed_accrual_daa: i64,
) -> Result<i64, String> {
    if delay_remaining_daa < 0
        || remaining_accrual_daa < 0
        || claimed_accrual_daa <= 0
        || claimed_accrual_daa > remaining_accrual_daa
    {
        return Err("invalid Savings accrual window".into());
    }
    delay_remaining_daa
        .checked_add(claimed_accrual_daa)
        .ok_or_else(|| "Savings DAA threshold overflow".into())
}
