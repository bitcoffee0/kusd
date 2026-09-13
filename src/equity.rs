//! Deterministic economic model for the KPS Reserve.
//! Amounts are KUSD atoms; floating-point arithmetic is never used.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EquityState {
    pub reserve_kusd: i64,
    pub total_kps: i64,
    pub collateral_sompi: u64,
}

impl EquityState {
    pub fn deposit(self, amount: i64) -> Result<(Self, i64), &'static str> {
        if amount <= 0 || self.reserve_kusd < 0 || self.total_kps < 0 || self.collateral_sompi != 0
        {
            return Err("invalid deposit or state");
        }
        let minted = if self.total_kps == 0 {
            if self.reserve_kusd != 0 {
                return Err("reserve without shares is forbidden");
            }
            amount
        } else {
            if self.reserve_kusd <= 0 {
                return Err("equity insolvable");
            }
            mul_div_floor(amount, self.total_kps, self.reserve_kusd)?
        };
        if minted <= 0 {
            return Err("deposit is too small to mint a share");
        }
        Ok((
            Self {
                reserve_kusd: self
                    .reserve_kusd
                    .checked_add(amount)
                    .ok_or("reserve overflow")?,
                total_kps: self
                    .total_kps
                    .checked_add(minted)
                    .ok_or("overflow shares")?,
                ..self
            },
            minted,
        ))
    }

    pub fn redeem(self, shares: i64) -> Result<(Self, i64, u64), &'static str> {
        if shares <= 0 || shares > self.total_kps {
            return Err("invalid redemption");
        }
        let payout = if self.reserve_kusd == 0 {
            0
        } else {
            mul_div_floor(shares, self.reserve_kusd, self.total_kps)?
        };
        let payout_kas =
            ((shares as i128) * (self.collateral_sompi as i128) / self.total_kps as i128) as u64;
        if payout <= 0 && payout_kas == 0 {
            return Err("redemption rounds down to zero");
        }
        Ok((
            Self {
                reserve_kusd: self.reserve_kusd - payout,
                total_kps: self.total_kps - shares,
                collateral_sompi: self.collateral_sompi - payout_kas,
                ..self
            },
            payout,
            payout_kas,
        ))
    }

    pub fn backstop(
        self,
        debt: i64,
        reward_ppm: i64,
        collateral_sompi: u64,
    ) -> Result<(Self, i64), &'static str> {
        self.backstop_with_assigned(debt, 0, reward_ppm, collateral_sompi)
    }

    pub fn backstop_with_assigned(
        self,
        debt: i64,
        assigned_reserve: i64,
        reward_ppm: i64,
        collateral_sompi: u64,
    ) -> Result<(Self, i64), &'static str> {
        if debt <= 0 || !(1..=100_000).contains(&reward_ppm) {
            return Err("invalid Auction parameters");
        }
        if assigned_reserve < 0 || assigned_reserve > debt {
            return Err("invalid assigned reserve");
        }
        let reward = mul_div_floor(debt, reward_ppm, 1_000_000)?;
        let payment = debt
            .checked_sub(assigned_reserve)
            .and_then(|value| value.checked_add(reward))
            .ok_or("overflow paiement")?;
        if self.reserve_kusd < payment {
            return Err("insufficient Reserve: Auction remains open");
        }
        Ok((
            Self {
                reserve_kusd: self.reserve_kusd - payment,
                collateral_sompi: self
                    .collateral_sompi
                    .checked_add(collateral_sompi)
                    .ok_or("collateral overflow")?,
                ..self
            },
            reward,
        ))
    }
}

pub fn mul_div_floor(a: i64, b: i64, denominator: i64) -> Result<i64, &'static str> {
    if a < 0 || b < 0 || denominator <= 0 {
        return Err("invalid mulDiv");
    }
    let result = (a as i128)
        .checked_mul(b as i128)
        .ok_or("overflow mulDiv")?
        / denominator as i128;
    i64::try_from(result).map_err(|_| "result exceeds i64 range")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deposits_and_redeems_pro_rata_without_overminting() {
        let (state, first) = EquityState::default().deposit(100).unwrap();
        assert_eq!(first, 100);
        let (state, second) = state.deposit(33).unwrap();
        assert_eq!(second, 33);
        let (state, payout, payout_kas) = state.redeem(33).unwrap();
        assert_eq!(payout, 33);
        assert_eq!(payout_kas, 0);
        assert_eq!(
            state,
            EquityState {
                reserve_kusd: 100,
                total_kps: 100,
                collateral_sompi: 0
            }
        );
    }

    #[test]
    fn backstop_burns_debt_and_reward_and_socializes_loss_to_kps() {
        let (state, _) = EquityState::default().deposit(2_000_000_000).unwrap();
        let (after, reward) = state
            .backstop(1_500_000_000, 10_000, 10_000_000_000)
            .unwrap();
        assert_eq!(reward, 15_000_000);
        assert_eq!(after.reserve_kusd, 485_000_000);
        assert_eq!(after.total_kps, 2_000_000_000);
        assert_eq!(after.collateral_sompi, 10_000_000_000);
    }

    #[test]
    fn insufficient_reserve_never_erases_uncovered_debt() {
        let (state, _) = EquityState::default().deposit(1_514_999_999).unwrap();
        assert_eq!(
            state.backstop(1_500_000_000, 10_000, 10_000_000_000),
            Err("insufficient Reserve: Auction remains open")
        );
    }

    #[test]
    fn deposit_rounding_never_dilutes_existing_holders_upward() {
        let state = EquityState {
            reserve_kusd: 3,
            total_kps: 2,
            collateral_sompi: 0,
        };
        let (next, minted) = state.deposit(2).unwrap();
        assert_eq!(minted, 1);
        assert!(minted * state.reserve_kusd <= 2 * state.total_kps);
        assert_eq!(next.reserve_kusd, 5);
    }

    #[test]
    fn exhaustive_small_integer_deposits_cannot_extract_rounding_profit() {
        for reserve in 1..=64 {
            for shares in 1..=64 {
                for deposit in 1..=64 {
                    let state = EquityState {
                        reserve_kusd: reserve,
                        total_kps: shares,
                        collateral_sompi: 0,
                    };
                    if let Ok((after_deposit, minted)) = state.deposit(deposit) {
                        assert!(minted * reserve <= deposit * shares);
                        if let Ok((_, payout, _)) = after_deposit.redeem(minted) {
                            assert!(payout <= deposit);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn backstop_accepts_the_exact_threshold_and_rejects_one_atom_less() {
        let debt = 1_500_000_000;
        let reward_ppm = 10_000;
        let payment = 1_515_000_000;
        let exact = EquityState {
            reserve_kusd: payment,
            total_kps: payment,
            collateral_sompi: 0,
        };
        assert_eq!(
            exact
                .backstop(debt, reward_ppm, 10_000_000_000)
                .unwrap()
                .0
                .reserve_kusd,
            0
        );
        assert!(
            EquityState {
                reserve_kusd: payment - 1,
                ..exact
            }
            .backstop(debt, reward_ppm, 10_000_000_000)
            .is_err()
        );
    }

    #[test]
    fn kps_can_redeem_collateral_after_an_exact_backstop() {
        let (funded, shares) = EquityState::default().deposit(1_515_000_000).unwrap();
        let (after_loss, _) = funded
            .backstop(1_500_000_000, 10_000, 10_000_000_000)
            .unwrap();
        assert_eq!(after_loss.reserve_kusd, 0);
        assert!(after_loss.deposit(1).is_err());
        let (empty, payout_kusd, payout_kas) = after_loss.redeem(shares).unwrap();
        assert_eq!(payout_kusd, 0);
        assert_eq!(payout_kas, 10_000_000_000);
        assert_eq!(empty, EquityState::default());
        assert!(empty.deposit(100).is_ok());
    }

    #[test]
    fn assigned_reserve_is_burned_before_equity_backstop() {
        let state = EquityState {
            reserve_kusd: 1_365_000_000,
            total_kps: 2_000_000_000,
            collateral_sompi: 0,
        };
        let (after, reward) = state
            .backstop_with_assigned(1_500_000_000, 150_000_000, 10_000, 10_000_000_000)
            .unwrap();
        assert_eq!(reward, 15_000_000);
        assert_eq!(after.reserve_kusd, 0);
        assert_eq!(after.collateral_sompi, 10_000_000_000);
        assert!(
            state
                .backstop_with_assigned(1_500_000_000, 150_000_001, 10_000, 0)
                .is_ok()
        );
        assert!(
            state
                .backstop_with_assigned(1_500_000_000, 1_500_000_001, 10_000, 0)
                .is_err()
        );
    }

    #[test]
    fn mul_div_handles_products_larger_than_i64_when_the_result_fits() {
        assert_eq!(
            mul_div_floor(1_515_000_000, 10_000_000_000, 1_515_000_000),
            Ok(10_000_000_000)
        );
        assert_eq!(
            mul_div_floor(i64::MAX - 1, i64::MAX - 2, i64::MAX),
            Ok(i64::MAX - 3)
        );
    }
}
