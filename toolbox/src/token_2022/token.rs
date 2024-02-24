use anchor_lang::{
    context::CpiContext,
    error::ErrorCode,
    solana_program::{
        account_info::AccountInfo, msg, program_pack::IsInitialized, rent::Rent, sysvar::Sysvar,
    },
    system_program::create_account,
    Result,
};
use anchor_spl::{
    token_2022::InitializeAccount3,
    token_interface::{
        initialize_account3,
        spl_token_2022::{
            extension::{BaseStateWithExtensions, StateWithExtensions},
            state::{Account, Mint},
        },
    },
};

use super::extension::{get_extension_types, IExtensionType};

/// Struct that holds the accounts required for initializing a token account.
pub struct InitializeTokenAccount<'a, 'b, 'info> {
    pub token_info: &'a AccountInfo<'info>,
    pub mint: &'a AccountInfo<'info>,
    pub authority: &'a AccountInfo<'info>,
    pub payer: &'a AccountInfo<'info>,
    pub system_program: &'a AccountInfo<'info>,
    pub token_program: &'a AccountInfo<'info>,
    pub signer_seeds: &'a [&'b [u8]],
}

/// Initializes a token account without checking for the mint extensions.
///
/// This function is useful when the mint is known to have any extensions, which might not be
/// supported by the current version of the spl-token-2022 create. Trying to deserialize the
/// extension types will result in an error. For this reason, this function uses an "internal"
/// `IExtensionType` to safely calculate the length of the account.
///
/// When `allow_existing` is true, the function will not try to create the account; otherwise, it will
/// fail if the account already exists. This provides the same functionality as `init_if_needed`.
pub fn safe_initialize_token_account(
    input: InitializeTokenAccount<'_, '_, '_>,
    allow_existing: bool,
) -> Result<()> {
    let mint_data = &input.mint.data.borrow();
    let mint = StateWithExtensions::<Mint>::unpack(mint_data)?;
    // Get the token extensions required for the mint.
    let extensions = IExtensionType::get_required_init_account_extensions(&get_extension_types(
        mint.get_tlv_data(),
    )?);

    // Check if the token account is already initialized.
    if input.token_info.owner == &anchor_lang::solana_program::system_program::ID
        && input.token_info.lamports() == 0
    {
        // Determine the size of the account. We cannot deserialize the mint since it might
        // have extensions that are not currently on the version of the spl-token crate being used.

        let required_length = IExtensionType::try_calculate_account_len::<Account>(&extensions)?;

        let lamports = Rent::get()?.minimum_balance(required_length);
        let cpi_accounts = anchor_lang::system_program::CreateAccount {
            from: input.payer.clone(),
            to: input.token_info.clone(),
        };

        // Create the account.
        let cpi_context = CpiContext::new(input.system_program.clone(), cpi_accounts);
        create_account(
            cpi_context.with_signer(&[input.signer_seeds]),
            lamports,
            required_length as u64,
            input.token_program.key,
        )?;

        // Initialize the token account.
        let cpi_ctx = anchor_lang::context::CpiContext::new(
            input.token_program.clone(),
            InitializeAccount3 {
                account: input.token_info.clone(),
                mint: input.mint.clone(),
                authority: input.authority.clone(),
            },
        );
        initialize_account3(cpi_ctx)?;
    } else if allow_existing {
        // Validate that we got the expected token account.
        if input.token_info.owner != input.token_program.key {
            msg!("Invalid token account owner");
            return Err(ErrorCode::AccountOwnedByWrongProgram.into());
        }

        let token_data = &input.token_info.data.borrow();
        let token = StateWithExtensions::<Account>::unpack(token_data)?;

        if !token.base.is_initialized() {
            msg!("Token account is not initialized");
            return Err(ErrorCode::AccountNotInitialized.into());
        }

        if token.base.mint != *input.mint.key {
            msg!("Invalid mint on token account");
            return Err(ErrorCode::ConstraintTokenMint.into());
        }

        if token.base.owner != *input.authority.key {
            msg!("Invalid token owner");
            return Err(ErrorCode::ConstraintOwner.into());
        }
    } else {
        // the account already exists but `allow_existing` is false
        msg!("Token account already exists (reinitialization not allowed)");
        return Err(ErrorCode::ConstraintState.into());
    }

    Ok(())
}
