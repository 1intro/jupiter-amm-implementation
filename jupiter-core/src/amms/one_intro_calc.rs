use anchor_lang::prelude::*;
use safemath::*;

pub const PONE: u64 = 1_000_000_000;

pub const MAX_IN_RATIO: u64 = PONE / 2; // 50%
pub const MAX_OUT_RATIO: u64 = PONE / 2; // 50%

#[error_code]
pub enum ErrorCode {
    #[msg("Calculation: general failure")]
    CalculationFailure,

    #[msg("Transaction Failed: Output token exceeds 50% of the token in pool liquidity. Reduce and retry.")]
    ValidationLiquidityTooBigTokenOutAmount,
}

pub fn proportional(amount: u64, numerator: u64, denominator: u64) -> anchor_lang::Result<u64> {
    if denominator == 0 {
        return Ok(amount);
    }

    let value = (amount as u128)
        .checked_mul(numerator as u128)
        .ok_or::<anchor_lang::error::Error>(ErrorCode::CalculationFailure.into())?
        .checked_div(denominator as u128)
        .ok_or::<anchor_lang::error::Error>(ErrorCode::CalculationFailure.into())?;

    u64::try_from(value).map_err(|_| ErrorCode::CalculationFailure.into())
}

pub fn value_from_shares(shares: u64, total_value: u64, total_shares: u64) -> anchor_lang::Result<u64> {
    proportional(shares, total_value, total_shares)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RoundDirection {
    Floor,
    Ceiling,
}

/**********************************************************************************************
// simple safe f64 math calculations                                                         //
**********************************************************************************************/

pub mod safemath {
    use super::RoundDirection;

    pub fn u64_to_f64_unchecked(value: u64) -> f64 {
        value as f64
    }

    pub fn f64_to_u64_rounded(value: f64, rounding: RoundDirection) -> u64 {
        match rounding {
            RoundDirection::Floor => value.floor() as u64,
            RoundDirection::Ceiling => value.ceil() as u64,
        }
    }

    pub fn add(left: f64, right: f64) -> f64 {
        left + right
    }

    pub fn sub(left: f64, right: f64) -> f64 {
        left - right
    }

    pub fn mul(left: f64, right: f64) -> f64 {
        left * right
    }

    pub fn div(left: f64, right: f64) -> f64 {
        left / right
    }

    pub fn pow(base: f64, n: f64) -> f64 {
        base.powf(n)
    }
}

/**********************************************************************************************
// calcOutGivenIn                                                                            //
// aO = tokenAmountOut                                                                       //
// bO = tokenBalanceOut                                                                      //
// bI = tokenBalanceIn              /      /            bI             \    (wI / wO) \      //
// aI = tokenAmountIn    aO = bO * |  1 - | --------------------------  | ^            |     //
// wI = tokenWeightIn               \      \ ( bI + ( aI * ( 1 - sF )) /              /      //
// wO = tokenWeightOut                                                                       //
// sF = swapFee                                                                              //
**********************************************************************************************/
pub fn calc_out_given_in(
    token_in_balance: u64,
    token_in_weight: u64,
    token_out_balance: u64,
    token_out_weight: u64,
    token_in_amount: u64,
    swap_fee: u64,
) -> anchor_lang::Result<u64> {
    let token_in_balance_f64 = u64_to_f64_unchecked(token_in_balance);
    let total_in_weight_f64 = u64_to_f64_unchecked(token_in_weight);
    let token_out_balance_f64 = u64_to_f64_unchecked(token_out_balance);
    let token_out_weight_f64 = u64_to_f64_unchecked(token_out_weight);
    let token_in_amount_f64 = u64_to_f64_unchecked(token_in_amount);
    let swap_fee_f64 = u64_to_f64_unchecked(swap_fee);

    let weight_ratio = div(total_in_weight_f64, token_out_weight_f64);
    let adjusted_in = mul(
        token_in_amount_f64,
        sub(1.0 as f64, div(swap_fee_f64, PONE as f64)),
    );
    let y = div(token_in_balance_f64, add(token_in_balance_f64, adjusted_in));
    let foo = pow(y, weight_ratio);
    let bar = sub(1.0 as f64, foo);

    Ok(f64_to_u64_rounded(
        mul(token_out_balance_f64, bar),
        RoundDirection::Floor,
    ))
}

/**********************************************************************************************
// calcInGivenOut                                                                            //
// aI = tokenAmountIn                                                                        //
// bO = tokenBalanceOut               /  /     bO      \    (wO / wI)      \                 //
// bI = tokenBalanceIn          bI * |  | ------------  | ^            - 1  |                //
// aO = tokenAmountOut    aI =        \  \ ( bO - aO ) /                   /                 //
// wI = tokenWeightIn           --------------------------------------------                 //
// wO = tokenWeightOut                          ( 1 - sF )                                   //
// sF = swapFee                                                                              //
**********************************************************************************************/
pub fn calc_in_given_out(
    token_in_balance: u64,
    token_in_weight: u64,
    token_out_balance: u64,
    token_out_weight: u64,
    token_out_amount: u64,
    swap_fee: u64,
) -> anchor_lang::Result<u64> {
    let token_in_balance_f64 = u64_to_f64_unchecked(token_in_balance);
    let total_in_weight_f64 = u64_to_f64_unchecked(token_in_weight);
    let token_out_balance_f64 = u64_to_f64_unchecked(token_out_balance);
    let token_out_weight_f64 = u64_to_f64_unchecked(token_out_weight);
    let token_out_amount_f64 = u64_to_f64_unchecked(token_out_amount);
    let swap_fee_f64 = u64_to_f64_unchecked(swap_fee);

    let weight_ratio = div(token_out_weight_f64, total_in_weight_f64);
    let diff = sub(token_out_balance_f64, token_out_amount_f64);
    let y = div(token_out_balance_f64, diff);
    let foo = sub(pow(y, weight_ratio), 1.0 as f64);

    Ok(f64_to_u64_rounded(
        div(
            mul(token_in_balance_f64, foo),
            sub(1.0 as f64, div(swap_fee_f64, PONE as f64)),
        ),
        RoundDirection::Ceiling,
    ))
}
