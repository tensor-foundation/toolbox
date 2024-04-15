use anchor_lang::prelude::*;

const DEFAULT_PUBKEY: Pubkey = Pubkey::new_from_array([0u8; 32]);

/// Used for Brosh types that can have a `None` value.
pub trait Nullable: AnchorSerialize + AnchorDeserialize {
    /// The value that represents `None`.
    const NONE: Self;

    /// Indicates if the value is `Some`.
    fn is_some(&self) -> bool;

    /// Indicates if the value is `None`.
    fn is_none(&self) -> bool;
}

/// A fixed-size Borsh type that can be used as an `Option<T>` without
/// requiring extra space to indicate if the value is `Some` or `None`.
///
/// This requires `T` to implement the `Nullable` trait so that it indicates the
/// value that is `None`.
#[repr(C)]
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, Eq, PartialEq, Hash)]
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

    #[inline]
    pub fn none() -> Self {
        Self(T::NONE)
    }
}

impl<T: Nullable> Default for NullableOption<T> {
    fn default() -> Self {
        Self(T::NONE)
    }
}

impl Nullable for Pubkey {
    const NONE: Self = DEFAULT_PUBKEY;

    fn is_some(&self) -> bool {
        self != &Self::NONE
    }

    fn is_none(&self) -> bool {
        self == &Self::NONE
    }
}

macro_rules! impl_nullable_for_ux {
    ($ux:ty) => {
        impl Nullable for $ux {
            const NONE: Self = 0;

            fn is_some(&self) -> bool {
                *self != Self::NONE
            }

            fn is_none(&self) -> bool {
                *self == Self::NONE
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nullable_option() {
        let mut none = NullableOption::<u8>::none();
        assert!(none.value().is_none());
        assert!(none.value_mut().is_none());

        let mut some = NullableOption::new(42u8);
        assert_eq!(some.value().unwrap(), &42);
        assert_eq!(some.value_mut().unwrap(), &mut 42);

        let opt = NullableOption::<Pubkey>::new(DEFAULT_PUBKEY);
        assert!(opt.value().is_none());

        let opt = NullableOption::<Pubkey>::new(Pubkey::new_from_array([1u8; 32]));
        assert!(opt.value().is_some());
        assert_eq!(opt.value().unwrap(), &Pubkey::new_from_array([1u8; 32]));
    }

    #[test]
    fn test_nullable_pubkey() {
        let none = Pubkey::NONE;
        assert!(none.is_none());
        assert!(!none.is_some());

        let some = Pubkey::new_from_array([1u8; 32]);
        assert!(!some.is_none());
        assert!(some.is_some());
    }

    #[test]
    fn test_nullable_ux() {
        let none = 0u8;
        assert!(none.is_none());
        assert!(!none.is_some());

        let some = 42u8;
        assert!(!some.is_none());
        assert!(some.is_some());
    }
}
