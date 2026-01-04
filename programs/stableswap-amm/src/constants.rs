/// Maximum number of tokens supported in a pool
pub const MAX_TOKENS: usize = 6;

/// Minimum liquidity locked as virtual dead shares on first deposit.
/// Prevents first depositor inflation attacks (Uniswap V2 style).
pub const MINIMUM_LIQUIDITY: u64 = 1_000;

/// Precision for math calculations (10^18)
pub const PRECISION: u128 = 1_000_000_000_000_000_000;

/// Maximum amplification parameter
pub const MAX_AMP: u64 = 1_000_000;

/// Maximum fee in basis points (100% = 10000 bps)
pub const MAX_FEE_BPS: u16 = 10000;

/// Minimum number of tokens in a pool
pub const MIN_TOKENS: usize = 2;

/// Maximum iterations for Newton's method
pub const MAX_ITERATIONS: u8 = 255;
