use anchor_lang::prelude::*;

#[error_code]
pub enum CNFTError {
    #[msg("bad royalties")]
    BadRoyaltiesPct,
    #[msg("insufficient balance")]
    InsufficientBalance,
    #[msg("creator mismatch")]
    CreatorMismatch,
}
