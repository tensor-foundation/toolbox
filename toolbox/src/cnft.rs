#![allow(clippy::result_large_err)]

use anchor_lang::{prelude::*, solana_program::keccak::hashv};
use mpl_bubblegum::{
    hash::hash_creators,
    instructions::TransferCpiBuilder,
    types::{Creator, MetadataArgs},
    utils::get_asset_id,
};

use crate::TCreator;

//into bubblgum
impl From<TCreator> for Creator {
    fn from(creator: TCreator) -> Self {
        Creator {
            address: creator.address,
            verified: creator.verified,
            share: creator.share,
        }
    }
}

//from bubblegum
impl From<Creator> for TCreator {
    fn from(creator: Creator) -> Self {
        TCreator {
            address: creator.address,
            verified: creator.verified,
            share: creator.share,
        }
    }
}

pub struct TransferArgs<'a, 'info> {
    pub root: [u8; 32],
    pub nonce: u64,
    pub index: u32,
    pub data_hash: [u8; 32],
    pub creator_hash: [u8; 32],
    pub tree_authority: &'a AccountInfo<'info>,
    pub leaf_owner: &'a AccountInfo<'info>,
    pub leaf_delegate: &'a AccountInfo<'info>,
    pub new_leaf_owner: &'a AccountInfo<'info>,
    pub merkle_tree: &'a AccountInfo<'info>,
    pub log_wrapper: &'a AccountInfo<'info>,
    pub compression_program: &'a AccountInfo<'info>,
    pub system_program: &'a AccountInfo<'info>,
    pub bubblegum_program: &'a AccountInfo<'info>,
    pub proof_accounts: &'a [AccountInfo<'info>],
    //either both or neither should be passed
    pub signer: Option<&'a AccountInfo<'info>>,
    pub signer_seeds: Option<&'a [&'a [u8]]>,
}

pub fn transfer_cnft(args: TransferArgs) -> Result<()> {
    let TransferArgs {
        root,
        nonce,
        index,
        data_hash,
        creator_hash,
        tree_authority,
        leaf_owner,
        leaf_delegate,
        new_leaf_owner,
        merkle_tree,
        log_wrapper,
        compression_program,
        system_program,
        bubblegum_program,
        proof_accounts,
        signer,
        signer_seeds,
    } = args;

    let (owner_signer, delegate_signer) = if let Some(signer) = signer {
        (
            leaf_owner.is_signer || signer.key() == leaf_owner.key(),
            leaf_delegate.is_signer || signer.key() == leaf_delegate.key(),
        )
    } else {
        (leaf_owner.is_signer, leaf_delegate.is_signer)
    };

    let mut transfer_cpi = TransferCpiBuilder::new(bubblegum_program);
    transfer_cpi
        .tree_config(tree_authority)
        .leaf_owner(leaf_owner, owner_signer)
        .leaf_delegate(leaf_delegate, delegate_signer)
        .new_leaf_owner(new_leaf_owner)
        .merkle_tree(merkle_tree)
        .log_wrapper(log_wrapper)
        .compression_program(compression_program)
        .system_program(system_program)
        .root(root)
        .data_hash(data_hash)
        .creator_hash(creator_hash)
        .nonce(nonce)
        .index(index);

    for proof in proof_accounts {
        transfer_cpi.add_remaining_account(proof, false, false);
    }

    if let Some(signer_seeds) = signer_seeds {
        transfer_cpi.invoke_signed(&[signer_seeds])?;
    } else {
        transfer_cpi.invoke()?;
    }

    Ok(())
}

pub enum MetadataSrc {
    Metadata(MetadataArgs),
    DataHash(DataHashArgs),
}

pub struct DataHashArgs {
    pub meta_hash: [u8; 32],
    pub creator_shares: Vec<u8>,
    pub creator_verified: Vec<bool>,
    pub seller_fee_basis_points: u16,
}

pub struct MakeCnftArgs<'a, 'info> {
    pub nonce: u64,
    pub metadata_src: MetadataSrc,
    pub merkle_tree: &'a AccountInfo<'info>,
    pub creator_accounts: &'a [AccountInfo<'info>],
}

pub struct CnftArgs {
    pub asset_id: Pubkey,
    pub data_hash: [u8; 32],
    pub creator_hash: [u8; 32],
    pub creators: Vec<Creator>,
}

pub fn make_cnft_args(args: MakeCnftArgs) -> Result<CnftArgs> {
    let MakeCnftArgs {
        metadata_src,
        creator_accounts,
        nonce,
        merkle_tree,
    } = args;

    // --------------------------------------- from bubblegum/process_mint_v1

    let (data_hash, creator_hash, creators) = match metadata_src {
        MetadataSrc::Metadata(mplex_metadata) => {
            let creator_hash = hash_creators(&mplex_metadata.creators);
            let metadata_args_hash = hashv(&[mplex_metadata.try_to_vec()?.as_slice()]);
            let data_hash = hashv(&[
                &metadata_args_hash.to_bytes(),
                &mplex_metadata.seller_fee_basis_points.to_le_bytes(),
            ])
            .to_bytes();

            (data_hash, creator_hash, mplex_metadata.creators)
        }
        MetadataSrc::DataHash(DataHashArgs {
            meta_hash,
            creator_shares,
            creator_verified,
            seller_fee_basis_points,
        }) => {
            // Verify seller fee basis points
            let data_hash = hashv(&[&meta_hash, &seller_fee_basis_points.to_le_bytes()]).to_bytes();
            // Verify creators
            let creators = creator_accounts
                .iter()
                .zip(creator_shares.iter())
                .zip(creator_verified.iter())
                .map(|((c, s), v)| Creator {
                    address: c.key(),
                    verified: *v,
                    share: *s,
                })
                .collect::<Vec<_>>();
            let creator_hash = hash_creators(&creators);

            (data_hash, creator_hash, creators)
        }
    };

    Ok(CnftArgs {
        asset_id: get_asset_id(&merkle_tree.key(), nonce),
        data_hash,
        creator_hash,
        creators,
    })
}
