#![allow(dead_code)]

pub const HUNDRED_PCT_BPS: u16 = 10000;

use std::slice::Iter;

use anchor_lang::{
    prelude::*,
    solana_program::{
        program::invoke,
        system_instruction,
    },
};
use mpl_bubblegum::{
    self,
    state::metaplex_adapter::Creator,
};

use vipers::{prelude::*};
use crate::CNFTError;

pub fn calc_fees(amount: u64, fee_bps: u16) -> Result<u64> {
    let fee = unwrap_checked!({
        (fee_bps as u64)
            .checked_mul(amount)?
            .checked_div(HUNDRED_PCT_BPS as u64)
    });

    Ok(fee)
}

pub fn calc_creators_fee(
    seller_fee_basis_points: u16,
    amount: u64,
    optional_royalty_pct: Option<u16>,
) -> Result<u64> {
    let creator_fee_bps = if let Some(optional_royalty_pct) = optional_royalty_pct {
        require!(
            optional_royalty_pct <= 100,
            CNFTError::BadRoyaltiesPct
        );

        // If optional passed, pay optional royalties
        unwrap_checked!({
            (seller_fee_basis_points as u64)
                .checked_mul(optional_royalty_pct as u64)?
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
        CNFTError::InsufficientBalance
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
pub fn transfer_creators_fee<'a, 'info>(
    from: &'a FromAcc<'a, 'info>,
    creators: &'a Vec<Creator>,
    creator_accounts: &mut Iter<AccountInfo<'info>>,
    creator_fee: u64,
) -> Result<u64> {
    // Send royalties: taken from AH's calculation:
    // https://github.com/metaplex-foundation/metaplex-program-library/blob/2320b30ec91b729b153f0c0fe719f96d325b2358/auction-house/program/src/utils.rs#L366-L471
    let mut remaining_fee = creator_fee;
    for creator in creators {
        let current_creator_info = next_account_info(creator_accounts)?;
        require!(
            creator.address.eq(current_creator_info.key),
            CNFTError::CreatorMismatch
        );

        let rent = Rent::get()?.minimum_balance(current_creator_info.data_len());

        let pct = creator.share as u64;
        let creator_fee = unwrap_checked!({ pct.checked_mul(creator_fee)?.checked_div(100) });

        // Prevents InsufficientFundsForRent, where creator acc doesn't have enough fee
        // https://explorer.solana.com/tx/vY5nYA95ELVrs9SU5u7sfU2ucHj4CRd3dMCi1gWrY7MSCBYQLiPqzABj9m8VuvTLGHb9vmhGaGY7mkqPa1NLAFE
        if unwrap_int!(current_creator_info.lamports().checked_add(creator_fee)) < rent {
            //skip current creator, we can't pay them
            continue;
        }

        remaining_fee = unwrap_int!(remaining_fee.checked_sub(creator_fee));
        if creator_fee > 0 {
            match from {
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
            }
        }
    }

    // Return the amount that was sent (minus any dust).
    Ok(unwrap_int!(creator_fee.checked_sub(remaining_fee)))
}
