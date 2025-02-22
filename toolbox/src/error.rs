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

    #[msg("invalid whitelist")]
    InvalidWhitelist = 9010,

    #[msg("invalid program owner")]
    InvalidProgramOwner = 9011,

    #[msg("invalid edition")]
    InvalidEdition = 9012,

    #[msg("invalid mint")]
    InvalidMint = 9013,

    #[msg("invalid owner")]
    InvalidOwner = 9014,
}
