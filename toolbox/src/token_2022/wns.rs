//! WNS types and functions for the SPL Token 2022 program.
//!
//! This module provides types and functions to interact with the WNS program
//! while there is no WNS crate available.
//!
//! TODO: This can be removed once the WNS crate is available.

use anchor_lang::{
    solana_program::{
        account_info::AccountInfo,
        instruction::{AccountMeta, Instruction},
        msg,
        program::invoke,
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
    pub payer: AccountInfo<'info>,
    pub authority: AccountInfo<'info>,
    pub mint: AccountInfo<'info>,
    pub approve_account: AccountInfo<'info>,
    pub payment_mint: Option<AccountInfo<'info>>,
    pub distribution_address: AccountInfo<'info>,
    pub payer_address: AccountInfo<'info>,
    pub distribution: AccountInfo<'info>,
    pub system_program: AccountInfo<'info>,
    pub distribution_program: AccountInfo<'info>,
    pub wns_program: AccountInfo<'info>,
    pub token_program: AccountInfo<'info>,
    pub associated_token_program: AccountInfo<'info>,
}

impl<'info> ApproveAccounts<'info> {
    pub fn to_account_infos(self) -> Vec<AccountInfo<'info>> {
        let mut accounts = vec![
            self.payer,
            self.authority,
            self.mint,
            self.approve_account,
            self.distribution_address,
            self.payer_address,
            self.distribution,
            self.system_program,
            self.distribution_program,
            self.wns_program,
            self.token_program,
            self.associated_token_program,
        ];

        if let Some(payment_mint) = self.payment_mint {
            accounts.push(payment_mint);
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
            AccountMeta::new(*self.distribution_address.key, false),
            AccountMeta::new(*self.payer_address.key, false),
            AccountMeta::new(*self.distribution.key, false),
            AccountMeta::new_readonly(*self.system_program.key, false),
            AccountMeta::new_readonly(*self.distribution_program.key, false),
            AccountMeta::new_readonly(*self.token_program.key, false),
            AccountMeta::new_readonly(*self.associated_token_program.key, false),
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
pub fn wns_validate_mint(mint_info: &AccountInfo) -> Result<u16> {
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

/// Approves a WNS token transfer.
///
/// This needs to be called before any attempt to transfer a WNS token. For transfers
/// that do not involve royalties payment, set the `amount` to `0`.
///
/// The current implementation "manually" creates the instruction data and invokes the
/// WNS program. This is necessary because there is no WNS crate available.
pub fn wns_approve(
    accounts: super::wns::ApproveAccounts,
    amount: u64,
    expected_fee: u64,
) -> Result<()> {
    // instruction data
    let mut data = vec![69, 74, 217, 36, 115, 117, 97, 76];
    data.extend(amount.to_le_bytes());

    let approve_ix = Instruction {
        program_id: super::wns::ID,
        accounts: accounts.to_account_metas(),
        data,
    };

    let payer = accounts.payer_address.clone();
    let approve = accounts.approve_account.clone();
    // store the previous values for the assert
    let payer_lamports = payer.lamports();
    let approve_rent = approve.lamports();

    // delegate the fee payment to WNS
    let result = invoke(&approve_ix, &accounts.to_account_infos()).map_err(|error| error.into());

    // we take the max value between the minimum rent and the previous rent in case the previous
    // value is higher than the minimum rent
    let rent_difference =
        std::cmp::max(Rent::get()?.minimum_balance(APPROVE_LEN), approve_rent) - approve_rent;
    // assert that payer was charged the expected fee
    if (payer_lamports - payer.lamports()) > (expected_fee + rent_difference) {
        msg!(
            "Unexpected lamports change: expected {} but got {}",
            expected_fee + rent_difference,
            payer_lamports - payer.lamports()
        );
        return Err(ProgramError::InvalidAccountData.into());
    }

    result
}
