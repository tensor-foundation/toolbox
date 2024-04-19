use anchor_lang::prelude::*;

/// Three-state operation that allows the ability to set, clear or do nothing with a value.
/// Useful to use in lieu of an Option when the None variant could be ambiguous
/// about whether to clear or do nothing.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Operation<T> {
    None,
    Clear,
    Set(T),
}

impl<T> Operation<T> {
    pub fn is_none(&self) -> bool {
        matches!(self, Operation::None)
    }

    pub fn is_clear(&self) -> bool {
        matches!(self, Operation::Clear)
    }

    pub fn is_set(&self) -> bool {
        matches!(self, Operation::Set(_))
    }

    pub fn value(&self) -> Option<&T> {
        match self {
            Operation::Set(value) => Some(value),
            _ => None,
        }
    }

    pub fn into_value(self) -> Option<T> {
        match self {
            Operation::Set(value) => Some(value),
            _ => None,
        }
    }
}
