use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{Mint, Token},
};
use crate::constants::{MAX_TOKENS, MIN_TOKENS, MAX_AMP, MAX_FEE_BPS};
use crate::errors::StableSwapError;
use crate::state::Pool;

#[derive(Accounts)]
pub struct InitializePool<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        init,
        payer = admin,
        space = Pool::LEN,
        seeds = [b"pool", lp_mint.key().as_ref()],
        bump,
    )]
    pub pool: Account<'info, Pool>,

    #[account(
        init,
        payer = admin,
        mint::decimals = 6,
        mint::authority = pool,
    )]
    pub lp_mint: Account<'info, Mint>,

    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub rent: Sysvar<'info, Rent>,
}

impl<'info> InitializePool<'info> {
    pub fn validate(
        &self,
        amplification: u64,
        fee_bps: u16,
        n_tokens: usize,
    ) -> Result<()> {
        require!(
            amplification > 0 && amplification <= MAX_AMP,
            StableSwapError::InvalidAmplification
        );
        require!(fee_bps <= MAX_FEE_BPS, StableSwapError::InvalidFee);
        require!(
            n_tokens >= MIN_TOKENS && n_tokens <= MAX_TOKENS,
            StableSwapError::InvalidTokenCount
        );
        Ok(())
    }

    pub fn initialize_pool(
        &mut self,
        amplification: u64,
        fee_bps: u16,
        token_mints: [Pubkey; MAX_TOKENS],
        n_tokens: u8,
        bump: u8,
    ) {
        self.pool.set_inner(Pool {
            admin: self.admin.key(),
            lp_mint: self.lp_mint.key(),
            amplification,
            fee_bps,
            n_tokens,
            token_mints,
            bump,
        });
    }
}

/// Initialize a new StableSwap pool.
/// 
/// # Arguments
/// * `amplification` - The A parameter (100-2000 typical for stablecoins)
/// * `fee_bps` - Swap fee in basis points (e.g., 4 = 0.04%)
/// 
/// # Remaining Accounts
/// For each token (2-6 tokens), pass in order:
/// - mint: The token mint
/// - vault: The pool's ATA for this token (will be initialized)
pub fn initialize_pool_handler(
    ctx: Context<InitializePool>,
    amplification: u64,
    fee_bps: u16,
) -> Result<()> {
    let remaining = &ctx.remaining_accounts;
    
    // Each token needs: mint, vault (2 accounts per token)
    require!(
        remaining.len() >= MIN_TOKENS * 2 && remaining.len() <= MAX_TOKENS * 2,
        StableSwapError::InvalidRemainingAccounts
    );
    require!(
        remaining.len() % 2 == 0,
        StableSwapError::InvalidRemainingAccounts
    );

    let n_tokens = remaining.len() / 2;
    
    ctx.accounts.validate(amplification, fee_bps, n_tokens)?;

    // Extract and validate mints, check for duplicates
    let mut token_mints = [Pubkey::default(); MAX_TOKENS];
    
    for i in 0..n_tokens {
        let mint_info = &remaining[i * 2];
        let vault_info = &remaining[i * 2 + 1];

        // Validate mint is actually a mint
        let mint_data = mint_info.try_borrow_data()?;
        require!(mint_data.len() == Mint::LEN, StableSwapError::InvalidMint);

        // Check for duplicate mints
        for j in 0..i {
            require!(
                token_mints[j] != mint_info.key(),
                StableSwapError::DuplicateMint
            );
        }

        // Validate vault is the expected ATA
        let expected_vault = anchor_spl::associated_token::get_associated_token_address(
            &ctx.accounts.pool.key(),
            &mint_info.key(),
        );
        require!(
            vault_info.key() == expected_vault,
            StableSwapError::InvalidVault
        );

        token_mints[i] = mint_info.key();
    }

    ctx.accounts.initialize_pool(
        amplification,
        fee_bps,
        token_mints,
        n_tokens as u8,
        ctx.bumps.pool,
    );

    msg!("Pool initialized with {} tokens, A={}, fee={}bps", n_tokens, amplification, fee_bps);

    Ok(())
}
