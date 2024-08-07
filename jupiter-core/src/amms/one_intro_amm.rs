use anchor_lang::AnchorDeserialize;
use anyhow::{Context, Result};
use itertools::Itertools;
use jupiter_amm_interface::{AccountMap, Amm, KeyedAccount, Quote, QuoteParams, Swap, SwapAndAccountMetas, SwapMode, SwapParams};
use rust_decimal::Decimal;
use solana_sdk::{instruction::AccountMeta, pubkey, pubkey::Pubkey};
use spl_associated_token_account::get_associated_token_address;

use super::{one_intro_calc::{calc_in_given_out, calc_out_given_in, value_from_shares, ErrorCode, MAX_IN_RATIO, MAX_OUT_RATIO, PONE}, one_intro_state::PoolState};

pub const ONE_INTRO_PROGRAM_ID: Pubkey = pubkey!("DEXYosS6oEGvk8uCDayvwEZz4qEyDJRf9nFgYCaqPMTm");

const ONE_INTRO_METADATA_STATE: Pubkey = pubkey!("5nmAbnjJfW1skrPvYjLTBNdhoKzJfznnbvDcM8G2U7Ki");
const ONE_INTRO_TOKEN_AUTH_PDA: Pubkey = pubkey!("ATowQwFzdJBJ9VFSfoNKmuB8GiSeo8foM5vRriwmKmFB");

pub struct OneIntroAmm {
    key: Pubkey,
    program_id: Pubkey,
    state: PoolState,
}

impl Clone for OneIntroAmm {
    fn clone(&self) -> Self {
        OneIntroAmm {
            key: self.key,
            program_id: self.program_id,
            state: self.state,
        }
    }
}

impl Amm for OneIntroAmm {
    fn key(&self) -> Pubkey {
        self.key
    }

    fn label(&self) -> String {
        String::from("1DEX")
    }

    fn program_id(&self) -> Pubkey {
        self.program_id
    }

    fn clone_amm(&self) -> Box<dyn Amm + Send + Sync> {
        Box::new(self.clone())
    }

    fn from_keyed_account(keyed_account: &KeyedAccount) -> Result<Self> {
        Ok(OneIntroAmm {
            key: keyed_account.key,
            program_id: keyed_account.account.owner,
            state: PoolState::deserialize(&mut &keyed_account.account.data[8..])?, // Skip the first 8-byte Anchor discriminator.
        })
    }

    fn get_reserve_mints(&self) -> Vec<Pubkey> {
        self.state.pool_token_array
            .map(|v| v.mint_key)
            .into_iter()
            .filter(|v| !v.eq(&pubkey!("11111111111111111111111111111111")))
            .collect_vec()
    }

    fn get_accounts_to_update(&self) -> Vec<Pubkey> {
        vec![self.key]
    }

    fn update(&mut self, _account_map: &AccountMap) -> Result<()> {
        let account = _account_map.get(&self.key).context("Pool state not found.")?;

        self.state = PoolState::deserialize(&mut &account.data[8..])?;

        Ok(())
    }

    fn quote(&self, quote_params: &QuoteParams) -> Result<Quote> {
        if quote_params.amount <= 0 {
            Err(ErrorCode::ValidationTooSmallTokenInAmount.into())
        }

        let record_0 = &self.state.pool_token_array[0];
        let record_1 = &self.state.pool_token_array[1];
        let swap_fee_ratio = self.state.pool_swap_fee_ratio;

        let (token_in_balance, token_out_balance, token_in_weight, token_out_weight) =
            if quote_params.input_mint == self.state.pool_token_array[0].mint_key {
                (record_0.balance, record_1.balance, record_0.weight, record_1.weight)
            } else {
                (record_1.balance, record_0.balance, record_1.weight, record_0.weight)
            };

        let (in_amount, out_amount, fee_amount, not_enough_liquidity) = match quote_params.swap_mode {
            SwapMode::ExactIn => {
                swap_exact_amount_in(
                    token_in_balance,
                    token_in_weight,
                    token_out_balance,
                    token_out_weight,
                    quote_params.amount,
                    swap_fee_ratio
                )?
            },
            SwapMode::ExactOut => {
                swap_exact_amount_out(
                    token_in_balance,
                    token_in_weight,
                    token_out_balance,
                    token_out_weight,
                    quote_params.amount,
                    swap_fee_ratio
                )?
            },
        };

        if out_amount <= 0 {
            Err(ErrorCode::ValidationTooSmallTokenOutAmount.into())
        }

        Ok(Quote {
            in_amount,
            out_amount,
            fee_amount,
            fee_mint: quote_params.input_mint,
            fee_pct: Decimal::new((swap_fee_ratio * 100) as i64, 9),
            not_enough_liquidity,
            ..Quote::default()
        })
    }

    fn get_swap_and_account_metas(&self, swap_params: &SwapParams) -> Result<SwapAndAccountMetas> {
        let record_0 = self.state.pool_token_array[0];
        let record_1 = self.state.pool_token_array[1];

        let (pool_token_in_account, pool_token_out_account) =
            if swap_params.source_mint == record_0.mint_key {
                (record_0.account_key, record_1.account_key)
            } else {
                (record_1.account_key, record_0.account_key)
            };

        let user = swap_params.token_transfer_authority;
        let ata_metadata_swap_fee = get_associated_token_address(&ONE_INTRO_TOKEN_AUTH_PDA, &swap_params.source_mint);

        Ok(SwapAndAccountMetas {
            swap: Swap::TokenSwap, // TODO How to add 1INTRO to Swap enum?
            account_metas: Vec::from([
                AccountMeta::new_readonly(ONE_INTRO_METADATA_STATE, false), // metadataState
                AccountMeta::new(self.key, false), // poolState
                AccountMeta::new_readonly(self.state.pool_auth_pda_key, false), // poolAuthPda
                AccountMeta::new(pool_token_in_account, false), // poolTokenInAccount
                AccountMeta::new(pool_token_out_account, false), // poolTokenOutAccount
                AccountMeta::new(user, true), // user
                AccountMeta::new(swap_params.source_token_account, false), // userTokenInAccount
                AccountMeta::new(swap_params.destination_token_account, false), // userTokenOutAccount
                AccountMeta::new(ata_metadata_swap_fee, false), // metadataSwapFeeAccount
                AccountMeta::new(self.key, false), // referrerTokenAccount
                AccountMeta::new_readonly(spl_token::id(), false), // tokenProgram
            ]),
        })
    }
}

fn swap_exact_amount_in(
    token_in_balance: u64,
    token_in_weight: u64,
    token_out_balance: u64,
    token_out_weight: u64,
    token_in_amount: u64,
    swap_fee_ratio: u64,
) -> Result<(u64, u64, u64, bool)> {
    let max_token_in_amount = value_from_shares(MAX_IN_RATIO, token_in_balance, PONE)?;

    let swap_fee_amount = value_from_shares(swap_fee_ratio, token_in_amount, PONE)?;
    let adjusted_token_in_amount = token_in_amount.checked_sub(swap_fee_amount).context("token_in_amount underflow")?;

    let token_out_amount = calc_out_given_in(
        token_in_balance,
        token_in_weight,
        token_out_balance,
        token_out_weight,
        adjusted_token_in_amount,
        0,
    )?;

    Ok((
        token_in_amount,
        token_out_amount,
        swap_fee_amount,
        token_in_amount > max_token_in_amount,
    ))
}

fn swap_exact_amount_out(
    token_in_balance: u64,
    token_in_weight: u64,
    token_out_balance: u64,
    token_out_weight: u64,
    token_out_amount: u64,
    swap_fee_ratio: u64,
) -> Result<(u64, u64, u64, bool)> {
    let max_token_out_amount = value_from_shares(MAX_OUT_RATIO, token_out_balance, PONE)?;

    let temp_token_in_amount = calc_in_given_out(
        token_in_balance,
        token_in_weight,
        token_out_balance,
        token_out_weight,
        token_out_amount,
        0,
    )?;

    let token_in_amount = value_from_shares(PONE, temp_token_in_amount, PONE.checked_sub(swap_fee_ratio).context("PONE underflow")?)?;
    let swap_fee_amount = token_in_amount.checked_sub(temp_token_in_amount).context("adjusted_token_in_amount underflow")?;

    Ok((
        token_in_amount,
        token_out_amount,
        swap_fee_amount,
        token_out_amount > max_token_out_amount,
    ))
}
