use anchor_lang::prelude::*;
use crate::errors::StableSwapError;

/// Compute the StableSwap invariant D for n tokens using Newton's method.
/// 
/// The invariant equation:
/// A * n^n * sum(x_i) + D = A * D * n^n + D^(n+1) / (n^n * prod(x_i))
///
/// # Arguments
/// * `reserves` - Current reserve amounts for each token
/// * `amp` - Amplification coefficient
///
/// # Returns
/// The invariant D value
pub fn compute_d(reserves: &[u128], amp: u128) -> Result<u128> {
    let n = reserves.len() as u128;
    if n == 0 {
        return Ok(0);
    }

    let sum: u128 = reserves.iter().sum();
    if sum == 0 {
        return Ok(0);
    }

    // A * n^n
    let ann = amp.checked_mul(n.pow(n as u32))
        .ok_or(StableSwapError::MathOverflow)?;

    let mut d = sum;
    
    for _ in 0..255 {
        let d_prev = d;
        
        // D_P = D^(n+1) / (n^n * prod(reserves))
        let mut d_p = d;
        for &reserve in reserves {
            if reserve == 0 {
                return Err(StableSwapError::EmptyPool.into());
            }
            // d_p = d_p * d / (reserve * n)
            d_p = d_p
                .checked_mul(d)
                .ok_or(StableSwapError::MathOverflow)?
                .checked_div(reserve.checked_mul(n).ok_or(StableSwapError::MathOverflow)?)
                .ok_or(StableSwapError::MathOverflow)?;
        }

        // Newton's method:
        // d = (ann * sum + d_p * n) * d / ((ann - 1) * d + (n + 1) * d_p)
        let numerator = ann
            .checked_mul(sum)
            .ok_or(StableSwapError::MathOverflow)?
            .checked_add(d_p.checked_mul(n).ok_or(StableSwapError::MathOverflow)?)
            .ok_or(StableSwapError::MathOverflow)?
            .checked_mul(d)
            .ok_or(StableSwapError::MathOverflow)?;

        let denominator = ann
            .checked_sub(1)
            .ok_or(StableSwapError::MathOverflow)?
            .checked_mul(d)
            .ok_or(StableSwapError::MathOverflow)?
            .checked_add(
                n.checked_add(1)
                    .ok_or(StableSwapError::MathOverflow)?
                    .checked_mul(d_p)
                    .ok_or(StableSwapError::MathOverflow)?
            )
            .ok_or(StableSwapError::MathOverflow)?;

        d = numerator.checked_div(denominator).ok_or(StableSwapError::MathOverflow)?;

        // Check convergence
        if d.abs_diff(d_prev) <= 1 {
            return Ok(d);
        }
    }

    Err(StableSwapError::ConvergenceFailed.into())
}

/// Compute the new reserve of token j after swapping.
/// Given: we're adding to reserve[i] and want to know new reserve[j]
///
/// # Arguments
/// * `reserves` - Current reserves (before swap)
/// * `i` - Index of input token
/// * `j` - Index of output token
/// * `new_reserve_i` - New reserve of token i (after adding input)
/// * `amp` - Amplification coefficient
///
/// # Returns
/// New reserve of token j
pub fn compute_y(
    reserves: &[u128],
    i: usize,
    j: usize,
    new_reserve_i: u128,
    amp: u128,
) -> Result<u128> {
    let n = reserves.len();
    require!(i < n && j < n, StableSwapError::InvalidTokenIndex);
    require!(i != j, StableSwapError::SameTokenSwap);

    let d = compute_d(reserves, amp)?;
    compute_y_given_d(reserves, i, j, new_reserve_i, d, amp)
}

/// Compute Y given a known D value (more efficient when D is already computed)
pub fn compute_y_given_d(
    reserves: &[u128],
    i: usize,
    j: usize,
    new_reserve_i: u128,
    d: u128,
    amp: u128,
) -> Result<u128> {
    let n = reserves.len() as u128;
    let ann = amp.checked_mul(n.pow(n as u32))
        .ok_or(StableSwapError::MathOverflow)?;

    // c = D^(n+1) / (n^n * prod(reserves_except_j) * ann)
    // where reserves_except_j uses new_reserve_i for position i
    let mut c = d;
    let mut s: u128 = 0;

    for (k, &reserve) in reserves.iter().enumerate() {
        let x = if k == i {
            new_reserve_i
        } else if k == j {
            continue; // Skip j, we're solving for it
        } else {
            reserve
        };

        s = s.checked_add(x).ok_or(StableSwapError::MathOverflow)?;
        
        // c = c * d / (x * n)
        c = c
            .checked_mul(d)
            .ok_or(StableSwapError::MathOverflow)?
            .checked_div(x.checked_mul(n).ok_or(StableSwapError::MathOverflow)?)
            .ok_or(StableSwapError::MathOverflow)?;
    }

    // c = c * d / (ann * n)
    c = c
        .checked_mul(d)
        .ok_or(StableSwapError::MathOverflow)?
        .checked_div(ann.checked_mul(n).ok_or(StableSwapError::MathOverflow)?)
        .ok_or(StableSwapError::MathOverflow)?;

    // b = s + d / ann
    let b = s
        .checked_add(d.checked_div(ann).ok_or(StableSwapError::MathOverflow)?)
        .ok_or(StableSwapError::MathOverflow)?;

    // Newton's method to solve for y
    let mut y = d;

    for _ in 0..255 {
        let y_prev = y;
        
        // y = (y^2 + c) / (2y + b - d)
        let numerator = y
            .checked_mul(y)
            .ok_or(StableSwapError::MathOverflow)?
            .checked_add(c)
            .ok_or(StableSwapError::MathOverflow)?;

        let denominator = y
            .checked_mul(2)
            .ok_or(StableSwapError::MathOverflow)?
            .checked_add(b)
            .ok_or(StableSwapError::MathOverflow)?
            .checked_sub(d)
            .ok_or(StableSwapError::MathOverflow)?;

        y = numerator.checked_div(denominator).ok_or(StableSwapError::MathOverflow)?;

        if y.abs_diff(y_prev) <= 1 {
            return Ok(y);
        }
    }

    Err(StableSwapError::ConvergenceFailed.into())
}

/// Calculate the swap output amount given an input amount.
///
/// # Arguments
/// * `reserves` - Current reserves
/// * `input_index` - Index of input token
/// * `output_index` - Index of output token  
/// * `amount_in` - Amount of input token
/// * `amp` - Amplification coefficient
/// * `fee_bps` - Fee in basis points
///
/// # Returns
/// (amount_out, fee_amount)
pub fn calculate_swap(
    reserves: &[u128],
    input_index: usize,
    output_index: usize,
    amount_in: u128,
    amp: u128,
    fee_bps: u16,
) -> Result<(u128, u128)> {
    require!(amount_in > 0, StableSwapError::ZeroAmount);
    
    let new_reserve_in = reserves[input_index]
        .checked_add(amount_in)
        .ok_or(StableSwapError::MathOverflow)?;

    let new_reserve_out = compute_y(reserves, input_index, output_index, new_reserve_in, amp)?;

    let amount_out_before_fee = reserves[output_index]
        .checked_sub(new_reserve_out)
        .ok_or(StableSwapError::MathOverflow)?;

    // Calculate fee
    let fee_amount = amount_out_before_fee
        .checked_mul(fee_bps as u128)
        .ok_or(StableSwapError::MathOverflow)?
        .checked_div(10000)
        .ok_or(StableSwapError::MathOverflow)?;

    let amount_out = amount_out_before_fee
        .checked_sub(fee_amount)
        .ok_or(StableSwapError::MathOverflow)?;

    Ok((amount_out, fee_amount))
}

/// Calculate how many LP tokens to mint for a deposit.
/// Uses the change in D to determine fair LP minting.
///
/// # Arguments
/// * `reserves_before` - Reserves before deposit
/// * `reserves_after` - Reserves after deposit
/// * `lp_supply` - Current LP token supply
/// * `amp` - Amplification coefficient
///
/// # Returns
/// Amount of LP tokens to mint
pub fn calculate_mint_amount(
    reserves_before: &[u128],
    reserves_after: &[u128],
    lp_supply: u128,
    amp: u128,
) -> Result<u128> {
    let d_before = compute_d(reserves_before, amp)?;
    let d_after = compute_d(reserves_after, amp)?;

    if d_before == 0 {
        // First deposit - mint D amount of LP tokens
        return Ok(d_after);
    }

    // mint_amount = lp_supply * (d_after - d_before) / d_before
    let d_diff = d_after
        .checked_sub(d_before)
        .ok_or(StableSwapError::MathOverflow)?;

    let mint_amount = lp_supply
        .checked_mul(d_diff)
        .ok_or(StableSwapError::MathOverflow)?
        .checked_div(d_before)
        .ok_or(StableSwapError::MathOverflow)?;

    Ok(mint_amount)
}

/// Calculate token amounts to withdraw for burning LP tokens.
/// Proportional withdrawal across all tokens.
///
/// # Arguments
/// * `reserves` - Current reserves
/// * `lp_amount` - LP tokens being burned
/// * `lp_supply` - Total LP supply
///
/// # Returns
/// Vector of amounts to withdraw for each token
pub fn calculate_withdraw_amounts(
    reserves: &[u128],
    lp_amount: u128,
    lp_supply: u128,
) -> Result<Vec<u128>> {
    require!(lp_supply > 0, StableSwapError::EmptyPool);
    require!(lp_amount <= lp_supply, StableSwapError::InsufficientLiquidity);

    let mut amounts = Vec::with_capacity(reserves.len());
    
    for &reserve in reserves {
        let amount = reserve
            .checked_mul(lp_amount)
            .ok_or(StableSwapError::MathOverflow)?
            .checked_div(lp_supply)
            .ok_or(StableSwapError::MathOverflow)?;
        amounts.push(amount);
    }

    Ok(amounts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_d_balanced() {
        // 2-token pool with 1M each, A=100
        let reserves = vec![1_000_000_000_000u128, 1_000_000_000_000u128];
        let amp = 100u128;
        
        let d = compute_d(&reserves, amp).unwrap();
        
        // D should be approximately 2M for a balanced pool
        assert!(d > 1_900_000_000_000u128);
        assert!(d < 2_100_000_000_000u128);
    }

    #[test]
    fn test_compute_d_three_tokens() {
        // 3-token pool
        let reserves = vec![1_000_000u128, 1_000_000u128, 1_000_000u128];
        let amp = 100u128;
        
        let d = compute_d(&reserves, amp).unwrap();
        
        // D should be approximately 3M for a balanced 3-token pool
        assert!(d > 2_900_000u128);
        assert!(d < 3_100_000u128);
    }

    #[test]
    fn test_swap_calculation() {
        let reserves = vec![1_000_000_000_000u128, 1_000_000_000_000u128];
        let amp = 100u128;
        let fee_bps = 4u16; // 0.04%
        
        let (amount_out, _fee) = calculate_swap(&reserves, 0, 1, 1_000_000u128, amp, fee_bps).unwrap();
        
        // For stableswap with high A, output should be very close to input
        assert!(amount_out > 990_000u128); // Very low slippage
        assert!(amount_out < 1_000_000u128); // But still less due to fee
    }
}
