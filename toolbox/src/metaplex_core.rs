use anchor_lang::solana_program::account_info::AccountInfo;
use anchor_lang::solana_program::msg;
use anchor_lang::solana_program::pubkey::Pubkey;
use anchor_lang::Result;
use mpl_core::{
    accounts::{BaseAssetV1, BaseCollectionV1},
    fetch_plugin,
    types::{
        Creator, Key, PluginType, Royalties, UpdateAuthority, VerifiedCreators,
        VerifiedCreatorsSignature,
    },
};

use crate::TensorError;

#[derive(Clone)]
pub struct MetaplexCore;

impl anchor_lang::Id for MetaplexCore {
    fn id() -> Pubkey {
        mpl_core::ID
    }
}

pub struct CoreAsset {
    pub pubkey: Pubkey,
    pub collection: Option<Pubkey>,
    pub whitelist_creators: Option<Vec<VerifiedCreatorsSignature>>,
    pub royalty_creators: Option<Vec<Creator>>,
    pub royalty_fee_bps: u16,
    pub royalty_enforced: bool,
}

pub fn validate_core_asset(
    asset_info: &AccountInfo,
    maybe_collection_info: Option<&AccountInfo>,
) -> Result<CoreAsset> {
    // validate the asset account
    assert_ownership(asset_info, Key::AssetV1)?;

    // validates the collection is owned by the MPL Core program
    maybe_collection_info
        .as_ref()
        .map(|c| assert_ownership(c, Key::CollectionV1));

    let asset = BaseAssetV1::try_from(asset_info)?;

    // if the asset has a collection, we must validate it, and fetch the royalties from it
    let (mut royalties, collection) =
        if let UpdateAuthority::Collection(asset_collection) = asset.update_authority {
            // Collection account must be provided.
            if maybe_collection_info.is_none() {
                msg!("Asset has a collection but no collection account was provided");
                return Err(TensorError::InvalidCoreAsset.into());
            }

            let collection_info = maybe_collection_info.unwrap();

            // Collection account must match the one on the asset.
            if asset_collection != *collection_info.key {
                msg!("Asset collection account does not match the provided collection account");
                return Err(TensorError::InvalidCoreAsset.into());
            }

            (
                fetch_plugin::<BaseCollectionV1, Royalties>(collection_info, PluginType::Royalties)
                    .ok()
                    .map(|(_, royalties, _)| royalties),
                Some(asset_collection),
            )
        } else {
            (None, None)
        };

    // Get royalties plugin from the asset, if present.
    // Royalties plugin directly on the asset take precedence over the one on the collection.
    royalties = fetch_plugin::<BaseAssetV1, Royalties>(asset_info, PluginType::Royalties)
        .ok()
        .map(|(_, royalties, _)| royalties)
        .or(royalties);

    let royalty_fee_bps = if let Some(Royalties { basis_points, .. }) = royalties {
        basis_points
    } else {
        0
    };

    // Fetch the verified creators from the MPL Core asset and map into the expected type
    // for whitelist verification.
    let verified_creators: Option<Vec<VerifiedCreatorsSignature>> =
        fetch_plugin::<BaseAssetV1, VerifiedCreators>(asset_info, PluginType::VerifiedCreators)
            .map(|(_, verified_creators, _)| verified_creators.signatures)
            .ok();

    Ok(CoreAsset {
        pubkey: *asset_info.key,
        collection,
        whitelist_creators: verified_creators,
        royalty_creators: royalties.map(|r| r.creators),
        royalty_fee_bps,
        royalty_enforced: true,
    })
}

/// Validates a mpl-core asset.
///
/// The validation consists of checking that the asset:
/// - is owned by mpl-core program
/// - is not burned
///
/// This function will return the royalties pulgin with the information
/// of the basis_points and creators.
pub fn validate_asset(
    nft_asset: &AccountInfo,
    nft_collection: Option<&AccountInfo>,
) -> Result<Option<Royalties>> {
    // validate the asset account
    assert_ownership(nft_asset, Key::AssetV1)?;

    if let Ok((_, plugin, _)) =
        fetch_plugin::<BaseAssetV1, Royalties>(nft_asset, PluginType::Royalties)
    {
        // if we have a royalties plugin on the asset, it will take
        // precedence over the one in the collection even if one is
        // present
        return Ok(Some(plugin));
    }

    let asset = BaseAssetV1::try_from(nft_asset)?;
    // if the asset has a collection, we must validate it
    if let UpdateAuthority::Collection(c) = asset.update_authority {
        if let Some(collection) = nft_collection {
            if c != *collection.key {
                msg!("Asset collection account does not match the provided collection account");
                return Err(TensorError::InvalidCoreAsset.into());
            }
            // validates the collection
            assert_ownership(collection, Key::CollectionV1)?;

            if let Ok((_, plugin, _)) =
                fetch_plugin::<BaseCollectionV1, Royalties>(collection, PluginType::Royalties)
            {
                return Ok(Some(plugin));
            }
        } else {
            msg!("Asset has a collection but no collection account was provided");
            return Err(TensorError::InvalidCoreAsset.into());
        }
    }

    // validation suceeded but no royalties plugin
    Ok(None)
}

#[inline(always)]
pub fn assert_ownership(account: &AccountInfo, discriminator: Key) -> Result<()> {
    if account.owner != &mpl_core::ID {
        return Err(TensorError::InvalidCoreAsset.into());
    }

    let data = account.data.borrow();

    if data.len() <= 1 || data[0] != discriminator as u8 {
        return Err(TensorError::InvalidCoreAsset.into());
    }

    Ok(())
}
