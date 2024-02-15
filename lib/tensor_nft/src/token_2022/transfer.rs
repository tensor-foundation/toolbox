//! Transfer CPI for SPL Token 2022 that requires remaining accounts to be passed.
//!
//! This is a workaround for the current limitation of the Anchor framework.

use anchor_lang::{
    context::CpiContext,
    solana_program::{instruction::AccountMeta, program::invoke_signed},
    Result,
};
use anchor_spl::token_interface::{spl_token_2022, TransferChecked};

pub fn transfer_checked<'info>(
    ctx: CpiContext<'_, '_, '_, 'info, TransferChecked<'info>>,
    amount: u64,
    decimals: u8,
) -> Result<()> {
    let mut ix = spl_token_2022::instruction::transfer_checked(
        ctx.program.key,
        ctx.accounts.from.key,
        ctx.accounts.mint.key,
        ctx.accounts.to.key,
        ctx.accounts.authority.key,
        &[],
        amount,
        decimals,
    )?;

    let (from, mint, to, authority, remaining) = (
        ctx.accounts.from,
        ctx.accounts.mint,
        ctx.accounts.to,
        ctx.accounts.authority,
        ctx.remaining_accounts,
    );

    let mut accounts = vec![from, mint, to, authority];

    remaining.into_iter().for_each(|account| {
        ix.accounts.push(AccountMeta {
            pubkey: *account.key,
            is_signer: account.is_signer,
            is_writable: account.is_writable,
        });
        accounts.push(account);
    });

    invoke_signed(&ix, &accounts, ctx.signer_seeds).map_err(Into::into)
}
