//! WNS types and functions for the SPL Token 2022 program.
//!
//! This module provides types and functions to interact with the WNS program
//! while there is no WNS crate available.
//!
//! TODO: This can be removed once the WNS crate is available and our programs are compatible with Solana v1.18.

use anchor_lang::{
    solana_program::{
        account_info::AccountInfo,
        instruction::{AccountMeta, Instruction},
        msg,
        program::invoke_signed,
        program_error::ProgramError,
        program_option::COption,
        pubkey::Pubkey,
        rent::Rent,
        sysvar::Sysvar,
    },
    Key, Result,
};
use anchor_spl::token_interface::spl_token_2022::{
    extension::{
        metadata_pointer::MetadataPointer, transfer_hook::TransferHook, BaseStateWithExtensions,
        StateWithExtensions,
    },
    state::Mint,
};
use spl_token_metadata_interface::state::TokenMetadata;
use std::str::FromStr;
use tensor_vipers::{unwrap_checked, unwrap_int};

use super::extension::{get_extension, get_variable_len_extension};

anchor_lang::declare_id!("wns1gDLt8fgLcGhWi5MqAqgXpwEP1JftKE9eZnXS1HM");

pub const ROYALTY_BASIS_POINTS_FIELD: &str = "royalty_basis_points";

pub const APPROVE_LEN: usize = 8 + 8;

/// WNS manager account.
const MANAGER_PUBKEY: Pubkey = Pubkey::new_from_array([
    125, 100, 129, 23, 165, 236, 2, 226, 233, 63, 107, 17, 242, 89, 72, 105, 75, 145, 77, 172, 118,
    210, 188, 66, 171, 78, 251, 66, 86, 35, 201, 190,
]);

/// Accounts for the `wns_approve` function.
pub struct ApproveAccounts<'info> {
    pub wns_program: AccountInfo<'info>,
    pub payer: AccountInfo<'info>,
    pub authority: AccountInfo<'info>,
    pub mint: AccountInfo<'info>,
    pub approve_account: AccountInfo<'info>,
    // Defaults to default pubkey, which is the System Program ID.
    pub payment_mint: Option<AccountInfo<'info>>,
    // Anchor Optional account--defaults to WNS program ID.
    pub distribution_token_account: Option<AccountInfo<'info>>,
    // Anchor Optional account--defaults to WNS program ID.
    pub authority_token_account: Option<AccountInfo<'info>>,
    pub distribution_account: AccountInfo<'info>,
    pub system_program: AccountInfo<'info>,
    pub distribution_program: AccountInfo<'info>,
    pub token_program: AccountInfo<'info>,
    pub payment_token_program: Option<AccountInfo<'info>>,
}

impl<'info> ApproveAccounts<'info> {
    pub fn to_account_infos(self) -> Vec<AccountInfo<'info>> {
        // Account Infos: order doesn't matter.
        let mut accounts = vec![
            self.wns_program,
            self.payer,
            self.authority,
            self.mint,
            self.approve_account,
            self.distribution_account,
            self.system_program,
            self.distribution_program,
            self.token_program,
        ];

        if let Some(distribution_token_account) = self.distribution_token_account {
            accounts.push(distribution_token_account);
        };

        if let Some(authority_token_account) = self.authority_token_account {
            accounts.push(authority_token_account);
        }

        if let Some(payment_mint) = self.payment_mint {
            accounts.push(payment_mint);
        }

        if let Some(payment_token_program) = self.payment_token_program {
            accounts.push(payment_token_program);
        }

        accounts
    }

    pub fn to_account_metas(&self) -> Vec<AccountMeta> {
        vec![
            AccountMeta::new(*self.payer.key, true),
            AccountMeta::new(*self.authority.key, true),
            AccountMeta::new_readonly(*self.mint.key, false),
            AccountMeta::new(*self.approve_account.key, false),
            AccountMeta::new_readonly(
                self.payment_mint
                    .as_ref()
                    .map_or(*self.system_program.key, |account| *account.key),
                false,
            ),
            // Anchor optional accounts, so either the token account or the WNS program.
            if let Some(distribution_token_account) = &self.distribution_token_account {
                AccountMeta::new(*distribution_token_account.key, false)
            } else {
                AccountMeta::new_readonly(*self.wns_program.key, false)
            },
            if let Some(authority_token_account) = &self.authority_token_account {
                AccountMeta::new(*authority_token_account.key, false)
            } else {
                AccountMeta::new_readonly(*self.wns_program.key, false)
            },
            AccountMeta::new(*self.distribution_account.key, false),
            AccountMeta::new_readonly(*self.system_program.key, false),
            AccountMeta::new_readonly(*self.distribution_program.key, false),
            AccountMeta::new_readonly(*self.token_program.key, false),
            if let Some(payment_token_program) = &self.payment_token_program {
                AccountMeta::new_readonly(*payment_token_program.key, false)
            } else {
                AccountMeta::new_readonly(*self.wns_program.key, false)
            },
        ]
    }
}

/// Validates a WNS Token 2022 non-fungible mint account.
///
/// For non-fungibles assets, the validation consists of checking that the mint:
/// - has no more than 1 supply
/// - has 0 decimals
/// - [TODO] has no mint authority (currently not possible to check)
/// - `ExtensionType::MetadataPointer` is present and points to the mint account
/// - `ExtensionType::TransferHook` is present and program id equals to WNS program
pub fn validate_mint(mint_info: &AccountInfo) -> Result<u16> {
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

    if mint.base.mint_authority.is_some()
        && mint.base.mint_authority != COption::Some(MANAGER_PUBKEY)
    {
        msg!("Mint authority must be none or the WNS manager account");
        return Err(ProgramError::InvalidAccountData.into());
    }

    if let Ok(extension) = get_extension::<MetadataPointer>(mint.get_tlv_data()) {
        let metadata_address: Option<Pubkey> = extension.metadata_address.into();
        if metadata_address != Some(mint_info.key()) {
            msg!("Metadata pointer extension: metadata address should be the mint itself");
            return Err(ProgramError::InvalidAccountData.into());
        }
    } else {
        msg!("Missing metadata pointer extension");
        return Err(ProgramError::InvalidAccountData.into());
    }

    if let Ok(extension) = get_extension::<TransferHook>(mint.get_tlv_data()) {
        let program_id: Option<Pubkey> = extension.program_id.into();
        if program_id != Some(super::wns::ID) {
            msg!("Transfer hook extension: program id mismatch");
            return Err(ProgramError::InvalidAccountData.into());
        }
    } else {
        msg!("Missing transfer hook extension");
        return Err(ProgramError::InvalidAccountData.into());
    }

    let metadata = get_variable_len_extension::<TokenMetadata>(mint.get_tlv_data())?;
    let royalty_basis_points = metadata
        .additional_metadata
        .iter()
        .find(|(key, _)| key == super::wns::ROYALTY_BASIS_POINTS_FIELD)
        .map(|(_, value)| value)
        .map(|value| u16::from_str(value).unwrap())
        .unwrap_or(0);

    Ok(royalty_basis_points)
}

/// Parameters for the `Approve` helper function.
pub struct ApproveParams<'a> {
    pub price: u64,
    pub royalty_fee: u64,
    pub signer_seeds: &'a [&'a [&'a [u8]]],
}

impl<'a> ApproveParams<'a> {
    /// Creates a new `ApproveParams` instance for no royalties for wallet signer.
    pub fn no_royalties() -> Self {
        Self {
            price: 0,
            royalty_fee: 0,
            signer_seeds: &[],
        }
    }
    /// Creates a new `ApproveParams` instance for no royalties for PDA signer.
    pub fn no_royalties_with_signer_seeds(signer_seeds: &'a [&'a [&'a [u8]]]) -> Self {
        Self {
            price: 0,
            royalty_fee: 0,
            signer_seeds,
        }
    }
}

/// Approves a WNS token transfer.
///
/// This needs to be called before any attempt to transfer a WNS token. For transfers
/// that do not involve royalties payment, set the `price` and `royalty_fee` to `0`.
///
/// The current implementation "manually" creates the instruction data and invokes the
/// WNS program. This is necessary because there is no WNS crate available.
pub fn approve(accounts: super::wns::ApproveAccounts, params: ApproveParams) -> Result<()> {
    let ApproveParams {
        price,
        royalty_fee,
        signer_seeds,
    } = params;

    // instruction data (the instruction was renamed to `ApproveTransfer`)
    let mut data = vec![198, 217, 247, 150, 208, 60, 169, 244];
    data.extend(price.to_le_bytes());

    let approve_ix = Instruction {
        program_id: super::wns::ID,
        accounts: accounts.to_account_metas(),
        data,
    };

    let payer = accounts.payer.clone();
    let approve = accounts.approve_account.clone();

    // store the previous values for the assert
    let initial_payer_lamports = payer.lamports();
    let initial_approve_rent = approve.lamports();

    // delegate the fee payment to WNS
    let result = invoke_signed(&approve_ix, &accounts.to_account_infos(), signer_seeds)
        .map_err(|error| error.into());

    let ending_payer_lamports = payer.lamports();

    // want to account for potential amount paid in rent.
    // in case WNS tries to drain to approve account, we cap
    // the rent difference to the minimum rent.
    let rent_difference = unwrap_int!(std::cmp::max(
        Rent::get()?.minimum_balance(APPROVE_LEN),
        initial_approve_rent,
    )
    .checked_sub(initial_approve_rent));
    // distribution account gets realloced based on creators potentially: overestimate here.
    let dist_realloc_fee = Rent::get()?.minimum_balance(1024);

    let payer_difference = unwrap_int!(initial_payer_lamports.checked_sub(ending_payer_lamports));
    let expected_fee = unwrap_checked!({
        royalty_fee
            .checked_add(rent_difference)?
            .checked_add(dist_realloc_fee)
    });

    // assert that payer was charged the expected fee: rent + any royalty fee.
    if payer_difference > expected_fee {
        msg!(
            "Unexpected lamports change: expected {} but got {}",
            expected_fee,
            payer_difference
        );
        return Err(ProgramError::InvalidAccountData.into());
    }

    result
}
