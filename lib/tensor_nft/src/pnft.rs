use anchor_lang::{
    prelude::*,
    solana_program::{
        instruction::Instruction,
        program::{invoke, invoke_signed},
    },
};
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{Mint, Token, TokenAccount},
};
use mpl_token_metadata::{
    self,
    instruction::{
        builders::{DelegateBuilder, TransferBuilder},
        DelegateArgs, InstructionBuilder, TransferArgs,
    },
    processor::AuthorizationData,
    state::{Metadata, ProgrammableConfig::V1, TokenMetadataAccount, TokenStandard},
};
use vipers::throw_err;

use crate::*;

#[inline(never)]
pub fn assert_decode_metadata<'info>(
    nft_mint: &Account<'info, Mint>,
    metadata_account: &UncheckedAccount<'info>,
) -> Result<Metadata> {
    let (key, _) = Pubkey::find_program_address(
        &[
            mpl_token_metadata::state::PREFIX.as_bytes(),
            mpl_token_metadata::id().as_ref(),
            nft_mint.key().as_ref(),
        ],
        &mpl_token_metadata::id(),
    );
    if key != *metadata_account.to_account_info().key {
        throw_err!(TensorError::BadMetadata);
    }
    // Check account owner (redundant because of find_program_address above, but why not).
    if *metadata_account.owner != mpl_token_metadata::id() {
        throw_err!(TensorError::BadMetadata);
    }

    Ok(Metadata::from_account_info(metadata_account)?)
}

pub struct PnftTransferArgs<'a, 'info> {
    //for escrow accounts authority always === owner, for token accs can be diff but our protocol doesn't yet support that
    pub authority_and_owner: &'a AccountInfo<'info>,
    //(!) payer can't carry data, has to be a normal KP:
    // https://github.com/solana-labs/solana/blob/bda0c606a19ce1cc44b5ab638ff0b993f612e76c/runtime/src/system_instruction_processor.rs#L197
    pub payer: &'a AccountInfo<'info>,
    pub source_ata: &'a Account<'info, TokenAccount>,
    pub dest_ata: &'a Account<'info, TokenAccount>,
    pub dest_owner: &'a AccountInfo<'info>,
    pub nft_mint: &'a Account<'info, Mint>,
    pub nft_metadata: &'a UncheckedAccount<'info>,
    pub nft_edition: &'a UncheckedAccount<'info>,
    pub system_program: &'a Program<'info, System>,
    pub token_program: &'a Program<'info, Token>,
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

#[allow(clippy::too_many_arguments)]
fn prep_pnft_transfer_ix<'info>(
    args: PnftTransferArgs<'_, 'info>,
) -> Result<(Instruction, Vec<AccountInfo<'info>>)> {
    let metadata = assert_decode_metadata(args.nft_mint, args.nft_metadata)?;
    let mut builder = TransferBuilder::new();
    builder
        .authority(*args.authority_and_owner.key)
        .token_owner(*args.authority_and_owner.key)
        .token(args.source_ata.key())
        .destination_owner(*args.dest_owner.key)
        .destination(args.dest_ata.key())
        .mint(args.nft_mint.key())
        .metadata(args.nft_metadata.key())
        .edition(args.nft_edition.key())
        .payer(*args.payer.key);

    let mut account_infos = vec![
        //   0. `[writable]` Token account
        args.source_ata.to_account_info(),
        //   1. `[]` Token account owner
        args.authority_and_owner.to_account_info(),
        //   2. `[writable]` Destination token account
        args.dest_ata.to_account_info(),
        //   3. `[]` Destination token account owner
        args.dest_owner.to_account_info(),
        //   4. `[]` Mint of token asset
        args.nft_mint.to_account_info(),
        //   5. `[writable]` Metadata account
        args.nft_metadata.to_account_info(),
        //   6. `[optional]` Edition of token asset
        args.nft_edition.to_account_info(),
        //   7. `[signer] Transfer authority (token or delegate owner)
        args.authority_and_owner.to_account_info(),
        //   8. `[optional, writable]` Owner record PDA
        //passed in below, if needed
        //   9. `[optional, writable]` Destination record PDA
        //passed in below, if needed
        //   10. `[signer, writable]` Payer
        args.payer.to_account_info(),
        //   11. `[]` System Program
        args.system_program.to_account_info(),
        //   12. `[]` Instructions sysvar account
        args.instructions.to_account_info(),
        //   13. `[]` SPL Token Program
        args.token_program.to_account_info(),
        //   14. `[]` SPL Associated Token Account program
        args.ata_program.to_account_info(),
        //   15. `[optional]` Token Authorization Rules Program
        //passed in below, if needed
        //   16. `[optional]` Token Authorization Rules account
        //passed in below, if needed
    ];

    if let Some(standard) = metadata.token_standard {
        if standard == TokenStandard::ProgrammableNonFungible {
            // msg!("programmable standard triggered");
            //1. add to builder
            builder
                .owner_token_record(args.owner_token_record.key())
                .destination_token_record(args.dest_token_record.key());

            //2. add to accounts (if try to pass these for non-pNFT, will get owner errors, since they don't exist)
            account_infos.push(args.owner_token_record.to_account_info());
            account_infos.push(args.dest_token_record.to_account_info());
        }
    }

    //if auth rules passed in, validate & include it in CPI call
    if let Some(config) = metadata.programmable_config {
        match config {
            V1 { rule_set } => {
                if let Some(rule_set) = rule_set {
                    // msg!("ruleset triggered");
                    //safe to unwrap here, it's expected
                    let rules_acc = args.rules_acc.unwrap();

                    //1. validate
                    if rule_set != *rules_acc.key {
                        throw_err!(TensorError::BadRuleSet);
                    }

                    //2. add to builder
                    builder.authorization_rules_program(*args.authorization_rules_program.key);
                    builder.authorization_rules(*rules_acc.key);

                    //3. add to accounts
                    account_infos.push(args.authorization_rules_program.to_account_info());
                    account_infos.push(rules_acc.to_account_info());

                    //4. invoke delegate if necessary
                    if let Some(delegate) = args.delegate {
                        let delegate_ix = DelegateBuilder::new()
                            .authority(*args.authority_and_owner.key)
                            .delegate(delegate.key())
                            .token(args.source_ata.key())
                            .mint(args.nft_mint.key())
                            .metadata(args.nft_metadata.key())
                            .master_edition(args.nft_edition.key())
                            .payer(*args.payer.key)
                            .spl_token_program(args.token_program.key())
                            .token_record(args.owner_token_record.key())
                            .authorization_rules(rules_acc.key())
                            .authorization_rules_program(args.authorization_rules_program.key())
                            .build(DelegateArgs::TransferV1 {
                                amount: 1,
                                authorization_data: args.authorization_data.clone(),
                            })
                            .unwrap()
                            .instruction();

                        let delegate_account_infos = vec![
                            //   0. `[optional, writable]` Delegate record account
                            // NO NEED
                            //   1. `[]` Delegated owner
                            delegate.to_account_info(),
                            //   2. `[writable]` Metadata account
                            args.nft_metadata.to_account_info(),
                            //   3. `[optional]` Master Edition account
                            args.nft_edition.to_account_info(),
                            //   4. `[optional, writable]` Token record account
                            args.owner_token_record.to_account_info(),
                            //   5. `[]` Mint account
                            args.nft_mint.to_account_info(),
                            //   6. `[optional, writable]` Token account
                            args.source_ata.to_account_info(),
                            //   7. `[signer]` Update authority or token owner
                            args.authority_and_owner.to_account_info(),
                            //   8. `[signer, writable]` Payer
                            args.payer.to_account_info(),
                            //   9. `[]` System Program
                            args.system_program.to_account_info(),
                            //   10. `[]` Instructions sysvar account
                            args.instructions.to_account_info(),
                            //   11. `[optional]` SPL Token Program
                            args.token_program.to_account_info(),
                            // ata_program.to_account_info(),
                            //   12. `[optional]` Token Authorization Rules program
                            args.authorization_rules_program.to_account_info(),
                            //   13. `[optional]` Token Authorization Rules account
                            rules_acc.to_account_info(),
                        ];

                        // msg!("invoking delegate");
                        //always invoked normally
                        invoke(&delegate_ix, &delegate_account_infos)?;

                        //replace authority on the builder with the newly assigned delegate
                        builder.authority(delegate.key());
                        account_infos.push(delegate.to_account_info());
                    }
                }
            }
        }
    }

    let transfer_ix = builder
        .build(TransferArgs::V1 {
            amount: 1, //currently 1 only
            authorization_data: args.authorization_data,
        })
        .unwrap()
        .instruction();

    Ok((transfer_ix, account_infos))
}

#[allow(clippy::too_many_arguments)]
pub fn send_pnft(
    //if passed, use signed_invoke() instead of invoke()
    signer_seeds: Option<&[&[&[u8]]]>,
    args: PnftTransferArgs,
) -> Result<()> {
    let (transfer_ix, account_infos) = prep_pnft_transfer_ix(args)?;

    if let Some(signer_seeds) = signer_seeds {
        invoke_signed(&transfer_ix, &account_infos, signer_seeds)?;
    } else {
        invoke(&transfer_ix, &account_infos)?;
    }

    Ok(())
}
