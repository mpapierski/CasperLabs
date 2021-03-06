use alloc::vec::Vec;

use num_traits::{AsPrimitive, Bounded, Num, One, Unsigned, WrappingAdd, WrappingSub, Zero};

use crate::bytesrepr::{self, Error, FromBytes, ToBytes};

#[allow(
    clippy::assign_op_pattern,
    clippy::ptr_offset_with_cast,
    clippy::range_plus_one,
    clippy::transmute_ptr_to_ptr
)]
mod macro_code {
    use uint::construct_uint;

    construct_uint! {
        pub struct U512(8);
    }
    construct_uint! {
        pub struct U256(4);
    }
    construct_uint! {
        pub struct U128(2);
    }
}

pub use self::macro_code::{U128, U256, U512};

/// Error type for parsing U128, U256, U512 from a string.
/// `FromDecStr` is the parsing error from the `uint` crate, which
/// only supports base-10 parsing. `InvalidRadix` is raised when
/// parsing is attempted on any string representing the number in some
/// base other than 10 presently, however a general radix may be
/// supported in the future.
#[derive(Debug)]
pub enum UIntParseError {
    FromDecStr(uint::FromDecStrErr),
    InvalidRadix,
}

macro_rules! ser_and_num_impls {
    ($type:ident, $total_bytes:expr) => {
        impl ToBytes for $type {
            fn to_bytes(&self) -> Result<Vec<u8>, Error> {
                let mut buf = [0u8; $total_bytes];
                self.to_little_endian(&mut buf);
                let mut non_zero_bytes: Vec<u8> =
                    buf.iter().rev().skip_while(|b| **b == 0).cloned().collect();
                let num_bytes = non_zero_bytes.len() as u8;
                non_zero_bytes.push(num_bytes);
                non_zero_bytes.reverse();
                Ok(non_zero_bytes)
            }
        }

        impl FromBytes for $type {
            fn from_bytes(bytes: &[u8]) -> Result<(Self, &[u8]), Error> {
                let (num_bytes, rem): (u8, &[u8]) = FromBytes::from_bytes(bytes)?;

                if num_bytes > $total_bytes {
                    Err(Error::FormattingError)
                } else {
                    let (value, rem) = bytesrepr::safe_split_at(rem, num_bytes as usize)?;
                    let result = $type::from_little_endian(value);
                    Ok((result, rem))
                }
            }
        }

        // Trait implementations for unifying U* as numeric types
        impl Zero for $type {
            fn zero() -> Self {
                $type::zero()
            }

            fn is_zero(&self) -> bool {
                self.is_zero()
            }
        }

        impl One for $type {
            fn one() -> Self {
                $type::one()
            }
        }

        // Requires Zero and One to be implemented
        impl Num for $type {
            type FromStrRadixErr = UIntParseError;
            fn from_str_radix(str: &str, radix: u32) -> Result<Self, Self::FromStrRadixErr> {
                if radix == 10 {
                    $type::from_dec_str(str).map_err(UIntParseError::FromDecStr)
                } else {
                    // TODO: other radix parsing
                    Err(UIntParseError::InvalidRadix)
                }
            }
        }

        // Requires Num to be implemented
        impl Unsigned for $type {}

        // Additional numeric trait, which also holds for these types
        impl Bounded for $type {
            fn min_value() -> Self {
                $type::zero()
            }

            fn max_value() -> Self {
                $type::MAX
            }
        }

        // Instead of implementing arbitrary methods we can use existing traits from num_trait
        // crate.
        impl WrappingAdd for $type {
            fn wrapping_add(&self, other: &$type) -> $type {
                self.overflowing_add(*other).0
            }
        }

        impl WrappingSub for $type {
            fn wrapping_sub(&self, other: &$type) -> $type {
                self.overflowing_sub(*other).0
            }
        }

        impl AsPrimitive<$type> for i32 {
            fn as_(self) -> $type {
                if self >= 0 {
                    $type::from(self as u32)
                } else {
                    let abs = 0u32.wrapping_sub(self as u32);
                    $type::zero().wrapping_sub(&$type::from(abs))
                }
            }
        }

        impl AsPrimitive<$type> for i64 {
            fn as_(self) -> $type {
                if self >= 0 {
                    $type::from(self as u64)
                } else {
                    let abs = 0u64.wrapping_sub(self as u64);
                    $type::zero().wrapping_sub(&$type::from(abs))
                }
            }
        }

        impl AsPrimitive<$type> for u8 {
            fn as_(self) -> $type {
                $type::from(self)
            }
        }

        impl AsPrimitive<$type> for u32 {
            fn as_(self) -> $type {
                $type::from(self)
            }
        }

        impl AsPrimitive<$type> for u64 {
            fn as_(self) -> $type {
                $type::from(self)
            }
        }

        impl AsPrimitive<i32> for $type {
            fn as_(self) -> i32 {
                self.0[0] as i32
            }
        }

        impl AsPrimitive<i64> for $type {
            fn as_(self) -> i64 {
                self.0[0] as i64
            }
        }

        impl AsPrimitive<u8> for $type {
            fn as_(self) -> u8 {
                self.0[0] as u8
            }
        }

        impl AsPrimitive<u32> for $type {
            fn as_(self) -> u32 {
                self.0[0] as u32
            }
        }

        impl AsPrimitive<u64> for $type {
            fn as_(self) -> u64 {
                self.0[0]
            }
        }
    };
}

ser_and_num_impls!(U128, 16);
ser_and_num_impls!(U256, 32);
ser_and_num_impls!(U512, 64);

impl AsPrimitive<U128> for U128 {
    fn as_(self) -> U128 {
        self
    }
}

impl AsPrimitive<U256> for U128 {
    fn as_(self) -> U256 {
        let mut result = U256::zero();
        result.0[..2].clone_from_slice(&self.0[..2]);
        result
    }
}

impl AsPrimitive<U512> for U128 {
    fn as_(self) -> U512 {
        let mut result = U512::zero();
        result.0[..2].clone_from_slice(&self.0[..2]);
        result
    }
}

impl AsPrimitive<U128> for U256 {
    fn as_(self) -> U128 {
        let mut result = U128::zero();
        result.0[..2].clone_from_slice(&self.0[..2]);
        result
    }
}

impl AsPrimitive<U256> for U256 {
    fn as_(self) -> U256 {
        self
    }
}

impl AsPrimitive<U512> for U256 {
    fn as_(self) -> U512 {
        let mut result = U512::zero();
        result.0[..4].clone_from_slice(&self.0[..4]);
        result
    }
}

impl AsPrimitive<U128> for U512 {
    fn as_(self) -> U128 {
        let mut result = U128::zero();
        result.0[..2].clone_from_slice(&self.0[..2]);
        result
    }
}

impl AsPrimitive<U256> for U512 {
    fn as_(self) -> U256 {
        let mut result = U256::zero();
        result.0[..4].clone_from_slice(&self.0[..4]);
        result
    }
}

impl AsPrimitive<U512> for U512 {
    fn as_(self) -> U512 {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check_as_i32<T: AsPrimitive<i32>>(expected: i32, input: T) {
        assert_eq!(expected, input.as_());
    }

    fn check_as_i64<T: AsPrimitive<i64>>(expected: i64, input: T) {
        assert_eq!(expected, input.as_());
    }

    fn check_as_u8<T: AsPrimitive<u8>>(expected: u8, input: T) {
        assert_eq!(expected, input.as_());
    }

    fn check_as_u32<T: AsPrimitive<u32>>(expected: u32, input: T) {
        assert_eq!(expected, input.as_());
    }

    fn check_as_u64<T: AsPrimitive<u64>>(expected: u64, input: T) {
        assert_eq!(expected, input.as_());
    }

    fn check_as_u128<T: AsPrimitive<U128>>(expected: U128, input: T) {
        assert_eq!(expected, input.as_());
    }

    fn check_as_u256<T: AsPrimitive<U256>>(expected: U256, input: T) {
        assert_eq!(expected, input.as_());
    }

    fn check_as_u512<T: AsPrimitive<U512>>(expected: U512, input: T) {
        assert_eq!(expected, input.as_());
    }

    #[test]
    fn as_primitive_from_i32() {
        let mut input = 0_i32;
        check_as_i32(0, input);
        check_as_i64(0, input);
        check_as_u8(0, input);
        check_as_u32(0, input);
        check_as_u64(0, input);
        check_as_u128(U128::zero(), input);
        check_as_u256(U256::zero(), input);
        check_as_u512(U512::zero(), input);

        input = i32::max_value() - 1;
        check_as_i32(input, input);
        check_as_i64(i64::from(input), input);
        check_as_u8(input as u8, input);
        check_as_u32(input as u32, input);
        check_as_u64(input as u64, input);
        check_as_u128(U128::from(input), input);
        check_as_u256(U256::from(input), input);
        check_as_u512(U512::from(input), input);

        input = i32::min_value() + 1;
        check_as_i32(input, input);
        check_as_i64(i64::from(input), input);
        check_as_u8(input as u8, input);
        check_as_u32(input as u32, input);
        check_as_u64(input as u64, input);
        // i32::min_value() is -1 - i32::max_value()
        check_as_u128(
            U128::zero().wrapping_sub(&U128::from(i32::max_value())),
            input,
        );
        check_as_u256(
            U256::zero().wrapping_sub(&U256::from(i32::max_value())),
            input,
        );
        check_as_u512(
            U512::zero().wrapping_sub(&U512::from(i32::max_value())),
            input,
        );
    }

    #[test]
    fn as_primitive_from_i64() {
        let mut input = 0_i64;
        check_as_i32(0, input);
        check_as_i64(0, input);
        check_as_u8(0, input);
        check_as_u32(0, input);
        check_as_u64(0, input);
        check_as_u128(U128::zero(), input);
        check_as_u256(U256::zero(), input);
        check_as_u512(U512::zero(), input);

        input = i64::max_value() - 1;
        check_as_i32(input as i32, input);
        check_as_i64(input, input);
        check_as_u8(input as u8, input);
        check_as_u32(input as u32, input);
        check_as_u64(input as u64, input);
        check_as_u128(U128::from(input), input);
        check_as_u256(U256::from(input), input);
        check_as_u512(U512::from(input), input);

        input = i64::min_value() + 1;
        check_as_i32(input as i32, input);
        check_as_i64(input, input);
        check_as_u8(input as u8, input);
        check_as_u32(input as u32, input);
        check_as_u64(input as u64, input);
        // i64::min_value() is (-1 - i64::max_value())
        check_as_u128(
            U128::zero().wrapping_sub(&U128::from(i64::max_value())),
            input,
        );
        check_as_u256(
            U256::zero().wrapping_sub(&U256::from(i64::max_value())),
            input,
        );
        check_as_u512(
            U512::zero().wrapping_sub(&U512::from(i64::max_value())),
            input,
        );
    }

    #[test]
    fn as_primitive_from_u8() {
        let mut input = 0_u8;
        check_as_i32(0, input);
        check_as_i64(0, input);
        check_as_u8(0, input);
        check_as_u32(0, input);
        check_as_u64(0, input);
        check_as_u128(U128::zero(), input);
        check_as_u256(U256::zero(), input);
        check_as_u512(U512::zero(), input);

        input = u8::max_value() - 1;
        check_as_i32(i32::from(input), input);
        check_as_i64(i64::from(input), input);
        check_as_u8(input, input);
        check_as_u32(u32::from(input), input);
        check_as_u64(u64::from(input), input);
        check_as_u128(U128::from(input), input);
        check_as_u256(U256::from(input), input);
        check_as_u512(U512::from(input), input);
    }

    #[test]
    fn as_primitive_from_u32() {
        let mut input = 0_u32;
        check_as_i32(0, input);
        check_as_i64(0, input);
        check_as_u8(0, input);
        check_as_u32(0, input);
        check_as_u64(0, input);
        check_as_u128(U128::zero(), input);
        check_as_u256(U256::zero(), input);
        check_as_u512(U512::zero(), input);

        input = u32::max_value() - 1;
        check_as_i32(input as i32, input);
        check_as_i64(i64::from(input), input);
        check_as_u8(input as u8, input);
        check_as_u32(input, input);
        check_as_u64(u64::from(input), input);
        check_as_u128(U128::from(input), input);
        check_as_u256(U256::from(input), input);
        check_as_u512(U512::from(input), input);
    }

    #[test]
    fn as_primitive_from_u64() {
        let mut input = 0_u64;
        check_as_i32(0, input);
        check_as_i64(0, input);
        check_as_u8(0, input);
        check_as_u32(0, input);
        check_as_u64(0, input);
        check_as_u128(U128::zero(), input);
        check_as_u256(U256::zero(), input);
        check_as_u512(U512::zero(), input);

        input = u64::max_value() - 1;
        check_as_i32(input as i32, input);
        check_as_i64(input as i64, input);
        check_as_u8(input as u8, input);
        check_as_u32(input as u32, input);
        check_as_u64(input, input);
        check_as_u128(U128::from(input), input);
        check_as_u256(U256::from(input), input);
        check_as_u512(U512::from(input), input);
    }

    fn make_little_endian_arrays(little_endian_bytes: &[u8]) -> ([u8; 4], [u8; 8]) {
        let le_32 = {
            let mut le_32 = [0; 4];
            le_32.copy_from_slice(&little_endian_bytes[..4]);
            le_32
        };

        let le_64 = {
            let mut le_64 = [0; 8];
            le_64.copy_from_slice(&little_endian_bytes[..8]);
            le_64
        };

        (le_32, le_64)
    }

    #[test]
    fn as_primitive_from_u128() {
        let mut input = U128::zero();
        check_as_i32(0, input);
        check_as_i64(0, input);
        check_as_u8(0, input);
        check_as_u32(0, input);
        check_as_u64(0, input);
        check_as_u128(U128::zero(), input);
        check_as_u256(U256::zero(), input);
        check_as_u512(U512::zero(), input);

        input = U128::max_value() - 1;

        let mut little_endian_bytes = [0_u8; 64];
        input.to_little_endian(&mut little_endian_bytes[..16]);
        let (le_32, le_64) = make_little_endian_arrays(&little_endian_bytes);

        check_as_i32(i32::from_le_bytes(le_32), input);
        check_as_i64(i64::from_le_bytes(le_64), input);
        check_as_u8(little_endian_bytes[0], input);
        check_as_u32(u32::from_le_bytes(le_32), input);
        check_as_u64(u64::from_le_bytes(le_64), input);
        check_as_u128(U128::from_little_endian(&little_endian_bytes[..16]), input);
        check_as_u256(U256::from_little_endian(&little_endian_bytes[..32]), input);
        check_as_u512(U512::from_little_endian(&little_endian_bytes), input);
    }

    #[test]
    fn as_primitive_from_u256() {
        let mut input = U256::zero();
        check_as_i32(0, input);
        check_as_i64(0, input);
        check_as_u8(0, input);
        check_as_u32(0, input);
        check_as_u64(0, input);
        check_as_u128(U128::zero(), input);
        check_as_u256(U256::zero(), input);
        check_as_u512(U512::zero(), input);

        input = U256::max_value() - 1;

        let mut little_endian_bytes = [0_u8; 64];
        input.to_little_endian(&mut little_endian_bytes[..32]);
        let (le_32, le_64) = make_little_endian_arrays(&little_endian_bytes);

        check_as_i32(i32::from_le_bytes(le_32), input);
        check_as_i64(i64::from_le_bytes(le_64), input);
        check_as_u8(little_endian_bytes[0], input);
        check_as_u32(u32::from_le_bytes(le_32), input);
        check_as_u64(u64::from_le_bytes(le_64), input);
        check_as_u128(U128::from_little_endian(&little_endian_bytes[..16]), input);
        check_as_u256(U256::from_little_endian(&little_endian_bytes[..32]), input);
        check_as_u512(U512::from_little_endian(&little_endian_bytes), input);
    }

    #[test]
    fn as_primitive_from_u512() {
        let mut input = U512::zero();
        check_as_i32(0, input);
        check_as_i64(0, input);
        check_as_u8(0, input);
        check_as_u32(0, input);
        check_as_u64(0, input);
        check_as_u128(U128::zero(), input);
        check_as_u256(U256::zero(), input);
        check_as_u512(U512::zero(), input);

        input = U512::max_value() - 1;

        let mut little_endian_bytes = [0_u8; 64];
        input.to_little_endian(&mut little_endian_bytes);
        let (le_32, le_64) = make_little_endian_arrays(&little_endian_bytes);

        check_as_i32(i32::from_le_bytes(le_32), input);
        check_as_i64(i64::from_le_bytes(le_64), input);
        check_as_u8(little_endian_bytes[0], input);
        check_as_u32(u32::from_le_bytes(le_32), input);
        check_as_u64(u64::from_le_bytes(le_64), input);
        check_as_u128(U128::from_little_endian(&little_endian_bytes[..16]), input);
        check_as_u256(U256::from_little_endian(&little_endian_bytes[..32]), input);
        check_as_u512(U512::from_little_endian(&little_endian_bytes), input);
    }

    #[test]
    fn wrapping_test_u512() {
        let max = U512::max_value();
        let value = max.wrapping_add(&1.into());
        assert_eq!(value, 0.into());

        let min = U512::min_value();
        let value = min.wrapping_sub(&1.into());
        assert_eq!(value, U512::max_value());
    }

    #[test]
    fn wrapping_test_u256() {
        let max = U256::max_value();
        let value = max.wrapping_add(&1.into());
        assert_eq!(value, 0.into());

        let min = U256::min_value();
        let value = min.wrapping_sub(&1.into());
        assert_eq!(value, U256::max_value());
    }

    #[test]
    fn wrapping_test_u128() {
        let max = U128::max_value();
        let value = max.wrapping_add(&1.into());
        assert_eq!(value, 0.into());

        let min = U128::min_value();
        let value = min.wrapping_sub(&1.into());
        assert_eq!(value, U128::max_value());
    }
}
