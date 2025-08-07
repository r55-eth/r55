use alloy_core::primitives::U256;
use core::ops::{Add, BitAnd, BitOrAssign, BitXor, Div, Mul, MulAssign, Sub};

#[derive(Debug, PartialEq)]
pub enum MathError {
    DenominatorIsZero,
    DenominatorIsLteProdOne,
}

const ONE: U256 = U256::from_limbs([1, 0, 0, 0]);
const TWO: U256 = U256::from_limbs([2, 0, 0, 0]);
const THREE: U256 = U256::from_limbs([3, 0, 0, 0]);

// Code borrowed from: https://github.com/0xKitsune/uniswap-v3-math/blob/main/src/full_math.rs
pub fn mul_div(x: U256, y: U256, mut denominator: U256) -> Result<U256, MathError> {
    // 512-bit multiply [prod1 prod0] = x * y
    // Compute the product mod 2**256 and mod 2**256 - 1
    // then use the Chinese Remainder Theorem to reconstruct
    // the 512 bit result. The result is stored in two 256
    // variables such that product = prod1 * 2**256 + prod0
    let mm = x.mul_mod(y, U256::MAX);

    let mut prod_0 = x.overflowing_mul(y).0; // Least significant 256 bits of the product
    let mut prod_1 = mm
        .overflowing_sub(prod_0)
        .0
        .overflowing_sub(U256::from((mm < prod_0) as u8))
        .0;

    // Handle non-overflow cases, 256 by 256 division
    if prod_1 == U256::ZERO {
        if denominator == U256::ZERO {
            return Err(MathError::DenominatorIsZero);
        }
        return Ok(U256::from_limbs(*prod_0.div(denominator).as_limbs()));
    }

    // Make sure the result is less than 2**256.
    // Also prevents denominator == 0
    if denominator <= prod_1 {
        return Err(MathError::DenominatorIsLteProdOne);
    }

    // 512 by 256 division.

    // Make division exact by subtracting the remainder from [prod1 prod0]
    // Compute remainder using mulmod
    let remainder = x.mul_mod(y, denominator);

    // Subtract 256 bit number from 512 bit number
    prod_1 = prod_1
        .overflowing_sub(U256::from((remainder > prod_0) as u8))
        .0;
    prod_0 = prod_0.overflowing_sub(remainder).0;

    // Factor powers of two out of denominator
    // Compute largest power of two divisor of denominator.
    // Always >= 1.
    let mut twos = U256::ZERO
        .overflowing_sub(denominator)
        .0
        .bitand(denominator);

    // Divide denominator by power of two

    denominator = denominator.wrapping_div(twos);

    // Divide [prod1 prod0] by the factors of two
    prod_0 = prod_0.wrapping_div(twos);

    // Shift in bits from prod1 into prod0. For this we need
    // to flip `twos` such that it is 2**256 / twos.
    // If twos is zero, then it becomes one

    twos = (U256::ZERO.overflowing_sub(twos).0.wrapping_div(twos)).add(ONE);

    prod_0.bitor_assign(prod_1 * twos);

    // Invert denominator mod 2**256
    // Now that denominator is an odd number, it has an inverse
    // modulo 2**256 such that denominator * inv = 1 mod 2**256.
    // Compute the inverse by starting with a seed that is correct
    // for four bits. That is, denominator * inv = 1 mod 2**4

    let mut inv = THREE.mul(denominator).bitxor(TWO);

    // Now use Newton-Raphson iteration to improve the precision.
    // Thanks to Hensel's lifting lemma, this also works in modular
    // arithmetic, doubling the correct bits in each step.

    inv.mul_assign(TWO - denominator * inv); // inverse mod 2**8
    inv.mul_assign(TWO - denominator * inv); // inverse mod 2**16
    inv.mul_assign(TWO - denominator * inv); // inverse mod 2**32
    inv.mul_assign(TWO - denominator * inv); // inverse mod 2**64
    inv.mul_assign(TWO - denominator * inv); // inverse mod 2**128
    inv.mul_assign(TWO - denominator * inv); // inverse mod 2**256

    // Because the division is now exact we can divide by multiplying
    // with the modular inverse of denominator. This will give us the
    // correct result modulo 2**256. Since the preconditions guarantee
    // that the outcome is less than 2**256, this is the final result.
    // We don't need to compute the high bits of the result and prod1
    // is no longer required.

    Ok(U256::from_le_slice((prod_0 * inv).as_le_slice()))
}

pub fn sqrt(y: U256) -> U256 {
    if y > THREE {
        let mut z = y;
        let mut x = y / TWO + ONE;
        while x < z {
            z = x;
            x = (y / x + x) / TWO;
        }
        z
    } else if y != U256::ZERO {
        ONE
    } else {
        U256::ZERO
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const Q128: U256 = U256::from_limbs([0, 0, 1, 0]);

    #[test]
    fn test_fails_mul_div() {
        //Revert if the denominator is zero
        let result = mul_div(Q128, U256::from(5), U256::ZERO);
        assert_eq!(result, Err(MathError::DenominatorIsZero));

        // Revert if the denominator is zero and numerator overflows
        let result = mul_div(Q128, Q128, U256::ZERO);
        assert_eq!(result, Err(MathError::DenominatorIsLteProdOne));

        // Revert if the output overflows U256
        let result = mul_div(Q128, Q128, ONE);
        assert_eq!(result, Err(MathError::DenominatorIsLteProdOne));

        // Reverts on overflow with all max inputs
        let result = mul_div(U256::MAX, U256::MAX, U256::MAX.sub(ONE));
        assert_eq!(result.err().unwrap(), MathError::DenominatorIsLteProdOne);

        // All max inputs
        let result = mul_div(U256::MAX, U256::MAX, U256::MAX);
        assert_eq!(result, Ok(U256::MAX));

        // Accurate without phantom overflow
        let result = mul_div(
            Q128,
            U256::from(50).mul(Q128).div(U256::from(100)),
            U256::from(150).mul(Q128).div(U256::from(100)),
        );
        assert_eq!(result, Ok(Q128.div(U256::from(3))));

        // Accurate with phantom overflow
        let result = mul_div(Q128, U256::from(35).mul(Q128), U256::from(8).mul(Q128));
        assert_eq!(result, Ok(U256::from(4375).mul(Q128).div(U256::from(1000)))
        );

        // Accurate with phantom overflow and repeating decimal
        let result = mul_div(Q128, U256::from(1000).mul(Q128), U256::from(3000).mul(Q128));
        assert_eq!(result, Ok(Q128.div(U256::from(3))));
    }
}

