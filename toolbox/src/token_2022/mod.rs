pub mod extension;
pub mod token;
pub mod transfer;
pub mod wns;

use anchor_lang::{
    solana_program::{account_info::AccountInfo, msg, program_error::ProgramError, pubkey::Pubkey},
    Result,
};
use anchor_spl::{
    token_2022::spl_token_2022::extension::transfer_hook::TransferHook,
    token_interface::spl_token_2022::{
        extension::{BaseStateWithExtensions, StateWithExtensions},
        state::Mint,
    },
};
use spl_token_metadata_interface::state::TokenMetadata;
use std::str::FromStr;

use self::extension::{get_extension, get_variable_len_extension};

// Prefix used by Libreplex to identify royalty accounts.
const LIBREPLEX_RO: &str = "_ro_";

// Libreplex transfer hook program id: CZ1rQoAHSqWBoAEfqGsiLhgbM59dDrCWk3rnG5FXaoRV.
const LIBREPLEX_TRANSFER_HOOK: Pubkey = Pubkey::new_from_array([
    171, 164, 26, 246, 200, 121, 33, 135, 216, 50, 55, 114, 165, 1, 182, 24, 180, 164, 102, 111, 3,
    53, 2, 250, 50, 121, 61, 15, 194, 104, 5, 76,
]);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoyaltyInfo {
    /// Royalties fee basis points.
    pub seller_fee: u16,

    /// List of creators (pubkey, share).
    pub creators: Vec<(Pubkey, u8)>,
}

/// Validates a "vanilla" Token 2022 non-fungible mint account.
///
/// For non-fungibles assets, the validation consists of checking that the mint:
/// - has no more than 1 supply
/// - has 0 decimals
/// - has no mint authority
///
/// It also supports Libreplex royalty enforcement by looking for the metadata extension
/// to retrieve the seller fee basis points and creators.
pub fn validate_mint(mint_info: &AccountInfo) -> Result<Option<RoyaltyInfo>> {
    let mint_data = &mint_info.data.borrow();
    let mint = StateWithExtensions::<Mint>::unpack(mint_data)?;

    if !mint.base.is_initialized {
        msg!("Mint is not initialized");
        return Err(ProgramError::UninitializedAccount.into());
    }

    if mint.base.decimals != 0 {
        msg!("Mint decimals must be 0");
        return Err(ProgramError::InvalidAccountData.into());
    }

    if mint.base.supply != 1 {
        msg!("Mint supply must be 1");
        return Err(ProgramError::InvalidAccountData.into());
    }

    if mint.base.mint_authority.is_some() {
        msg!("Mint authority must be none");
        return Err(ProgramError::InvalidAccountData.into());
    }

    let hook_program: Option<Pubkey> =
        if let Ok(extension) = get_extension::<TransferHook>(mint.get_tlv_data()) {
            extension.program_id.into()
        } else {
            None
        };

    // currently only Libreplex is supported, but this can be expanded to include
    // other standards in the future; only looks for the metadata if the correct
    // hook is in place to avoid parsing the metadata unnecessarily (or match on a
    // value that it is not intended to be used)
    if hook_program == Some(LIBREPLEX_TRANSFER_HOOK) {
        if let Ok(metadata) = get_variable_len_extension::<TokenMetadata>(mint.get_tlv_data()) {
            let royalties = metadata
                .additional_metadata
                .iter()
                .find(|(key, _)| key.starts_with(LIBREPLEX_RO));

            if let Some((destination, seller_fee)) = royalties {
                let seller_fee: u16 = seller_fee.parse().map_err(|_error| {
                    msg!("[ERROR] Could not parse seller fee");
                    ProgramError::InvalidAccountData
                })?;

                if seller_fee > 10000 {
                    msg!("[ERROR] Seller fee must be less than or equal to 10000");
                    return Err(ProgramError::InvalidAccountData.into());
                }

                let destination = Pubkey::from_str(destination.trim_start_matches(LIBREPLEX_RO))
                    .map_err(|_error| {
                        msg!("[ERROR] Could not parse destination address");
                        ProgramError::InvalidAccountData
                    })?;

                return Ok(Some(RoyaltyInfo {
                    seller_fee,
                    creators: vec![(destination, 100)],
                }));
            }
        }
    }

    Ok(None)
}
