use wasmi::{FromRuntimeValue, RuntimeArgs, Trap};

pub trait Args
where
    Self: Sized,
{
    fn parse(args: RuntimeArgs) -> Result<Self, Trap>;
}

/// This macro generates a code to extract nth arguments and create a tuple based on
/// that.
///
/// This idea is inspired by excellent example from https://github.com/dtolnay/case-studies/blob/master/integer-match/README.md
///
/// For a given set of tokens {T1, T2, T3, ...} it generates a code that enumerates them, and
/// constructs a tuple in place:
///
/// ```ignore
/// impl<T1: FromRuntimeValue + Sized, T2: FromRuntimeValue + Sized> Args for (T1, T2) {
///     fn parse(args: RuntimeArgs) -> Result<Self, Trap> {
///         #[allow(dead_code)]
///         mod m {
///             pub const X: usize = 0;
///             pub mod m {
///                 pub const X: usize = super::X + 1;
///                 pub mod m {
///                     pub const X: usize = super::X + 1;
///                 }
///             }
///         }
///         Ok((
///             args.nth_checked::<T1>(m::X)?,
///             args.nth_checked::<T2>(m::m::X)?,
///         ))
///     }
/// }
/// ```
/// First, it enumerates input tokens creating a nested structure of constants, and then enumerates
/// it again by constructing a paths, and generating a code for accessing nth values.

macro_rules! impl_args_for_tuple {
    ($($v:ident),*) => {
        impl<$($v:FromRuntimeValue + Sized,)*> Args for ($($v,)*) {
            fn parse(args: RuntimeArgs) -> Result<Self, Trap> {
                impl_args_for_tuple_helper! {
                    args
                    path: (m::X)
                    def: ()
                    arms: ()
                    $($v),*
                }
            }
        }
    };
}

macro_rules! impl_args_for_tuple_helper {
    (
        $args:ident
        path: ($($path:tt)*)
        def: ($($def:tt)*)
        arms: ($(($i:expr, $v:ident))*)
    ) => {
        #[allow(dead_code)]
        mod m {
            pub const X: usize = 0;
            $($def)*
        }
        Ok((
            $(
                $args.nth_checked::<$v>($i)?,
            )*
        ))
    };
    (
        $args:ident
        path: ($($path:tt)*)
        def: ($($def:tt)*)
        arms: ($(($i:expr, $v:ident))*)
        $next:ident $(, $rest:ident)*
    ) => {
        impl_args_for_tuple_helper! {
            $args
            path: (m::$($path)*)
            def: (pub mod m { pub const X: usize = super::X + 1; $($def)* })
            arms: ($(($i, $v))* ($($path)*, $next))
            $($rest),*
        }
    };
}

impl_args_for_tuple! {T1}
impl_args_for_tuple! {T1, T2}
impl_args_for_tuple! {T1, T2, T3}
impl_args_for_tuple! {T1, T2, T3, T4}
impl_args_for_tuple! {T1, T2, T3, T4, T5}
impl_args_for_tuple! {T1, T2, T3, T4, T5, T6}
