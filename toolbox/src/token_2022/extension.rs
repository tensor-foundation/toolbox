//! Helpers for working with the token extension TLV format.
//! Taken + adapted from https://github.com/solana-labs/solana-program-library/blob/2124f7562ed27a5f03f29c0ea0b77ef0167d5028/token/program-2022/src/extension/mod.rs
//!
//! The main purpose of these helpers is to graciously handle unknown extensions.

use anchor_lang::{
    solana_program::{
        borsh0_10::try_from_slice_unchecked, program_error::ProgramError, program_pack::Pack,
    },
    AnchorDeserialize,
};
use anchor_spl::{
    token::spl_token::state::{Account, Multisig},
    token_interface::spl_token_2022::extension::{
        AccountType, BaseState, Extension, ExtensionType, Length,
    },
};
use bytemuck::Pod;
use std::mem::size_of;

const BASE_ACCOUNT_LENGTH: usize = Account::LEN;

const BASE_ACCOUNT_AND_TYPE_LENGTH: usize = BASE_ACCOUNT_LENGTH + size_of::<AccountType>();

struct TlvIndices {
    pub type_start: usize,
    pub length_start: usize,
    pub value_start: usize,
}

/// Unpack a portion of the TLV data as the desired type
pub fn get_extension<V: Extension + Pod>(
    tlv_data: &[u8],
) -> core::result::Result<&V, ProgramError> {
    bytemuck::try_from_bytes::<V>(get_extension_bytes::<V>(tlv_data)?)
        .map_err(|_error| ProgramError::InvalidAccountData)
}

fn get_extension_bytes<V: Extension>(tlv_data: &[u8]) -> core::result::Result<&[u8], ProgramError> {
    let TlvIndices {
        type_start: _,
        length_start,
        value_start,
    } = get_extension_indices::<V>(tlv_data)?;
    // get_extension_indices has checked that tlv_data is long enough to include these indices
    let length = bytemuck::try_from_bytes::<Length>(&tlv_data[length_start..value_start])
        .map_err(|_error| ProgramError::InvalidAccountData)?;
    let value_end = value_start.saturating_add(usize::from(*length));
    if tlv_data.len() < value_end {
        return Err(ProgramError::InvalidAccountData);
    }
    Ok(&tlv_data[value_start..value_end])
}

fn get_extension_indices<V: Extension>(
    tlv_data: &[u8],
) -> core::result::Result<TlvIndices, ProgramError> {
    let mut start_index = 0;
    while start_index < tlv_data.len() {
        let tlv_indices = get_tlv_indices(start_index);
        if tlv_data.len() < tlv_indices.value_start {
            return Err(ProgramError::InvalidAccountData);
        }
        let extension_type =
            ExtensionType::try_from(&tlv_data[tlv_indices.type_start..tlv_indices.length_start]);
        // [FEBO] Make sure we don't bubble the error in case we don't recognize
        // the extension type; the best we can do when we don't recognize the extension is
        // to keep looking for the one we're interested in
        if extension_type.is_ok() && extension_type.unwrap() == V::TYPE {
            // found an instance of the extension that we're looking, return!
            return Ok(tlv_indices);
        }
        let length = bytemuck::try_from_bytes::<Length>(
            &tlv_data[tlv_indices.length_start..tlv_indices.value_start],
        )
        .map_err(|_| ProgramError::InvalidArgument)?;
        let value_end_index = tlv_indices.value_start.saturating_add(usize::from(*length));
        start_index = value_end_index;
    }
    Err(ProgramError::InvalidAccountData)
}

/// Helper function to get the current TlvIndices from the current spot
fn get_tlv_indices(type_start: usize) -> TlvIndices {
    let length_start = type_start.saturating_add(size_of::<ExtensionType>());
    let value_start = length_start.saturating_add(size_of::<Length>());
    TlvIndices {
        type_start,
        length_start,
        value_start,
    }
}

/// Fetches the "known" extension types from the TLV data.
pub fn get_extension_types(tlv_data: &[u8]) -> Result<Vec<IExtensionType>, ProgramError> {
    let mut extension_types = vec![];
    let mut start_index = 0;
    while start_index < tlv_data.len() {
        let tlv_indices = get_tlv_indices(start_index);
        if tlv_data.len() < tlv_indices.length_start {
            // There aren't enough bytes to store the next type, which means we
            // got to the end. The last byte could be used during a realloc!
            return Ok(extension_types);
        }
        let extension_type = u16::from_le_bytes(
            (&tlv_data[tlv_indices.type_start..tlv_indices.length_start])
                .try_into()
                .map_err(|_| ProgramError::InvalidAccountData)?,
        );
        // we recognize the extension type, add it to the list
        if let Ok(extension_type) = IExtensionType::try_from(extension_type) {
            extension_types.push(extension_type);
        }

        if tlv_data.len() < tlv_indices.value_start {
            // not enough bytes to store the length, malformed
            return Err(ProgramError::InvalidAccountData);
        }

        let length = bytemuck::try_from_bytes::<Length>(
            &tlv_data[tlv_indices.length_start..tlv_indices.value_start],
        )
        .map_err(|_| ProgramError::InvalidAccountData)?;

        let value_end_index = tlv_indices.value_start.saturating_add(usize::from(*length));
        if value_end_index > tlv_data.len() {
            // value blows past the size of the slice, malformed
            return Err(ProgramError::InvalidAccountData);
        }
        start_index = value_end_index;
    }
    Ok(extension_types)
}

pub fn get_variable_len_extension<V: Extension + AnchorDeserialize>(
    tlv_data: &[u8],
) -> core::result::Result<V, ProgramError> {
    let data = get_extension_bytes::<V>(tlv_data)?;
    try_from_slice_unchecked::<V>(data).map_err(|_error| ProgramError::InvalidAccountData)
}

#[repr(u16)]
#[derive(Debug, PartialEq)]
pub enum IExtensionType {
    /// [MINT] Includes an optional mint close authority
    MintCloseAuthority = 3,
    /// [MINT] Specifies the default Account::state for new Accounts
    DefaultAccountState = 6,
    /// [ACCOUNT] Indicates that the Account owner authority cannot be changed
    ImmutableOwner = 7,
    /// [MINT] Indicates that the tokens from this mint can't be transfered
    NonTransferable = 9,
    /// [ACCOUNT] Locks privileged token operations from happening via CPI
    CpiGuard = 11,
    /// [MINT] Includes an optional permanent delegate
    PermanentDelegate = 12,
    /// [ACCOUNT] Indicates that the tokens in this account belong to a non-transferable
    /// mint
    NonTransferableAccount = 13,
    /// [MINT] Mint requires a CPI to a program implementing the "transfer hook"
    /// interface
    TransferHook = 14,
    /// [ACCOUNT] Indicates that the tokens in this account belong to a mint with a
    /// transfer hook
    TransferHookAccount = 15,
    /// [MINT] Mint contains a pointer to another account (or the same account) that
    /// holds metadata
    MetadataPointer = 18,
    /// [MINT] Mint contains a pointer to another account (or the same account) that
    /// holds group configurations
    GroupPointer = 20,
    /// [MINT] Mint contains token group configurations
    TokenGroup = 21,
    /// [MINT] Mint contains a pointer to another account (or the same account) that
    /// holds group member configurations
    GroupMemberPointer = 22,
    /// [MINT] Mint contains token group member configurations
    TokenGroupMember = 23,
}

impl IExtensionType {
    fn get_type_len(&self) -> usize {
        match self {
            IExtensionType::MintCloseAuthority => 32,
            IExtensionType::DefaultAccountState => 1,
            IExtensionType::ImmutableOwner => 0,
            IExtensionType::NonTransferable => 0,
            IExtensionType::CpiGuard => 1,
            IExtensionType::PermanentDelegate => 32,
            IExtensionType::NonTransferableAccount => 0,
            IExtensionType::TransferHook => 64,
            IExtensionType::TransferHookAccount => 1,
            IExtensionType::MetadataPointer => 64,
            IExtensionType::GroupPointer => 64,
            IExtensionType::TokenGroup => 72,
            IExtensionType::GroupMemberPointer => 64,
            IExtensionType::TokenGroupMember => 68,
        }
    }

    /// Get the TLV length for an ExtensionType
    ///
    /// Fails if the extension type has a variable length
    fn try_get_tlv_len(&self) -> Result<usize, ProgramError> {
        Ok(add_type_and_length_to_len(self.get_type_len()))
    }

    /// Get the TLV length for a set of ExtensionTypes
    ///
    /// Fails if any of the extension types has a variable length
    fn try_get_total_tlv_len(extension_types: &[Self]) -> Result<usize, ProgramError> {
        // dedupe extensions
        let mut extensions = vec![];
        for extension_type in extension_types {
            if !extensions.contains(&extension_type) {
                extensions.push(extension_type);
            }
        }
        extensions.iter().map(|e| e.try_get_tlv_len()).sum()
    }

    /// Get the required account data length for the given ExtensionTypes
    ///
    /// Fails if any of the extension types has a variable length
    pub fn try_calculate_account_len<S: BaseState>(
        extension_types: &[Self],
    ) -> Result<usize, ProgramError> {
        if extension_types.is_empty() {
            Ok(S::LEN)
        } else {
            let extension_size = Self::try_get_total_tlv_len(extension_types)?;
            let total_len = extension_size.saturating_add(BASE_ACCOUNT_AND_TYPE_LENGTH);
            Ok(adjust_len_for_multisig(total_len))
        }
    }

    /// Based on a set of [MINT] ExtensionTypes, get the list of
    /// [ACCOUNT] ExtensionTypes required on InitializeAccount
    pub fn get_required_init_account_extensions(mint_extension_types: &[Self]) -> Vec<Self> {
        let mut account_extension_types = vec![];
        for extension_type in mint_extension_types {
            match extension_type {
                IExtensionType::NonTransferable => {
                    account_extension_types.push(IExtensionType::NonTransferableAccount);
                }
                IExtensionType::TransferHook => {
                    account_extension_types.push(IExtensionType::TransferHookAccount);
                }
                _ => {}
            }
        }
        account_extension_types
    }
}

impl TryFrom<u16> for IExtensionType {
    type Error = ProgramError;
    fn try_from(value: u16) -> Result<Self, Self::Error> {
        let extension = match value {
            3 => IExtensionType::MintCloseAuthority,
            6 => IExtensionType::DefaultAccountState,
            7 => IExtensionType::ImmutableOwner,
            9 => IExtensionType::NonTransferable,
            11 => IExtensionType::CpiGuard,
            12 => IExtensionType::PermanentDelegate,
            13 => IExtensionType::NonTransferableAccount,
            14 => IExtensionType::TransferHook,
            15 => IExtensionType::TransferHookAccount,
            18 => IExtensionType::MetadataPointer,
            20 => IExtensionType::GroupPointer,
            21 => IExtensionType::TokenGroup,
            22 => IExtensionType::GroupMemberPointer,
            23 => IExtensionType::TokenGroupMember,
            _ => return Err(ProgramError::InvalidArgument),
        };
        Ok(extension)
    }
}

/// Helper function to calculate exactly how many bytes a value will take up,
/// given the value's length
const fn add_type_and_length_to_len(value_len: usize) -> usize {
    value_len
        .saturating_add(size_of::<ExtensionType>())
        .saturating_add(size_of::<Length>())
}

const fn adjust_len_for_multisig(account_len: usize) -> usize {
    if account_len == Multisig::LEN {
        account_len.saturating_add(size_of::<ExtensionType>())
    } else {
        account_len
    }
}
