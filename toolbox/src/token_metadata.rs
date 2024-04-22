#![allow(clippy::result_large_err)]

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
use vipers::throw_err;

use crate::TensorError;

#[inline(never)]
pub fn assert_decode_metadata(mint: &Pubkey, metadata: &AccountInfo) -> Result<Metadata> {
    // Check account owner (redundant because of find_program_address above, but why not).
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

pub struct TransferArgs<'a, 'info> {
    // (!) payer can't carry data, has to be a normal KP:
    // https://github.com/solana-labs/solana/blob/bda0c606a19ce1cc44b5ab638ff0b993f612e76c/runtime/src/system_instruction_processor.rs#L197
    pub payer: &'a AccountInfo<'info>,
    pub owner: &'a AccountInfo<'info>,
    pub source_ata: &'a InterfaceAccount<'info, TokenAccount>,
    pub destination_ata: &'a InterfaceAccount<'info, TokenAccount>,
    pub destination_owner: &'a AccountInfo<'info>,
    pub mint: &'a InterfaceAccount<'info, Mint>,
    pub metadata: &'a UncheckedAccount<'info>,
    pub edition: &'a UncheckedAccount<'info>,
    pub system_program: &'a Program<'info, System>,
    pub spl_token_program: &'a Interface<'info, TokenInterface>,
    pub spl_ata_program: &'a Program<'info, AssociatedToken>,
    pub sysvar_instructions: &'a UncheckedAccount<'info>,
    pub token_metadata_program: &'a UncheckedAccount<'info>,
    pub owner_token_record: Option<&'a UncheckedAccount<'info>>,
    pub destination_token_record: Option<&'a UncheckedAccount<'info>>,
    pub authorization_rules_program: Option<&'a UncheckedAccount<'info>>,
    pub authorization_rules: Option<&'a UncheckedAccount<'info>>,
    pub authorization_data: Option<AuthorizationData>,
    // if passed, we assign a delegate first, and the call invoke_signed() instead of invoke()
    pub delegate: Option<&'a AccountInfo<'info>>,
}

fn cpi_transfer(args: TransferArgs, signer_seeds: Option<&[&[&[u8]]]>) -> Result<()> {
    // prepares the CPI instruction
    let mut transfer_cpi = TransferV1CpiBuilder::new(args.token_metadata_program);
    transfer_cpi
        .authority(args.owner)
        .token_owner(args.owner)
        .token(args.source_ata.as_ref())
        .destination_owner(args.destination_owner)
        .destination_token(args.destination_ata.as_ref())
        .mint(args.mint.as_ref())
        .metadata(args.metadata.as_ref())
        .edition(Some(args.edition))
        .payer(args.payer)
        .spl_ata_program(args.spl_ata_program)
        .spl_token_program(args.spl_token_program)
        .system_program(args.system_program)
        .sysvar_instructions(args.sysvar_instructions)
        .token_record(args.owner_token_record.map(|account| account.as_ref()))
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

        let mut delegate_cpi = DelegateTransferV1CpiBuilder::new(args.token_metadata_program);
        delegate_cpi
            .authority(args.owner)
            .delegate(delegate)
            .token(args.source_ata.as_ref())
            .mint(args.mint.as_ref())
            .metadata(args.metadata)
            .master_edition(Some(args.edition))
            .payer(args.payer)
            .spl_token_program(Some(args.spl_token_program))
            .token_record(args.owner_token_record.map(|account| account.as_ref()))
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
            authority: args.owner.to_account_info(),
            mint: args.mint.to_account_info(),
        },
    );

    if let Some(signer_seeds) = signer_seeds {
        token_interface::transfer_checked(ctx.with_signer(signer_seeds), 1, 0)
    } else {
        token_interface::transfer_checked(ctx, 1, 0)
    }
}
