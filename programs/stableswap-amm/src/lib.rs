use anchor_lang::prelude::*;

pub mod constants;
pub mod errors;
pub mod instructions;
pub mod math;
pub mod state;

use constants::MAX_TOKENS;
use instructions::*;

declare_id!("66wyxbM93npnY75VwM6CCQ7Evf9wp7uhw31QXXZ6Pkex");

#[program]
pub mod stableswap_amm {
    use super::*;

    /// Initialize a new StableSwap pool with 2-6 tokens.
    /// 
    /// # Arguments
    /// * `amplification` - The A parameter (typical range: 100-2000 for stablecoins)
    /// * `fee_bps` - Swap fee in basis points (e.g., 4 = 0.04%)
    /// 
    /// # Remaining Accounts
    /// For each token (2-6), pass pairs of: [mint, vault]
    pub fn initialize_pool(
        ctx: Context<InitializePool>,
        amplification: u64,
        fee_bps: u16,
    ) -> Result<()> {
        instructions::initialize_pool::initialize_pool_handler(ctx, amplification, fee_bps)
    }

    /// Add liquidity to the pool (supports imbalanced deposits).
    /// 
    /// # Arguments
    /// * `lp_amount` - Desired LP tokens to mint
    /// * `max_amounts` - Maximum of each token willing to deposit (0 = skip)
    /// 
    /// # Remaining Accounts
    /// - First n: ALL vault accounts
    /// - Rest: User ATAs for non-zero max_amounts only
    pub fn add_liquidity<'info>(
        ctx: Context<'_, '_, 'info, 'info, ModifyLiquidity<'info>>,
        lp_amount: u64,
        max_amounts: [u64; MAX_TOKENS],
    ) -> Result<()> {
        instructions::modify_liquidity::add_liquidity_handler(ctx, lp_amount, max_amounts)
    }

    /// Remove liquidity proportionally from all tokens.
    /// 
    /// # Arguments
    /// * `lp_amount` - LP tokens to burn
    /// * `min_amounts` - Minimum of each token to receive (0 = skip)
    /// 
    /// # Remaining Accounts
    /// - First n: ALL vault accounts
    /// - Rest: User ATAs for withdrawal
    pub fn remove_liquidity<'info>(
        ctx: Context<'_, '_, 'info, 'info, ModifyLiquidity<'info>>,
        lp_amount: u64,
        min_amounts: [u64; MAX_TOKENS],
    ) -> Result<()> {
        instructions::modify_liquidity::remove_liquidity_handler(ctx, lp_amount, min_amounts)
    }

    /// Swap one token for another.
    /// 
    /// # Arguments
    /// * `amount_in` - Amount of input token
    /// * `min_amount_out` - Minimum output tokens to receive
    /// * `input_index` - Index of input token
    /// * `output_index` - Index of output token
    /// 
    /// # Remaining Accounts
    /// - First n: ALL vault accounts
    /// - Next: User's input ATA
    /// - Last: User's output ATA
    pub fn swap<'info>(
        ctx: Context<'_, '_, 'info, 'info, Swap<'info>>,
        amount_in: u64,
        min_amount_out: u64,
        input_index: u8,
        output_index: u8,
    ) -> Result<()> {
        instructions::swap::swap_handler(ctx, amount_in, min_amount_out, input_index, output_index)
    }
}
