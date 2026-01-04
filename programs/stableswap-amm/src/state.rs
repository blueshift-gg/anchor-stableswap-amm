use anchor_lang::prelude::*;
use crate::constants::MAX_TOKENS;

#[account]
pub struct Pool {
    /// Admin authority that can modify pool parameters
    pub admin: Pubkey,
    /// LP token mint
    pub lp_mint: Pubkey,
    /// Amplification coefficient (A parameter)
    pub amplification: u64,
    /// Swap fee in basis points (1 bps = 0.01%)
    pub fee_bps: u16,
    /// Number of tokens in the pool (2-6)
    pub n_tokens: u8,
    /// Token mints in the pool (unused slots are Pubkey::default())
    pub token_mints: [Pubkey; MAX_TOKENS],
    /// PDA bump seed
    pub bump: u8,
}

impl Pool {
    pub const LEN: usize = 8 +  // discriminator
        32 +                    // admin
        32 +                    // lp_mint
        8 +                     // amplification
        2 +                     // fee_bps
        1 +                     // n_tokens
        32 * MAX_TOKENS +       // token_mints
        1;                      // bump

    /// Get the active token mints (non-default pubkeys)
    pub fn active_mints(&self) -> &[Pubkey] {
        &self.token_mints[..self.n_tokens as usize]
    }

    /// Check if a mint is in the pool and return its index
    pub fn find_mint_index(&self, mint: &Pubkey) -> Option<usize> {
        self.active_mints().iter().position(|m| m == mint)
    }
}
