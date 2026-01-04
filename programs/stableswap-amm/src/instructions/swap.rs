use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer};
use crate::errors::StableSwapError;
use crate::math::calculate_swap;
use crate::state::Pool;

#[derive(Accounts)]
pub struct Swap<'info> {
    #[account(mut)]
    pub pool: Account<'info, Pool>,

    #[account(mut)]
    pub user: Signer<'info>,

    pub token_program: Program<'info, Token>,
}

impl<'info> Swap<'info> {
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

    /// Transfer input tokens from user to vault
    pub fn transfer_in(
        &self,
        user_ata: &AccountInfo<'info>,
        vault: &AccountInfo<'info>,
        amount: u64,
    ) -> Result<()> {
        token::transfer(
            CpiContext::new(
                self.token_program.to_account_info(),
                Transfer {
                    from: user_ata.to_account_info(),
                    to: vault.to_account_info(),
                    authority: self.user.to_account_info(),
                },
            ),
            amount,
        )
    }

    /// Transfer output tokens from vault to user
    pub fn transfer_out(
        &self,
        vault: &AccountInfo<'info>,
        user_ata: &AccountInfo<'info>,
        amount: u64,
        signer_seeds: &[&[&[u8]]],
    ) -> Result<()> {
        token::transfer(
            CpiContext::new_with_signer(
                self.token_program.to_account_info(),
                Transfer {
                    from: vault.to_account_info(),
                    to: user_ata.to_account_info(),
                    authority: self.pool.to_account_info(),
                },
                signer_seeds,
            ),
            amount,
        )
    }
}

pub fn swap_handler<'info>(
    ctx: Context<'_, '_, 'info, 'info, Swap<'info>>,
    amount_in: u64,
    min_amount_out: u64,
    input_index: u8,
    output_index: u8,
) -> Result<()> {
    require!(amount_in > 0, StableSwapError::ZeroAmount);

    let pool = &ctx.accounts.pool;
    let n = pool.n_tokens as usize;
    let remaining = ctx.remaining_accounts;

    // Validate indices
    require!(input_index < pool.n_tokens, StableSwapError::InvalidTokenIndex);
    require!(output_index < pool.n_tokens, StableSwapError::InvalidTokenIndex);
    require!(input_index != output_index, StableSwapError::SameTokenSwap);

    // Validate remaining accounts: n vaults + 2 user ATAs
    require!(remaining.len() == n + 2, StableSwapError::InvalidRemainingAccounts);

    let vaults = &remaining[0..n];
    let user_input = &remaining[n];
    let user_output = &remaining[n + 1];

    ctx.accounts.validate_vaults(vaults)?;
    let reserves = ctx.accounts.read_reserves(vaults)?;

    // Calculate swap
    let (amount_out, fee_amount) = calculate_swap(
        &reserves,
        input_index as usize,
        output_index as usize,
        amount_in as u128,
        pool.amplification as u128,
        pool.fee_bps,
    )?;

    require!(amount_out >= min_amount_out as u128, StableSwapError::SlippageExceeded);

    // Execute transfers
    ctx.accounts.transfer_in(user_input, &vaults[input_index as usize], amount_in)?;

    let seeds: &[&[u8]] = &[b"pool", pool.lp_mint.as_ref(), &[pool.bump]];
    ctx.accounts.transfer_out(&vaults[output_index as usize], user_output, amount_out as u64, &[seeds])?;

    msg!("Swap: {} in -> {} out (fee: {})", amount_in, amount_out, fee_amount);
    Ok(())
}
