//! Integer accounting for Positions with upfront fees.
//!
//! The debt is the gross mint. One fraction is assigned to the minter
//! reserve, another is irreversible Reserve/equity income, and only
//! the remainder is paid to the Position owner.

use crate::equity::mul_div_floor;

pub const PPM: i64 = 1_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MintTerms {
    pub reserve_contribution_ppm: i64,
    pub annual_interest_ppm: i64,
    /// Explicit calibration of DAA units per year.
    pub daa_per_year: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MintQuote {
    pub gross_debt: i64,
    pub user_kusd: i64,
    pub assigned_reserve_kusd: i64,
    pub equity_fee_kusd: i64,
    pub current_fee_ppm: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CloseQuote {
    pub user_burn_kusd: i64,
    pub assigned_reserve_burn_kusd: i64,
}

impl MintTerms {
    pub fn quote(
        self,
        gross_debt: i64,
        remaining_lifetime_daa: i64,
    ) -> Result<MintQuote, &'static str> {
        if gross_debt <= 0
            || remaining_lifetime_daa < 0
            || self.daa_per_year <= 0
            || !(0..PPM).contains(&self.reserve_contribution_ppm)
            || self.annual_interest_ppm < 0
        {
            return Err("invalid mint parameters");
        }
        let current_fee_ppm = mul_div_floor(
            self.annual_interest_ppm,
            remaining_lifetime_daa,
            self.daa_per_year,
        )?;
        let total_rate = self
            .reserve_contribution_ppm
            .checked_add(current_fee_ppm)
            .ok_or("overflow taux")?;
        if total_rate >= PPM {
            return Err("fees and reserve absorb the entire mint");
        }

        let assigned_reserve_kusd = mul_div_floor(gross_debt, self.reserve_contribution_ppm, PPM)?;
        let equity_fee_kusd = mul_div_floor(gross_debt, current_fee_ppm, PPM)?;
        let user_kusd = gross_debt
            .checked_sub(assigned_reserve_kusd)
            .and_then(|value| value.checked_sub(equity_fee_kusd))
            .ok_or("invalid mint distribution")?;
        if user_kusd <= 0 {
            return Err("user mint rounds down to zero");
        }

        Ok(MintQuote {
            gross_debt,
            user_kusd,
            assigned_reserve_kusd,
            equity_fee_kusd,
            current_fee_ppm,
        })
    }
}

impl MintQuote {
    pub fn reserve_credit(self) -> Result<i64, &'static str> {
        self.assigned_reserve_kusd
            .checked_add(self.equity_fee_kusd)
            .ok_or("reserve credit overflow")
    }

    /// Full close: the assigned contribution is burned from the
    /// Reserve while the owner supplies the remainder. The fee stays
    /// in equity and is never refunded.
    pub fn close_quote(self) -> Result<CloseQuote, &'static str> {
        Ok(CloseQuote {
            user_burn_kusd: self
                .gross_debt
                .checked_sub(self.assigned_reserve_kusd)
                .ok_or("assigned reserve exceeds debt")?,
            assigned_reserve_burn_kusd: self.assigned_reserve_kusd,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const YEAR: i64 = 31_536_000;

    #[test]
    fn frankencoin_style_example_splits_user_reserve_and_fee() {
        let quote = MintTerms {
            reserve_contribution_ppm: 200_000,
            annual_interest_ppm: 50_000,
            daa_per_year: YEAR,
        }
        .quote(500_0000_0000, YEAR)
        .unwrap();
        assert_eq!(quote.user_kusd, 375_0000_0000);
        assert_eq!(quote.assigned_reserve_kusd, 100_0000_0000);
        assert_eq!(quote.equity_fee_kusd, 25_0000_0000);
        assert_eq!(quote.reserve_credit().unwrap(), 125_0000_0000);
    }

    #[test]
    fn interest_declines_linearly_with_remaining_lifetime() {
        let terms = MintTerms {
            reserve_contribution_ppm: 100_000,
            annual_interest_ppm: 60_000,
            daa_per_year: YEAR,
        };
        assert_eq!(
            terms.quote(1_000_000_000, YEAR).unwrap().current_fee_ppm,
            60_000
        );
        assert_eq!(
            terms
                .quote(1_000_000_000, YEAR / 2)
                .unwrap()
                .current_fee_ppm,
            30_000
        );
        assert_eq!(terms.quote(1_000_000_000, 0).unwrap().current_fee_ppm, 0);
    }

    #[test]
    fn close_uses_assigned_reserve_but_never_refunds_the_fee() {
        let quote = MintTerms {
            reserve_contribution_ppm: 100_000,
            annual_interest_ppm: 20_000,
            daa_per_year: YEAR,
        }
        .quote(300_000_000_000, YEAR)
        .unwrap();
        assert_eq!(quote.user_kusd, 264_000_000_000);
        assert_eq!(quote.reserve_credit().unwrap(), 36_000_000_000);
        assert_eq!(
            quote.close_quote().unwrap(),
            CloseQuote {
                user_burn_kusd: 270_000_000_000,
                assigned_reserve_burn_kusd: 30_000_000_000,
            }
        );
    }

    #[test]
    fn rates_cannot_consume_the_whole_mint() {
        assert!(
            MintTerms {
                reserve_contribution_ppm: 900_000,
                annual_interest_ppm: 100_000,
                daa_per_year: YEAR,
            }
            .quote(100, YEAR)
            .is_err()
        );
    }
}
