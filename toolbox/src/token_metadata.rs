#![allow(clippy::result_large_err)]

use anchor_lang::error::ErrorCode;
use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{self, Mint, TokenAccount, TokenInterface, TransferChecked},
};
use mpl_token_metadata::{
    accounts::Metadata,
    instructions::{DelegateTransferV1CpiBuilder, TransferV1CpiBuilder},
    types::{AuthorizationData, TokenStandard},
};
use tensor_vipers::{throw_err, unwrap_opt};

use crate::TensorError;

pub use mpl_token_metadata::ID;

#[inline(never)]
pub fn assert_decode_metadata(mint: &Pubkey, metadata: &AccountInfo) -> Result<Metadata> {
    if *metadata.owner != mpl_token_metadata::ID {
        throw_err!(TensorError::BadMetadata);
    }

    // We must use `safe_deserialize` since there are variations on the metadata struct
    // which are not compatible with borsh's default deserialization. Using `try_from` will
    // fail when there are missing fields.
    let metadata = Metadata::safe_deserialize(&metadata.try_borrow_data()?)
        .map_err(|_error| TensorError::BadMetadata)?;

    if metadata.mint != *mint {
        throw_err!(TensorError::BadMetadata);
    }

    Ok(metadata)
}

/// Transfer Args using AccountInfo types to be more generic.
pub struct TransferArgsAi<'a, 'info> {
    /// Account that will pay for any associated fees.
    pub payer: &'a AccountInfo<'info>,

    /// Account that will transfer the token.
    pub source: &'a AccountInfo<'info>,

    /// Associated token account of the source.
    pub source_ata: &'a AccountInfo<'info>,

    /// Token record of the source.
    pub source_token_record: Option<&'a AccountInfo<'info>>,

    /// Account that will receive the token.
    pub destination: &'a AccountInfo<'info>,

    /// Associated token account of the destination.
    pub destination_ata: &'a AccountInfo<'info>,

    /// Token record of the destination.
    pub destination_token_record: Option<&'a AccountInfo<'info>>,

    /// Mint of the token.
    pub mint: &'a AccountInfo<'info>,

    /// Metadata of the token.
    pub metadata: &'a AccountInfo<'info>,

    /// Edition of the token.
    pub edition: &'a AccountInfo<'info>,

    /// System program account.
    pub system_program: &'a AccountInfo<'info>,

    /// SPL Token program account.
    pub spl_token_program: &'a AccountInfo<'info>,

    /// SPL ATA program account.
    pub spl_ata_program: &'a AccountInfo<'info>,

    /// Sysvar instructions account.
    pub sysvar_instructions: Option<&'a AccountInfo<'info>>,

    /// Token Metadata program account.
    pub token_metadata_program: Option<&'a AccountInfo<'info>>,

    /// Authorization rules program account.
    pub authorization_rules_program: Option<&'a AccountInfo<'info>>,

    /// Authorization rules account.
    pub authorization_rules: Option<&'a AccountInfo<'info>>,

    /// Authorization data.
    pub authorization_data: Option<AuthorizationData>,

    /// Delegate to use in the transfer.
    ///
    /// If passed, we assign a delegate first, and the call invoke_signed() instead of invoke().
    pub delegate: Option<&'a AccountInfo<'info>>,
}

/// Transfer Args using Anchor types to be more ergonomic.
pub struct TransferArgs<'a, 'info> {
    /// Account that will pay for any associated fees.
    pub payer: &'a AccountInfo<'info>,

    /// Account that will transfer the token.
    pub source: &'a AccountInfo<'info>,

    /// Associated token account of the source.
    pub source_ata: &'a InterfaceAccount<'info, TokenAccount>,

    /// Token record of the source.
    pub source_token_record: Option<&'a UncheckedAccount<'info>>,

    /// Account that will receive the token.
    pub destination: &'a AccountInfo<'info>,

    /// Associated token account of the destination.
    pub destination_ata: &'a InterfaceAccount<'info, TokenAccount>,

    /// Token record of the destination.
    pub destination_token_record: Option<&'a UncheckedAccount<'info>>,

    /// Mint of the token.
    pub mint: &'a InterfaceAccount<'info, Mint>,

    /// Metadata of the token.
    pub metadata: &'a UncheckedAccount<'info>,

    /// Edition of the token.
    pub edition: &'a UncheckedAccount<'info>,

    /// System program account.
    pub system_program: &'a Program<'info, System>,

    /// SPL Token program account.
    pub spl_token_program: &'a Interface<'info, TokenInterface>,

    /// SPL ATA program account.
    pub spl_ata_program: &'a Program<'info, AssociatedToken>,

    /// Sysvar instructions account.
    pub sysvar_instructions: Option<&'a UncheckedAccount<'info>>,

    /// Token Metadata program account.
    pub token_metadata_program: Option<&'a UncheckedAccount<'info>>,

    /// Authorization rules program account.
    pub authorization_rules_program: Option<&'a UncheckedAccount<'info>>,

    /// Authorization rules account.
    pub authorization_rules: Option<&'a UncheckedAccount<'info>>,

    /// Authorization data.
    pub authorization_data: Option<AuthorizationData>,

    /// Delegate to use in the transfer.
    ///
    /// If passed, we assign a delegate first, and the call invoke_signed() instead of invoke().
    pub delegate: Option<&'a AccountInfo<'info>>,
}

fn cpi_transfer_ai(args: TransferArgsAi, signer_seeds: Option<&[&[&[u8]]]>) -> Result<()> {
    let token_metadata_program =
        unwrap_opt!(args.token_metadata_program, ErrorCode::AccountNotEnoughKeys);
    let sysvar_instructions =
        unwrap_opt!(args.sysvar_instructions, ErrorCode::AccountNotEnoughKeys);

    // prepares the CPI instruction
    let mut transfer_cpi = TransferV1CpiBuilder::new(token_metadata_program);
    transfer_cpi
        .authority(args.source)
        .token_owner(args.source)
        .token(args.source_ata.as_ref())
        .destination_owner(args.destination)
        .destination_token(args.destination_ata.as_ref())
        .mint(args.mint.as_ref())
        .metadata(args.metadata.as_ref())
        .edition(Some(args.edition))
        .payer(args.payer)
        .spl_ata_program(args.spl_ata_program)
        .spl_token_program(args.spl_token_program)
        .system_program(args.system_program)
        .sysvar_instructions(sysvar_instructions)
        .token_record(args.source_token_record)
        .destination_token_record(args.destination_token_record)
        .authorization_rules_program(args.authorization_rules_program)
        .authorization_rules(args.authorization_rules)
        .amount(1);

    // set the authorization data if passed in
    args.authorization_data
        .clone()
        .map(|data| transfer_cpi.authorization_data(data));

    // invoke delegate if necessary
    if let Some(delegate) = args.delegate {
        // replace authority on the builder with the newly assigned delegate
        transfer_cpi.authority(delegate);

        let mut delegate_cpi = DelegateTransferV1CpiBuilder::new(token_metadata_program);
        delegate_cpi
            .authority(args.source)
            .delegate(delegate)
            .token(args.source_ata.as_ref())
            .mint(args.mint.as_ref())
            .metadata(args.metadata)
            .master_edition(Some(args.edition))
            .payer(args.payer)
            .spl_token_program(Some(args.spl_token_program))
            .token_record(args.source_token_record)
            .authorization_rules(args.authorization_rules)
            .authorization_rules_program(args.authorization_rules_program)
            .amount(1);

        args.authorization_data
            .map(|data| delegate_cpi.authorization_data(data));

        delegate_cpi.invoke()?;
    }

    if let Some(signer_seeds) = signer_seeds {
        transfer_cpi.invoke_signed(signer_seeds)?;
    } else {
        transfer_cpi.invoke()?;
    }

    Ok(())
}

fn cpi_transfer(args: TransferArgs, signer_seeds: Option<&[&[&[u8]]]>) -> Result<()> {
    let token_metadata_program =
        unwrap_opt!(args.token_metadata_program, ErrorCode::AccountNotEnoughKeys);
    let sysvar_instructions =
        unwrap_opt!(args.sysvar_instructions, ErrorCode::AccountNotEnoughKeys);

    // prepares the CPI instruction
    let mut transfer_cpi = TransferV1CpiBuilder::new(token_metadata_program);
    transfer_cpi
        .authority(args.source)
        .token_owner(args.source)
        .token(args.source_ata.as_ref())
        .destination_owner(args.destination)
        .destination_token(args.destination_ata.as_ref())
        .mint(args.mint.as_ref())
        .metadata(args.metadata.as_ref())
        .edition(Some(args.edition))
        .payer(args.payer)
        .spl_ata_program(args.spl_ata_program)
        .spl_token_program(args.spl_token_program)
        .system_program(args.system_program)
        .sysvar_instructions(sysvar_instructions)
        .token_record(args.source_token_record.map(|account| account.as_ref()))
        .destination_token_record(
            args.destination_token_record
                .map(|account| account.as_ref()),
        )
        .authorization_rules_program(
            args.authorization_rules_program
                .map(|account| account.as_ref()),
        )
        .authorization_rules(args.authorization_rules.map(|account| account.as_ref()))
        .amount(1);

    // set the authorization data if passed in
    args.authorization_data
        .clone()
        .map(|data| transfer_cpi.authorization_data(data));

    // invoke delegate if necessary
    if let Some(delegate) = args.delegate {
        // replace authority on the builder with the newly assigned delegate
        transfer_cpi.authority(delegate);

        let mut delegate_cpi = DelegateTransferV1CpiBuilder::new(token_metadata_program);
        delegate_cpi
            .authority(args.source)
            .delegate(delegate)
            .token(args.source_ata.as_ref())
            .mint(args.mint.as_ref())
            .metadata(args.metadata)
            .master_edition(Some(args.edition))
            .payer(args.payer)
            .spl_token_program(Some(args.spl_token_program))
            .token_record(args.source_token_record.map(|account| account.as_ref()))
            .authorization_rules(args.authorization_rules.map(|account| account.as_ref()))
            .authorization_rules_program(
                args.authorization_rules_program
                    .map(|account| account.as_ref()),
            )
            .amount(1);

        args.authorization_data
            .map(|data| delegate_cpi.authorization_data(data));

        delegate_cpi.invoke()?;
    }

    if let Some(signer_seeds) = signer_seeds {
        transfer_cpi.invoke_signed(signer_seeds)?;
    } else {
        transfer_cpi.invoke()?;
    }

    Ok(())
}

/// Transfer a NFT or PNFT using AccountInfos rather than Anchor types.
pub fn transfer_with_ai(
    args: TransferArgsAi,
    //if passed, use signed_invoke() instead of invoke()
    signer_seeds: Option<&[&[&[u8]]]>,
) -> Result<()> {
    let metadata = assert_decode_metadata(&args.mint.key(), args.metadata)?;

    if matches!(
        metadata.token_standard,
        Some(TokenStandard::ProgrammableNonFungible)
            | Some(TokenStandard::ProgrammableNonFungibleEdition)
    ) {
        // pnft transfer
        return cpi_transfer_ai(args, signer_seeds);
    }

    // non-pnft / no token std, normal transfer

    let ctx = CpiContext::new(
        args.spl_token_program.to_account_info(),
        TransferChecked {
            from: args.source_ata.to_account_info(),
            to: args.destination_ata.to_account_info(),
            authority: args.source.to_account_info(),
            mint: args.mint.to_account_info(),
        },
    );

    if let Some(signer_seeds) = signer_seeds {
        token_interface::transfer_checked(ctx.with_signer(signer_seeds), 1, 0)
    } else {
        token_interface::transfer_checked(ctx, 1, 0)
    }
}

/// Transfer a NFT or PNFT.
pub fn transfer(
    args: TransferArgs,
    //if passed, use signed_invoke() instead of invoke()
    signer_seeds: Option<&[&[&[u8]]]>,
) -> Result<()> {
    let metadata = assert_decode_metadata(&args.mint.key(), args.metadata)?;

    if matches!(
        metadata.token_standard,
        Some(TokenStandard::ProgrammableNonFungible)
            | Some(TokenStandard::ProgrammableNonFungibleEdition)
    ) {
        // pnft transfer
        return cpi_transfer(args, signer_seeds);
    }

    // non-pnft / no token std, normal transfer

    let ctx = CpiContext::new(
        args.spl_token_program.to_account_info(),
        TransferChecked {
            from: args.source_ata.to_account_info(),
            to: args.destination_ata.to_account_info(),
            authority: args.source.to_account_info(),
            mint: args.mint.to_account_info(),
        },
    );

    if let Some(signer_seeds) = signer_seeds {
        token_interface::transfer_checked(ctx.with_signer(signer_seeds), 1, 0)
    } else {
        token_interface::transfer_checked(ctx, 1, 0)
    }
}
