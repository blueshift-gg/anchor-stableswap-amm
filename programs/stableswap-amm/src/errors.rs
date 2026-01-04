use anchor_lang::prelude::*;

#[error_code]
pub enum StableSwapError {
    #[msg("Invalid amplification parameter")]
    InvalidAmplification,

    #[msg("Invalid fee: exceeds maximum")]
    InvalidFee,

    #[msg("Invalid number of tokens: must be between 2 and 6")]
    InvalidTokenCount,

    #[msg("Invalid token index")]
    InvalidTokenIndex,

    #[msg("Invalid vault account")]
    InvalidVault,

    #[msg("Invalid mint account")]
    InvalidMint,

    #[msg("Slippage exceeded: output less than minimum")]
    SlippageExceeded,

    #[msg("Insufficient liquidity in pool")]
    InsufficientLiquidity,

    #[msg("Math overflow")]
    MathOverflow,

    #[msg("Zero amount not allowed")]
    ZeroAmount,

    #[msg("Duplicate token mints not allowed")]
    DuplicateMint,

    #[msg("Convergence failed in Newton's method")]
    ConvergenceFailed,

    #[msg("Invalid remaining accounts count")]
    InvalidRemainingAccounts,

    #[msg("Same token swap not allowed")]
    SameTokenSwap,

    #[msg("Pool is empty")]
    EmptyPool,

    #[msg("Initial liquidity too low: must exceed MINIMUM_LIQUIDITY")]
    InsufficientInitialLiquidity,
}
