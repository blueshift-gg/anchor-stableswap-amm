use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, MintTo, Burn, Token, TokenAccount, Transfer};
use crate::constants::{MAX_TOKENS, MINIMUM_LIQUIDITY};
use crate::errors::StableSwapError;
use crate::math::{compute_d, calculate_mint_amount, calculate_withdraw_amounts};
use crate::state::Pool;

#[derive(Accounts)]
pub struct ModifyLiquidity<'info> {
    #[account(mut)]
    pub pool: Account<'info, Pool>,

    #[account(
        mut,
        constraint = lp_mint.key() == pool.lp_mint @ StableSwapError::InvalidMint
    )]
    pub lp_mint: Account<'info, Mint>,

    #[account(
        mut,
        constraint = user_lp_account.mint == pool.lp_mint @ StableSwapError::InvalidMint
    )]
    pub user_lp_account: Account<'info, TokenAccount>,

    #[account(mut)]
    pub user: Signer<'info>,

    pub token_program: Program<'info, Token>,
}

impl<'info> ModifyLiquidity<'info> {
    /// Validate that vault accounts match expected ATAs
    pub fn validate_vaults(&self, vaults: &[AccountInfo<'info>]) -> Result<()> {
        for (i, vault) in vaults.iter().enumerate() {
            let expected = anchor_spl::associated_token::get_associated_token_address(
                &self.pool.key(),
                &self.pool.token_mints[i],
            );
            require!(vault.key() == expected, StableSwapError::InvalidVault);
        }
        Ok(())
    }

    /// Read reserves from vault accounts
    pub fn read_reserves(&self, vaults: &[AccountInfo<'info>]) -> Result<Vec<u128>> {
        let mut reserves = Vec::with_capacity(vaults.len());
        for vault in vaults {
            let data = TokenAccount::try_deserialize(&mut &**vault.try_borrow_data()?)?;
            reserves.push(data.amount as u128);
        }
        Ok(reserves)
    }

    /// Transfer tokens from user ATAs to vaults (deposit)
    pub fn transfer_to_vaults(
        &self,
        vaults: &[AccountInfo<'info>],
        user_atas: &[AccountInfo<'info>],
        deposits: &[u128],
        max_amounts: &[u64],
    ) -> Result<()> {
        let mut user_ata_idx = 0;
        for (i, &deposit) in deposits.iter().enumerate() {
            if max_amounts[i] > 0 && deposit > 0 {
                token::transfer(
                    CpiContext::new(
                        self.token_program.to_account_info(),
                        Transfer {
                            from: user_atas[user_ata_idx].to_account_info(),
                            to: vaults[i].to_account_info(),
                            authority: self.user.to_account_info(),
                        },
                    ),
                    deposit as u64,
                )?;
                user_ata_idx += 1;
            }
        }
        Ok(())
    }

    /// Transfer tokens from vaults to user ATAs (withdraw)
    pub fn transfer_from_vaults(
        &self,
        vaults: &[AccountInfo<'info>],
        user_atas: &[AccountInfo<'info>],
        amounts: &[u128],
        min_amounts: &[u64],
        is_proportional: bool,
        signer_seeds: &[&[&[u8]]],
    ) -> Result<()> {
        let mut user_ata_idx = 0;
        for (i, &amount) in amounts.iter().enumerate() {
            let should_withdraw = is_proportional || min_amounts[i] > 0;
            if should_withdraw && amount > 0 {
                token::transfer(
                    CpiContext::new_with_signer(
                        self.token_program.to_account_info(),
                        Transfer {
                            from: vaults[i].to_account_info(),
                            to: user_atas[user_ata_idx].to_account_info(),
                            authority: self.pool.to_account_info(),
                        },
                        signer_seeds,
                    ),
                    amount as u64,
                )?;
                user_ata_idx += 1;
            }
        }
        Ok(())
    }

    /// Mint LP tokens to user
    pub fn mint_lp_tokens(&self, amount: u64, signer_seeds: &[&[&[u8]]]) -> Result<()> {
        if amount == 0 {
            return Ok(());
        }
        token::mint_to(
            CpiContext::new_with_signer(
                self.token_program.to_account_info(),
                MintTo {
                    mint: self.lp_mint.to_account_info(),
                    to: self.user_lp_account.to_account_info(),
                    authority: self.pool.to_account_info(),
                },
                signer_seeds,
            ),
            amount,
        )
    }

    /// Burn LP tokens from user
    pub fn burn_lp_tokens(&self, amount: u64) -> Result<()> {
        if amount == 0 {
            return Ok(());
        }
        token::burn(
            CpiContext::new(
                self.token_program.to_account_info(),
                Burn {
                    mint: self.lp_mint.to_account_info(),
                    from: self.user_lp_account.to_account_info(),
                    authority: self.user.to_account_info(),
                },
            ),
            amount,
        )
    }
}

pub fn add_liquidity_handler<'info>(
    ctx: Context<'_, '_, 'info, 'info, ModifyLiquidity<'info>>,
    lp_amount: u64,
    max_amounts: [u64; MAX_TOKENS],
) -> Result<()> {
    require!(lp_amount > 0, StableSwapError::ZeroAmount);

    let pool = &ctx.accounts.pool;
    let n = pool.n_tokens as usize;
    let amp = pool.amplification as u128;
    let lp_supply = ctx.accounts.lp_mint.supply as u128;
    let remaining = ctx.remaining_accounts;

    // Count non-zero deposits and validate remaining accounts
    let deposit_count = max_amounts[..n].iter().filter(|&&x| x > 0).count();
    require!(deposit_count > 0, StableSwapError::ZeroAmount);
    require!(
        remaining.len() == n + deposit_count,
        StableSwapError::InvalidRemainingAccounts
    );

    let vaults = &remaining[0..n];
    let user_atas = &remaining[n..];

    ctx.accounts.validate_vaults(vaults)?;
    let reserves = ctx.accounts.read_reserves(vaults)?;

    // Calculate deposits and LP tokens to mint
    let (deposits, lp_to_mint): (Vec<u128>, u64) = if lp_supply == 0 {
        // First deposit: use max amounts directly
        let deposits: Vec<u128> = max_amounts[..n].iter().map(|&x| x as u128).collect();
        let d = compute_d(&deposits, amp)?;
        
        // Ensure enough liquidity to lock MINIMUM_LIQUIDITY as virtual dead shares
        require!(d > MINIMUM_LIQUIDITY as u128, StableSwapError::InsufficientInitialLiquidity);
        
        // Mint D - MINIMUM_LIQUIDITY to user (MINIMUM_LIQUIDITY stays virtual/unminted)
        let lp_to_mint = (d - MINIMUM_LIQUIDITY as u128).min(u64::MAX as u128) as u64;
        (deposits, lp_to_mint)
    } else {
        // Subsequent deposits: use adjusted supply (actual + virtual dead shares)
        // This dilutes the impact of any inflation attack
        let adjusted_supply = lp_supply
            .checked_add(MINIMUM_LIQUIDITY as u128)
            .ok_or(StableSwapError::MathOverflow)?;

        let d_before = compute_d(&reserves, amp)?;
        let target_d = d_before
            .checked_mul(adjusted_supply.checked_add(lp_amount as u128).ok_or(StableSwapError::MathOverflow)?)
            .ok_or(StableSwapError::MathOverflow)?
            .checked_div(adjusted_supply)
            .ok_or(StableSwapError::MathOverflow)?;

        let total_max: u128 = max_amounts[..n].iter().map(|&x| x as u128).sum();
        require!(total_max > 0, StableSwapError::ZeroAmount);

        let d_increase = target_d.saturating_sub(d_before);
        let deposits: Vec<u128> = max_amounts[..n].iter().map(|&max| {
            if max == 0 {
                0u128
            } else {
                (max as u128)
                    .checked_mul(d_increase)
                    .unwrap_or(0)
                    .checked_div(total_max)
                    .unwrap_or(0)
                    .min(max as u128)
            }
        }).collect();

        let reserves_after: Vec<u128> = reserves.iter()
            .zip(deposits.iter())
            .map(|(r, d)| r + d)
            .collect();

        // Use adjusted supply for LP calculation
        let actual_lp = calculate_mint_amount(&reserves, &reserves_after, adjusted_supply, amp)?;
        (deposits, actual_lp.min(u64::MAX as u128) as u64)
    };

    // Verify slippage
    for (&deposit, &max) in deposits.iter().zip(max_amounts[..n].iter()) {
        if max > 0 {
            require!(deposit <= max as u128, StableSwapError::SlippageExceeded);
        }
    }

    // Execute transfers and mint LP
    ctx.accounts.transfer_to_vaults(vaults, user_atas, &deposits, &max_amounts)?;

    let seeds: &[&[u8]] = &[b"pool", pool.lp_mint.as_ref(), &[pool.bump]];
    ctx.accounts.mint_lp_tokens(lp_to_mint, &[seeds])?;

    msg!("Added liquidity: {} LP tokens minted", lp_to_mint);
    Ok(())
}

pub fn remove_liquidity_handler<'info>(
    ctx: Context<'_, '_, 'info, 'info, ModifyLiquidity<'info>>,
    lp_amount: u64,
    min_amounts: [u64; MAX_TOKENS],
) -> Result<()> {
    require!(lp_amount > 0, StableSwapError::ZeroAmount);

    let pool = &ctx.accounts.pool;
    let n = pool.n_tokens as usize;
    let lp_supply = ctx.accounts.lp_mint.supply as u128;
    let remaining = ctx.remaining_accounts;

    require!(lp_supply > 0, StableSwapError::EmptyPool);
    require!(lp_amount as u128 <= lp_supply, StableSwapError::InsufficientLiquidity);

    // Determine withdrawal mode and validate remaining accounts
    let withdraw_count = min_amounts[..n].iter().filter(|&&x| x > 0).count();
    let is_proportional = withdraw_count == 0 || withdraw_count == n;
    let user_ata_count = if is_proportional { n } else { withdraw_count };

    require!(
        remaining.len() == n + user_ata_count,
        StableSwapError::InvalidRemainingAccounts
    );

    let vaults = &remaining[0..n];
    let user_atas = &remaining[n..];

    ctx.accounts.validate_vaults(vaults)?;
    let reserves = ctx.accounts.read_reserves(vaults)?;

    // Use adjusted supply (actual + virtual dead shares) for withdrawal calculation
    // This ensures withdrawals are proportional to the diluted share value
    let adjusted_supply = lp_supply
        .checked_add(MINIMUM_LIQUIDITY as u128)
        .ok_or(StableSwapError::MathOverflow)?;

    let withdraw_amounts = calculate_withdraw_amounts(&reserves, lp_amount as u128, adjusted_supply)?;

    // Verify slippage
    for (&amount, &min) in withdraw_amounts.iter().zip(min_amounts[..n].iter()) {
        if min > 0 {
            require!(amount >= min as u128, StableSwapError::SlippageExceeded);
        }
    }

    // Burn LP and transfer tokens
    ctx.accounts.burn_lp_tokens(lp_amount)?;

    let seeds: &[&[u8]] = &[b"pool", pool.lp_mint.as_ref(), &[pool.bump]];
    ctx.accounts.transfer_from_vaults(vaults, user_atas, &withdraw_amounts, &min_amounts, is_proportional, &[seeds])?;

    msg!("Removed liquidity: {} LP tokens burned", lp_amount);
    Ok(())
}
