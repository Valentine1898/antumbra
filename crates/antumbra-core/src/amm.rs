//! Constant-product execution used by `end_epoch`, with the exact integer
//! arithmetic the wallet and the circuit witness builder must reproduce.

use primitive_types::U256;

/// Fee in basis points taken from every swap input (0.3%).
pub const FEE_BPS: u128 = 30;

/// floor(a * b / c), exact over 256 bits. Panics on c == 0.
pub fn mul_div(a: u128, b: u128, c: u128) -> u128 {
    assert!(c != 0, "division by zero");
    let r = U256::from(a) * U256::from(b) / U256::from(c);
    r.try_into().expect("quotient fits in u128")
}

/// Output of selling `delta_in` into a pool with reserves `(reserve_in, reserve_out)`.
/// Returns `(amount_out, new_reserve_in, new_reserve_out)`; a pool without
/// liquidity executes nothing.
pub fn execute(reserve_in: u128, reserve_out: u128, delta_in: u128) -> (u128, u128, u128) {
    if delta_in == 0 || reserve_in == 0 || reserve_out == 0 {
        return (0, reserve_in, reserve_out);
    }
    let effective = delta_in - mul_div(delta_in, FEE_BPS, 10_000);
    let out = mul_div(reserve_out, effective, reserve_in + effective);
    (out, reserve_in + delta_in, reserve_out - out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execute_matches_hand_computation() {
        // 100_000 in with 0.3% fee: 99_700 effective; out = floor(1e6 * 99_700 / 1_099_700) = 90_661
        let (out, r_in, r_out) = execute(1_000_000, 1_000_000, 100_000);
        assert_eq!(out, 90_661);
        assert_eq!(r_in, 1_100_000);
        assert_eq!(r_out, 1_000_000 - 90_661);
        assert_eq!(execute(0, 0, 5), (0, 0, 0));
        assert_eq!(mul_div(u128::MAX, u128::MAX, u128::MAX), u128::MAX);
    }
}
