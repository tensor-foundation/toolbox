use anchor_lang::prelude::*;

/// Used for Brosh types that can have a `None` value.
pub trait Nullable: AnchorSerialize + AnchorDeserialize {
    /// Indicates if the value is `Some`.
    fn is_some(&self) -> bool;

    /// Indicates if the value is `None`.
    fn is_none(&self) -> bool;
}

/// A fixed-size Borsh type that can be used as an `Option<T>` without
/// requiring extra space to indicate if the value is `Some` or `None`.
///
/// This can be used when a specific value of `T` indicates that its
/// value is `None`.
#[repr(C)]
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub struct NullableOption<T: Nullable>(T);

impl<T: Nullable> NullableOption<T> {
    #[inline]
    pub fn new(value: T) -> Self {
        Self(value)
    }

    #[inline]
    pub fn value(&self) -> Option<&T> {
        if self.0.is_some() {
            Some(&self.0)
        } else {
            None
        }
    }

    #[inline]
    pub fn value_mut(&mut self) -> Option<&mut T> {
        if self.0.is_some() {
            Some(&mut self.0)
        } else {
            None
        }
    }
}

impl Nullable for Pubkey {
    fn is_some(&self) -> bool {
        self != &Pubkey::default()
    }

    fn is_none(&self) -> bool {
        self == &Pubkey::default()
    }
}

macro_rules! impl_nullable_for_ux {
    ($ux:ty) => {
        impl Nullable for $ux {
            fn is_some(&self) -> bool {
                *self != 0
            }

            fn is_none(&self) -> bool {
                *self == 0
            }
        }
    };
}

impl_nullable_for_ux!(u8);
impl_nullable_for_ux!(u16);
impl_nullable_for_ux!(u32);
impl_nullable_for_ux!(u64);
impl_nullable_for_ux!(u128);
impl_nullable_for_ux!(usize);
