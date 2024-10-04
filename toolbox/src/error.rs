use anchor_lang::prelude::*;

#[error_code]
pub enum TensorError {
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
    #[msg("metadata account does not match")]
    BadMetadata = 9006,
    #[msg("rule set for programmable nft does not match")]
    BadRuleSet = 9007,
    #[msg("invalid core asset")]
    InvalidCoreAsset = 9008,
    #[msg("invalid fee account")]
    InvalidFeeAccount = 9009,
}
