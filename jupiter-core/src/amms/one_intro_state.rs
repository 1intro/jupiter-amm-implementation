use anchor_lang::prelude::*;
use anchor_lang::{AnchorDeserialize, AnchorSerialize};
use solana_sdk::pubkey::Pubkey;

pub const MAX_TOKEN_COUNT: usize = 4;

#[derive(AnchorSerialize, AnchorDeserialize, PartialEq, Debug, Clone, Copy)]
#[repr(C)]
pub struct TokenRecord {
    pub mint_key: Pubkey,
    pub account_key: Pubkey,
    pub balance: u64,
    pub weight: u64,
}

#[derive(AnchorSerialize, AnchorDeserialize, PartialEq, Debug, Clone, Copy)]
pub struct PoolState {
    // one unique authority PDA
    pub pool_auth_pda_key: Pubkey,
    pub pool_auth_pda_bump: u8,

    // pool lp mint and virtual supply
    pub pool_lp_mint_key: Pubkey,
    pub pool_lp_virtual_supply: u64,

    // pool token list
    pub pool_token_count: u64,
    pub pool_token_array: [TokenRecord; MAX_TOKEN_COUNT],
    pub pool_token_total_weight: u64,

    // swap fee
    pub pool_swap_fee_ratio: u64,
}
