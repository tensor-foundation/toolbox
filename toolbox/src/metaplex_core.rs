use anchor_lang::solana_program::account_info::AccountInfo;
use anchor_lang::solana_program::msg;
use anchor_lang::solana_program::pubkey::Pubkey;
use anchor_lang::Result;
use mpl_core::accounts::{BaseAssetV1, BaseCollectionV1};
use mpl_core::fetch_plugin;
use mpl_core::types::{Key, PluginType, Royalties, UpdateAuthority};

use crate::TensorError;

#[derive(Clone)]
pub struct MetaplexCore;

impl anchor_lang::Id for MetaplexCore {
    fn id() -> Pubkey {
        mpl_core::ID
    }
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
fn assert_ownership(account: &AccountInfo, discriminator: Key) -> Result<()> {
    if account.owner != &mpl_core::ID {
        return Err(TensorError::InvalidCoreAsset.into());
    }

    let data = account.data.borrow();

    if data.len() <= 1 || data[0] != discriminator as u8 {
        return Err(TensorError::InvalidCoreAsset.into());
    }

    Ok(())
}
