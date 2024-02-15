pub mod extension;
pub mod token;
pub mod transfer;
pub mod wns;

use anchor_lang::{
    solana_program::{account_info::AccountInfo, msg, program_error::ProgramError, pubkey::Pubkey},
    Result,
};
use anchor_spl::token_interface::spl_token_2022::{
    extension::{metadata_pointer::MetadataPointer, BaseStateWithExtensions, StateWithExtensions},
    state::Mint,
};

use self::extension::get_extension;

/// Validates a "vanilla" Token 2022 non-fungible mint account.
///
/// For non-fungibles assets, the validation consists of checking that the mint:
/// - has no more than 1 supply
/// - has 0 decimals
/// - has no mint authority
/// - `ExtensionType::MetadataPointer` is present and points to the mint account
pub fn t22_validate_mint(mint_info: &AccountInfo) -> Result<()> {
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

    if let Ok(extension) = get_extension::<MetadataPointer>(mint.get_tlv_data()) {
        let metadata_address: Option<Pubkey> = extension.metadata_address.into();
        if metadata_address != Some(*mint_info.key) {
            msg!("Metadata pointer extension: metadata address should be the mint itself");
            return Err(ProgramError::InvalidAccountData.into());
        }
    } else {
        msg!("Missing metadata pointer extension");
        return Err(ProgramError::InvalidAccountData.into());
    }

    Ok(())
}
