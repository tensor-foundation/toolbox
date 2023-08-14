use anchor_lang::prelude::*;

#[error_code]
pub enum CNftError {
    #[msg("bad royalties")]
    BadRoyaltiesPct = 9001,
    #[msg("insufficient balance")]
    InsufficientBalance = 9002,
    #[msg("creator mismatch")]
    CreatorMismatch = 9003,
    #[msg("failed leaf verification")]
    FailedLeafVerification = 9004,
    #[msg("arithmetic error")]
    ArithmeticError = 9005,
}
