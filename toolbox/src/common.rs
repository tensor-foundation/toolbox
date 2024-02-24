#![allow(clippy::result_large_err)]

use anchor_lang::{
    prelude::*,
    solana_program::{program::invoke, system_instruction, system_program},
};
use anchor_spl::{associated_token::AssociatedToken, token_interface::TokenInterface};
use mpl_token_metadata::types::TokenStandard;
use std::slice::Iter;
use vipers::prelude::*;

use crate::TensorError;

pub const HUNDRED_PCT_BPS: u16 = 10000;
pub const GAMESHIFT_FEE_BPS: u16 = 200;
pub const GAMESHIFT_BROKER_PCT: u16 = 50; // Out of 100

pub mod gameshift {
    use anchor_lang::declare_id;
    declare_id!("3g2nyraTXqEKke3sTtZw9JtfjCo8Hzw6qhKe8K2hrYuf");
}

pub fn calc_fees(
    amount: u64,
    fee_bps: u16,
    maker_broker_pct: u16,
    maker_broker: Option<Pubkey>,
    _taker_broker: Option<Pubkey>,
) -> Result<(u64, u64, u64)> {
    let (fee_bps, maker_broker_pct) = if maker_broker == Some(crate::gameshift::ID) {
        // gameshift fee schedule
        (GAMESHIFT_FEE_BPS, GAMESHIFT_BROKER_PCT)
    } else {
        (fee_bps, maker_broker_pct)
    };

    let full_fee = unwrap_checked!({
        (fee_bps as u64)
            .checked_mul(amount)?
            .checked_div(HUNDRED_PCT_BPS as u64)
    });
    let taker_broker_fee = 0; // todo: taker broker not enabled
    let maker_broker_fee = unwrap_checked!({
        full_fee
            .checked_mul(maker_broker_pct as u64)?
            .checked_div(100)
    });
    let protocol_fee = unwrap_checked!({
        full_fee
            .checked_sub(maker_broker_fee)
            .unwrap()
            .checked_sub(taker_broker_fee)
    });

    // Stupidity check, broker should never be higher than main fee (== when zero)
    require!(
        protocol_fee >= maker_broker_fee + taker_broker_fee,
        TensorError::ArithmeticError
    );

    Ok((protocol_fee, maker_broker_fee, taker_broker_fee))
}

pub fn calc_creators_fee(
    seller_fee_basis_points: u16,
    amount: u64,
    token_standard: Option<TokenStandard>,
    optional_royalty_pct: Option<u16>,
) -> Result<u64> {
    // Enforce royalties on pnfts.
    let adj_optional_royalty_pct =
        if let Some(TokenStandard::ProgrammableNonFungible) = token_standard {
            Some(100)
        } else {
            optional_royalty_pct
        };

    let creator_fee_bps = if let Some(royalty_pct) = adj_optional_royalty_pct {
        require!(royalty_pct <= 100, TensorError::BadRoyaltiesPct);

        // If optional passed, pay optional royalties
        unwrap_checked!({
            (seller_fee_basis_points as u64)
                .checked_mul(royalty_pct as u64)?
                .checked_div(100_u64)
        })
    } else {
        // Else pay 0
        0_u64
    };
    let fee = unwrap_checked!({
        creator_fee_bps
            .checked_mul(amount)?
            .checked_div(HUNDRED_PCT_BPS as u64)
    });

    Ok(fee)
}

pub fn transfer_all_lamports_from_pda<'info>(
    from_pda: &AccountInfo<'info>,
    to: &AccountInfo<'info>,
) -> Result<()> {
    let rent = Rent::get()?.minimum_balance(from_pda.data_len());
    let to_move = unwrap_int!(from_pda.lamports().checked_sub(rent));

    transfer_lamports_from_pda(from_pda, to, to_move)
}

pub fn transfer_lamports_from_pda<'info>(
    from_pda: &AccountInfo<'info>,
    to: &AccountInfo<'info>,
    lamports: u64,
) -> Result<()> {
    let remaining_pda_lamports = unwrap_int!(from_pda.lamports().checked_sub(lamports));
    // Check we are not withdrawing into our rent.
    let rent = Rent::get()?.minimum_balance(from_pda.data_len());
    require!(
        remaining_pda_lamports >= rent,
        TensorError::InsufficientBalance
    );

    **from_pda.try_borrow_mut_lamports()? = remaining_pda_lamports;

    let new_to = unwrap_int!(to.lamports.borrow().checked_add(lamports));
    **to.lamports.borrow_mut() = new_to;

    Ok(())
}

pub struct FromExternal<'b, 'info> {
    pub from: &'b AccountInfo<'info>,
    pub sys_prog: &'b AccountInfo<'info>,
}

pub enum FromAcc<'a, 'info> {
    Pda(&'a AccountInfo<'info>),
    External(&'a FromExternal<'a, 'info>),
}

#[derive(AnchorSerialize, AnchorDeserialize, PartialEq, Eq, Debug, Clone)]
pub struct TCreator {
    pub address: Pubkey,
    pub verified: bool,
    // In percentages, NOT basis points ;) Watch out!
    pub share: u8,
}

//into token meta
impl From<TCreator> for mpl_token_metadata::types::Creator {
    fn from(creator: TCreator) -> Self {
        mpl_token_metadata::types::Creator {
            address: creator.address,
            verified: creator.verified,
            share: creator.share,
        }
    }
}

//from token meta
impl From<mpl_token_metadata::types::Creator> for TCreator {
    fn from(creator: mpl_token_metadata::types::Creator) -> Self {
        TCreator {
            address: creator.address,
            verified: creator.verified,
            share: creator.share,
        }
    }
}

#[repr(u8)]
pub enum CreatorFeeMode<'a, 'info> {
    Sol {
        from: &'a FromAcc<'a, 'info>,
    },
    Spl {
        associated_token_program: &'a Program<'info, AssociatedToken>,
        token_program: &'a Interface<'info, TokenInterface>,
        system_program: &'a Program<'info, System>,
        currency: &'a AccountInfo<'info>,
        from: &'a AccountInfo<'info>,
        from_token_acc: &'a AccountInfo<'info>,
        rent_payer: &'a AccountInfo<'info>,
    },
}

pub fn transfer_creators_fee<'a, 'info>(
    //using TCreator here so that this fn is agnostic to normal NFTs and cNFTs
    creators: &'a Vec<TCreator>,
    creator_accounts: &mut Iter<AccountInfo<'info>>,
    creator_fee: u64,
    // put not-in-common args in an enum so the invoker doesn't require it
    mode: &'a CreatorFeeMode<'a, 'info>,
) -> Result<u64> {
    // Send royalties: taken from AH's calculation:
    // https://github.com/metaplex-foundation/metaplex-program-library/blob/2320b30ec91b729b153f0c0fe719f96d325b2358/auction-house/program/src/utils.rs#L366-L471
    let mut remaining_fee = creator_fee;
    for creator in creators {
        let current_creator_info = next_account_info(creator_accounts)?;
        require!(
            creator.address.eq(current_creator_info.key),
            TensorError::CreatorMismatch
        );

        let pct = creator.share as u64;
        let creator_fee = unwrap_checked!({ pct.checked_mul(creator_fee)?.checked_div(100) });

        match mode {
            CreatorFeeMode::Sol { from: _ } => {
                // Prevents InsufficientFundsForRent, where creator acc doesn't have enough fee
                // https://explorer.solana.com/tx/vY5nYA95ELVrs9SU5u7sfU2ucHj4CRd3dMCi1gWrY7MSCBYQLiPqzABj9m8VuvTLGHb9vmhGaGY7mkqPa1NLAFE
                let rent = Rent::get()?.minimum_balance(current_creator_info.data_len());
                if unwrap_int!(current_creator_info.lamports().checked_add(creator_fee)) < rent {
                    //skip current creator, we can't pay them
                    continue;
                }
            }
            CreatorFeeMode::Spl {
                associated_token_program: _,
                token_program: _,
                system_program: _,
                currency: _,
                from: _,
                from_token_acc: _,
                rent_payer: _,
            } => {}
        }

        remaining_fee = unwrap_int!(remaining_fee.checked_sub(creator_fee));
        if creator_fee > 0 {
            match mode {
                CreatorFeeMode::Sol { from } => match from {
                    FromAcc::Pda(from_pda) => {
                        transfer_lamports_from_pda(from_pda, current_creator_info, creator_fee)?;
                    }
                    FromAcc::External(from_ext) => {
                        let FromExternal { from, sys_prog } = from_ext;
                        invoke(
                            &system_instruction::transfer(
                                from.key,
                                current_creator_info.key,
                                creator_fee,
                            ),
                            &[
                                (*from).clone(),
                                current_creator_info.clone(),
                                (*sys_prog).clone(),
                            ],
                        )?;
                    }
                },

                CreatorFeeMode::Spl {
                    associated_token_program,
                    token_program,
                    system_program,
                    currency,
                    from,
                    from_token_acc: from_ata,
                    rent_payer,
                } => {
                    let current_creator_ata_info = next_account_info(creator_accounts)?;

                    anchor_spl::associated_token::create_idempotent(CpiContext::new(
                        associated_token_program.to_account_info(),
                        anchor_spl::associated_token::Create {
                            payer: rent_payer.to_account_info(),
                            associated_token: current_creator_ata_info.to_account_info(),
                            authority: current_creator_info.to_account_info(),
                            mint: currency.to_account_info(),
                            system_program: system_program.to_account_info(),
                            token_program: token_program.to_account_info(),
                        },
                    ))?;

                    anchor_spl::token::transfer(
                        CpiContext::new(
                            token_program.to_account_info(),
                            anchor_spl::token::Transfer {
                                from: from_ata.to_account_info(),
                                to: current_creator_ata_info.to_account_info(),
                                authority: from.to_account_info(),
                            },
                        ),
                        creator_fee,
                    )?;
                }
            }
        }
    }

    // Return the amount that was sent (minus any dust).
    Ok(unwrap_int!(creator_fee.checked_sub(remaining_fee)))
}

// NOT: https://github.com/coral-xyz/sealevel-attacks/blob/master/programs/9-closing-accounts/secure/src/lib.rs
// Instead: https://github.com/coral-xyz/anchor/blob/b7bada148cead931bc3bdae7e9a641e9be66e6a6/lang/src/common.rs#L6
pub fn close_account(
    pda_to_close: &mut AccountInfo,
    sol_destination: &mut AccountInfo,
) -> Result<()> {
    // Transfer tokens from the account to the sol_destination.
    let dest_starting_lamports = sol_destination.lamports();
    **sol_destination.lamports.borrow_mut() =
        unwrap_int!(dest_starting_lamports.checked_add(pda_to_close.lamports()));
    **pda_to_close.lamports.borrow_mut() = 0;

    pda_to_close.assign(&system_program::ID);
    pda_to_close.realloc(0, false).map_err(Into::into)
}
