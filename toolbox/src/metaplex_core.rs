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

/// Core asset information extracted from the MPL Core program plugins.
/// Used by Tensor protocols to validate whitelist conditions and pay out royalties in appropriate cases.
pub struct CoreAsset {
    pub pubkey: Pubkey,
    pub collection: Option<Pubkey>,
    pub whitelist_creators: Option<Vec<VerifiedCreatorsSignature>>,
    pub royalty_creators: Option<Vec<Creator>>,
    pub royalty_fee_bps: u16,
    pub royalty_enforced: bool,
}

/// Validates a mpl-core asset.
///
/// Ensures the asset and collection, if passed in are:
/// - owned by mpl-core program
/// - not burned
/// - correct discriminator
/// - collection matches the one stored on the asset
///
/// Extracts royalty and verified creators information from the appropriate plugins.
pub fn validate_core_asset(
    asset_info: &AccountInfo,
    maybe_collection_info: Option<&AccountInfo>,
) -> Result<CoreAsset> {
    // validate the asset account
    assert_ownership(asset_info, Key::AssetV1)?;

    // validates the collection is owned by the MPL Core program
    if let Some(collection_info) = maybe_collection_info {
        assert_ownership(collection_info, Key::CollectionV1)?;
    }

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
