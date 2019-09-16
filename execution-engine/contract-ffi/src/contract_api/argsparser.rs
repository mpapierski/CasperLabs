use crate::bytesrepr;
use crate::value::Value;
use alloc::vec::Vec;
use bytesrepr::{Error, ToBytes};

/// Parses `Self` into a byte representation that is ABI compliant.
/// It means that each type of the tuple have to implement `ToBytes`.
/// Implemented for tuples of various sizes.
pub trait ArgsParser {
    /// `parse` returns `Vec<Vec<u8>>` because we want to be able to
    /// discriminate between elements of the tuple and retain the order.
    fn parse(&self) -> Result<Vec<Vec<u8>>, Error>;
}

impl ArgsParser for () {
    fn parse(&self) -> Result<Vec<Vec<u8>>, Error> {
        Ok(Vec::new())
    }
}

macro_rules! impl_argsparser_tuple {
    ( $($name:ident)+) => (
        impl<$($name: Into<Value> + Clone),*> ArgsParser for ($($name,)*) {
            #[allow(non_snake_case)]
            fn parse(&self) -> Result<Vec<Vec<u8>>, Error> {
                let (ref $($name,)+) = self;
                // TODO: This has to take ownership of $name by cloning it as &T to &Value conversion is not possible. Changing the arguments of &self to self could solve problem of excess clones but requires a lot of public API changes.
                let values: &[Value] = &[$($name.clone().into(),)+];
                values.into_iter().map(ToBytes::to_bytes).collect()
            }
        }
    );
}

impl_argsparser_tuple! { T1 }
impl_argsparser_tuple! { T1 T2 }
impl_argsparser_tuple! { T1 T2 T3 }
impl_argsparser_tuple! { T1 T2 T3 T4 }
impl_argsparser_tuple! { T1 T2 T3 T4 T5 }
impl_argsparser_tuple! { T1 T2 T3 T4 T5 T6 }
impl_argsparser_tuple! { T1 T2 T3 T4 T5 T6 T7 }
impl_argsparser_tuple! { T1 T2 T3 T4 T5 T6 T7 T8 }
impl_argsparser_tuple! { T1 T2 T3 T4 T5 T6 T7 T8 T9 }
impl_argsparser_tuple! { T1 T2 T3 T4 T5 T6 T7 T8 T9 T10 }
impl_argsparser_tuple! { T1 T2 T3 T4 T5 T6 T7 T8 T9 T10 T11 }
impl_argsparser_tuple! { T1 T2 T3 T4 T5 T6 T7 T8 T9 T10 T11 T12 }
impl_argsparser_tuple! { T1 T2 T3 T4 T5 T6 T7 T8 T9 T10 T11 T12 T13 }
impl_argsparser_tuple! { T1 T2 T3 T4 T5 T6 T7 T8 T9 T10 T11 T12 T13 T14 }
impl_argsparser_tuple! { T1 T2 T3 T4 T5 T6 T7 T8 T9 T10 T11 T12 T13 T14 T15 }
impl_argsparser_tuple! { T1 T2 T3 T4 T5 T6 T7 T8 T9 T10 T11 T12 T13 T14 T15 T16 }
