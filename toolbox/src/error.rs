use anchor_lang::prelude::*;

#[error_code(offset = 9001)]
pub enum TensorError {
    #[msg("bad royalties")]
    BadRoyaltiesPct,
    #[msg("insufficient balance")]
    InsufficientBalance,
    #[msg("creator mismatch")]
    CreatorMismatch,
    #[msg("failed leaf verification")]
    FailedLeafVerification,
    #[msg("arithmetic error")]
    ArithmeticError,
    #[msg("metadata account does not match")]
    BadMetadata,
    #[msg("rule set for programmable nft does not match")]
    BadRuleSet,
    #[msg("invalid core asset")]
    InvalidCoreAsset,
    #[msg("invalid fee account")]
    InvalidFeeAccount,
}
