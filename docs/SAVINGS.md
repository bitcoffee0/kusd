# Savings

Savings is disabled at genesis and enabled only by its dedicated Governor.
Each series fixes its interest rate, delay, maximum accrual window, referral
share, and account template.

Users transfer KUSD into independent SavingsAccount UTXOs. Interest is lazy:
no balance changes merely because DAA advances. During `refresh`, the account
claims an elapsed term, consensus timelocks bound that claim, and EquityReserve
transfers the exact gross interest. The account receives net interest and the
referrer receives its configured share. `withdraw` returns the complete account
balance and closes the account without touching Reserve again.

```text
grossInterest = floor(principal × ratePpm × elapsedDaa
                      / 1,000,000 / daaPerYear)
referral      = floor(grossInterest × referralPpm / 1,000,000)
netInterest   = grossInterest - referral
```

Interest never creates KUSD. If Reserve cannot fund the exact payment, refresh
fails. Savings can therefore be deployed with the protocol but left disabled
until governance considers Reserve liquidity sufficient.
