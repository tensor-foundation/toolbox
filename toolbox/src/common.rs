#![allow(clippy::result_large_err)]

use anchor_lang::{
    prelude::*,
    solana_program::{program::invoke, system_instruction, system_program},
};
use anchor_spl::{associated_token::AssociatedToken, token_interface::TokenInterface};
use mpl_token_metadata::types::TokenStandard;
use solana_program::pubkey;
use std::slice::Iter;
use tensor_vipers::prelude::*;

use crate::TensorError;

pub const HUNDRED_PCT_BPS: u64 = 10000;
pub const HUNDRED_PCT: u64 = 100;
pub const BROKER_FEE_PCT: u64 = 50;
pub const TNSR_DISCOUNT_BPS: u64 = 2500;
pub const TAKER_FEE_BPS: u64 = 200;
pub const MAKER_BROKER_PCT: u64 = 80; // Out of 100

pub mod escrow {
    use super::*;
    declare_id!("TSWAPaqyCSx2KABk68Shruf4rp7CxcNi8hAsbdwmHbN");

    pub const TSWAP_SINGLETON: Pubkey = pubkey!("4zdNGgAtFsW1cQgHqkiWyRsxaAgxrSRRynnuunxzjxue");
}

pub mod fees {
    use super::*;
    declare_id!("TFEEgwDP6nn1s8mMX2tTNPPz8j2VomkphLUmyxKm17A");
}

pub mod marketplace {
    use super::*;
    declare_id!("TCMPhJdwDryooaGtiocG1u3xcYbRpiJzb283XfCZsDp");

    pub const TCOMP_SINGLETON: Pubkey = pubkey!("q4s8z5dRAt2fKC2tLthBPatakZRXPMx1LfacckSXd4f");
}

pub mod mpl_token_auth_rules {
    use super::*;
    declare_id!("auth9SigNpDKz4sJJ1DfCTuZrZNSAgh9sFD3rboVmgg");
}

pub mod price_lock {
    use super::*;
    declare_id!("TLoCKic2wGJm7VhZKumih4Lc35fUhYqVMgA4j389Buk");

    pub const TLOCK_SINGLETON: Pubkey = pubkey!("CdXA5Vpg4hqvsmLSKC2cygnJVvsQTrDrrn428nAZQaKz");
}

/// Calculates fee vault shard from a given AccountInfo or Pubkey. Relies on the Anchor `Key` trait.
#[macro_export]
macro_rules! shard_num {
    ($value:expr) => {
        &$value.key().as_ref()[31].to_le_bytes()
    };
}

pub struct CalcFeesArgs {
    pub amount: u64,
    pub total_fee_bps: u64,
    pub broker_fee_pct: u64,
    pub maker_broker_pct: u64,
    pub tnsr_discount: bool,
}

/// Fees struct that holds the calculated fees.
pub struct Fees {
    /// Taker fee is the total fee sans royalties: protocol fee + broker fees.
    pub taker_fee: u64,
    /// Protocol fee is the fee that goes to the protocol, a percentage of the total fee determined by 1 - broker_fee_pct.
    pub protocol_fee: u64,
    /// Maker broker fee is the fee that goes to the maker broker: a percentage of the total broker fee.
    pub maker_broker_fee: u64,
    /// Taker broker fee is the fee that goes to the taker broker: the remainder of the total broker fee.
    pub taker_broker_fee: u64,
}

// Calculate fees for a given amount.
pub fn calc_fees(args: CalcFeesArgs) -> Result<Fees> {
    let CalcFeesArgs {
        amount,
        total_fee_bps,
        broker_fee_pct,
        maker_broker_pct,
        tnsr_discount,
    } = args;

    // Apply the TNSR discount if enabled.
    let total_fee_bps = if tnsr_discount {
        unwrap_checked!({
            total_fee_bps
                .checked_mul(HUNDRED_PCT_BPS - TNSR_DISCOUNT_BPS)?
                .checked_div(HUNDRED_PCT_BPS)
        })
    } else {
        total_fee_bps
    };

    // Total fee is calculated from the passed in total_fee_bps and is protocol fee + broker fees.
    let total_fee = unwrap_checked!({
        (amount)
            .checked_mul(total_fee_bps)?
            .checked_div(HUNDRED_PCT_BPS)
    });

    // Broker fees are a percentage of the total fee.
    let broker_fees = unwrap_checked!({
        total_fee
            .checked_mul(broker_fee_pct)?
            .checked_div(HUNDRED_PCT)
    });

    // Protocol fee is the remainder.
    let protocol_fee = unwrap_checked!({ total_fee.checked_sub(broker_fees) });

    // Maker broker is a percentage of the total brokers fee.
    let maker_broker_fee = unwrap_checked!({
        broker_fees
            .checked_mul(maker_broker_pct)?
            .checked_div(HUNDRED_PCT)
    });

    // Remaining broker fee is the taker broker fee.
    let taker_broker_fee = unwrap_int!(broker_fees.checked_sub(maker_broker_fee));

    Ok(Fees {
        taker_fee: total_fee,
        protocol_fee,
        maker_broker_fee,
        taker_broker_fee,
    })
}

pub fn is_royalty_enforced(token_standard: Option<TokenStandard>) -> bool {
    matches!(
        token_standard,
        Some(TokenStandard::ProgrammableNonFungible)
            | Some(TokenStandard::ProgrammableNonFungibleEdition)
    )
}

pub fn calc_creators_fee(
    seller_fee_basis_points: u16,
    amount: u64,
    royalty_pct: Option<u16>,
) -> Result<u64> {
    let creator_fee_bps = if let Some(royalty_pct) = royalty_pct {
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
            .checked_div(HUNDRED_PCT_BPS)
    });

    Ok(fee)
}

/// Transfers all lamports from a PDA (except for rent) to a destination account.
pub fn transfer_all_lamports_from_pda<'info>(
    from_pda: &AccountInfo<'info>,
    to: &AccountInfo<'info>,
) -> Result<()> {
    let rent = Rent::get()?.minimum_balance(from_pda.data_len());
    let to_move = unwrap_int!(from_pda.lamports().checked_sub(rent));

    transfer_lamports_from_pda(from_pda, to, to_move)
}

/// Transfers specified lamports from a PDA to a destination account.
/// Throws an error if less than rent remains in the PDA.
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

#[cfg(feature = "mpl-core")]
//from token meta
impl From<mpl_core::types::Creator> for TCreator {
    fn from(creator: mpl_core::types::Creator) -> Self {
        TCreator {
            address: creator.address,
            share: creator.percentage,
            // mpl-core does not have a concept of "verified" creator
            verified: false,
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
                    // ATA validated on transfer CPI.
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

/// Transfers lamports from one account to another, handling the cases where the account
/// is either a PDA or a system account.
pub fn transfer_lamports<'info>(
    from: &AccountInfo<'info>,
    to: &AccountInfo<'info>,
    lamports: u64,
) -> Result<()> {
    // if the from account is empty, we can use the system program to transfer
    if from.data_is_empty() && from.owner == &system_program::ID {
        invoke(
            &system_instruction::transfer(from.key, to.key, lamports),
            &[from.clone(), to.clone()],
        )
        .map_err(Into::into)
    } else {
        transfer_lamports_from_pda(from, to, lamports)
    }
}

/// Transfers lamports, skipping the transfer if the `to` account would not be rent exempt.
///
/// This is useful when transferring lamports to a new account that may not have been created yet
/// and the transfer amount is less than the rent exemption.
pub fn transfer_lamports_checked<'info, 'b>(
    from: &'b AccountInfo<'info>,
    to: &'b AccountInfo<'info>,
    lamports: u64,
) -> Result<()> {
    let rent = Rent::get()?.minimum_balance(to.data_len());
    if unwrap_int!(to.lamports().checked_add(lamports)) < rent {
        // skip the transfer if the account as the account would not be rent exempt
        msg!(
            "Skipping transfer of {} lamports to {}: account would not be rent exempt",
            lamports,
            to.key
        );
        Ok(())
    } else {
        transfer_lamports(from, to, lamports)
    }
}

/// Asserts that the account is a valid fee account: either one of the program singletons or the fee vault.
pub fn assert_fee_account(fee_vault_info: &AccountInfo, state_info: &AccountInfo) -> Result<()> {
    let expected_fee_vault = Pubkey::find_program_address(
        &[
            b"fee_vault",
            // Use the last byte of the mint as the fee shard number
            shard_num!(state_info),
        ],
        &fees::ID,
    )
    .0;

    require!(
        fee_vault_info.key == &expected_fee_vault
            || &marketplace::TCOMP_SINGLETON == fee_vault_info.key,
        TensorError::InvalidFeeAccount
    );

    Ok(())
}
