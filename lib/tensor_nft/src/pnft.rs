#![allow(clippy::result_large_err)]

use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{self, Mint, TokenAccount, TokenInterface, TransferChecked},
};
use mpl_token_metadata::{
    accounts::Metadata,
    instructions::{DelegateTransferV1CpiBuilder, TransferV1CpiBuilder},
    types::{AuthorizationData, ProgrammableConfig, TokenStandard},
};
use vipers::throw_err;

use crate::*;

#[inline(never)]
pub fn assert_decode_metadata(
    nft_mint: &Pubkey,
    metadata_account: &AccountInfo,
) -> Result<Metadata> {
    let (key, _) = Metadata::find_pda(nft_mint);
    if key != metadata_account.key() {
        throw_err!(TensorError::BadMetadata);
    }
    // Check account owner (redundant because of find_program_address above, but why not).
    if *metadata_account.owner != mpl_token_metadata::ID {
        throw_err!(TensorError::BadMetadata);
    }

    Ok(Metadata::try_from(metadata_account)?)
}

pub struct PnftTransferArgs<'a, 'info> {
    //for escrow accounts authority always === owner, for token accs can be diff but our protocol doesn't yet support that
    pub authority_and_owner: &'a AccountInfo<'info>,
    //(!) payer can't carry data, has to be a normal KP:
    // https://github.com/solana-labs/solana/blob/bda0c606a19ce1cc44b5ab638ff0b993f612e76c/runtime/src/system_instruction_processor.rs#L197
    pub payer: &'a AccountInfo<'info>,
    pub source_ata: &'a InterfaceAccount<'info, TokenAccount>,
    pub dest_ata: &'a InterfaceAccount<'info, TokenAccount>,
    pub dest_owner: &'a AccountInfo<'info>,
    pub nft_mint: &'a InterfaceAccount<'info, Mint>,
    pub nft_metadata: &'a UncheckedAccount<'info>,
    pub nft_edition: &'a UncheckedAccount<'info>,
    pub system_program: &'a Program<'info, System>,
    pub token_program: &'a Interface<'info, TokenInterface>,
    pub ata_program: &'a Program<'info, AssociatedToken>,
    pub instructions: &'a UncheckedAccount<'info>,
    pub owner_token_record: &'a UncheckedAccount<'info>,
    pub dest_token_record: &'a UncheckedAccount<'info>,
    pub authorization_rules_program: &'a UncheckedAccount<'info>,
    pub rules_acc: Option<&'a AccountInfo<'info>>,
    pub authorization_data: Option<AuthorizationData>,
    //if passed, we assign a delegate first, and the call signed_invoke() instead of invoke()
    pub delegate: Option<&'a AccountInfo<'info>>,
}

fn pnft_transfer_cpi(signer_seeds: Option<&[&[&[u8]]]>, args: PnftTransferArgs) -> Result<()> {
    let metadata = assert_decode_metadata(&args.nft_mint.key(), args.nft_metadata)?;

    let mut transfer_cpi = TransferV1CpiBuilder::new(args.token_program);
    transfer_cpi
        .authority(args.authority_and_owner)
        .token_owner(args.authority_and_owner)
        .token(args.source_ata.as_ref())
        .destination_owner(args.dest_owner)
        .destination_token(args.dest_ata.as_ref())
        .mint(args.nft_mint.as_ref())
        .metadata(args.nft_metadata.as_ref())
        .edition(Some(args.nft_edition))
        .payer(args.payer)
        .spl_ata_program(args.ata_program)
        .spl_token_program(args.token_program)
        .system_program(args.system_program)
        .sysvar_instructions(args.instructions)
        .amount(1);
    // set the authorization data if passed in
    args.authorization_data
        .clone()
        .map(|data| transfer_cpi.authorization_data(data));

    if matches!(
        metadata.token_standard,
        Some(TokenStandard::ProgrammableNonFungible)
    ) {
        transfer_cpi
            .token_record(Some(args.owner_token_record.as_ref()))
            .destination_token_record(Some(args.dest_token_record.as_ref()));
    }

    // if auth rules passed in, validate & include it in CPI call
    if let Some(ProgrammableConfig::V1 {
        rule_set: Some(rule_set),
    }) = metadata.programmable_config
    {
        let rules_acc = args.rules_acc.ok_or(TensorError::BadRuleSet)?;

        // 1. validate
        if rule_set != *rules_acc.key {
            throw_err!(TensorError::BadRuleSet);
        }

        // 2. add to builder
        transfer_cpi
            .authorization_rules_program(Some(args.authorization_rules_program))
            .authorization_rules(Some(rules_acc));

        // 3. invoke delegate if necessary
        if let Some(delegate) = args.delegate {
            // replace authority on the builder with the newly assigned delegate
            transfer_cpi.authority(delegate);

            let mut delegate_cpi = DelegateTransferV1CpiBuilder::new(args.token_program);
            delegate_cpi
                .authority(args.authority_and_owner)
                .delegate(delegate)
                .token(args.source_ata.as_ref())
                .mint(args.nft_mint.as_ref())
                .metadata(args.nft_metadata)
                .master_edition(Some(args.nft_edition))
                .payer(args.payer)
                .spl_token_program(Some(args.token_program))
                .token_record(Some(args.owner_token_record))
                .authorization_rules(Some(rules_acc))
                .authorization_rules_program(Some(args.authorization_rules_program))
                .amount(1);

            args.authorization_data
                .map(|data| delegate_cpi.authorization_data(data));

            delegate_cpi.invoke()?;
        }
    }

    if let Some(signer_seeds) = signer_seeds {
        transfer_cpi.invoke_signed(signer_seeds)?;
    } else {
        transfer_cpi.invoke()?;
    }

    Ok(())
}

pub fn send_pnft(
    //if passed, use signed_invoke() instead of invoke()
    signer_seeds: Option<&[&[&[u8]]]>,
    args: PnftTransferArgs,
) -> Result<()> {
    // for some reason for some old nfts, the user can no longer delist with this error
    // https://solscan.io/tx/4EbK8Us3c3mixGY4Y6zUx4pRoarWHJ718PtU8vDnAkcC6GmVtBWi8jotZ8koML8c94JPmQB6jHjQnPEBb83Mfv7C
    // hence have to do a normal transfer

    let metadata = assert_decode_metadata(&args.nft_mint.key(), args.nft_metadata)?;

    if metadata.token_standard != Some(TokenStandard::ProgrammableNonFungible) {
        // msg!("non-pnft / no token std, normal transfer");

        let ctx = CpiContext::new(
            args.token_program.to_account_info(),
            TransferChecked {
                from: args.source_ata.to_account_info(),
                to: args.dest_ata.to_account_info(),
                authority: args.authority_and_owner.to_account_info(),
                mint: args.nft_mint.to_account_info(),
            },
        );

        if let Some(signer_seeds) = signer_seeds {
            token_interface::transfer_checked(ctx.with_signer(signer_seeds), 1, 0)?;
        } else {
            token_interface::transfer_checked(ctx, 1, 0)?;
        }

        return Ok(());
    }

    // --------------------------------------- pnft transfer

    pnft_transfer_cpi(signer_seeds, args)?;

    Ok(())
}
