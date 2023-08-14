use anchor_lang::{
    prelude::*,
};
use mpl_bubblegum::{
    self,
    state::{
        metaplex_adapter::{
            Collection, Creator, MetadataArgs, TokenProgramVersion, TokenStandard, UseMethod, Uses,
        },
    },
};

#[derive(AnchorSerialize, AnchorDeserialize, PartialEq, Eq, Debug, Clone)]
pub enum TTokenProgramVersion {
    Original,
    Token2022,
}

impl From<TTokenProgramVersion> for TokenProgramVersion {
    fn from(v: TTokenProgramVersion) -> Self {
        match v {
            TTokenProgramVersion::Original => TokenProgramVersion::Original,
            TTokenProgramVersion::Token2022 => TokenProgramVersion::Token2022,
        }
    }
}

#[repr(u8)]
#[derive(AnchorSerialize, AnchorDeserialize, PartialEq, Eq, Debug, Clone)]
pub enum TTokenStandard {
    NonFungible = 0,        // This is a master edition
    FungibleAsset = 1,      // A token with metadata that can also have attrributes
    Fungible = 2,           // A token with simple metadata
    NonFungibleEdition = 3, // This is a limited edition
}

impl From<TTokenStandard> for TokenStandard {
    fn from(s: TTokenStandard) -> Self {
        match s {
            TTokenStandard::NonFungible => TokenStandard::NonFungible,
            TTokenStandard::FungibleAsset => TokenStandard::FungibleAsset,
            TTokenStandard::Fungible => TokenStandard::Fungible,
            TTokenStandard::NonFungibleEdition => TokenStandard::NonFungibleEdition,
        }
    }
}

#[derive(AnchorSerialize, AnchorDeserialize, PartialEq, Eq, Debug, Clone)]
pub enum TUseMethod {
    Burn,
    Multiple,
    Single,
}

impl From<TUseMethod> for UseMethod {
    fn from(m: TUseMethod) -> Self {
        match m {
            TUseMethod::Burn => UseMethod::Burn,
            TUseMethod::Multiple => UseMethod::Multiple,
            TUseMethod::Single => UseMethod::Single,
        }
    }
}

#[derive(AnchorSerialize, AnchorDeserialize, PartialEq, Eq, Debug, Clone)]
pub struct TUses {
    // 17 bytes + Option byte
    pub use_method: TUseMethod,
    pub remaining: u64,
    pub total: u64,
}

impl From<TUses> for Uses {
    fn from(u: TUses) -> Self {
        Self {
            use_method: UseMethod::from(u.use_method),
            remaining: u.remaining,
            total: u.total,
        }
    }
}

#[repr(C)]
#[derive(AnchorSerialize, AnchorDeserialize, PartialEq, Eq, Debug, Clone)]
pub struct TCollection {
    pub verified: bool,
    pub key: Pubkey,
}

impl From<TCollection> for Collection {
    fn from(c: TCollection) -> Self {
        Self {
            verified: c.verified,
            key: c.key,
        }
    }
}

#[derive(AnchorSerialize, AnchorDeserialize, PartialEq, Eq, Debug, Clone)]
pub struct TMetadataArgs {
    /// The name of the asset
    pub name: String,
    /// The symbol for the asset
    pub symbol: String,
    /// URI pointing to JSON representing the asset
    pub uri: String,
    /// Royalty basis points that goes to creators in secondary sales (0-10000)
    pub seller_fee_basis_points: u16,
    // Immutable, once flipped, all sales of this metadata are considered secondary.
    pub primary_sale_happened: bool,
    // Whether or not the data struct is mutable, default is not
    pub is_mutable: bool,
    /// nonce for easy calculation of editions, if present
    pub edition_nonce: Option<u8>,
    /// Since we cannot easily change Metadata, we add the new DataV2 fields here at the end.
    pub token_standard: Option<TTokenStandard>,
    /// Collection
    pub collection: Option<TCollection>,
    /// Uses
    pub uses: Option<TUses>,
    pub token_program_version: TTokenProgramVersion,
    // Metadata with creators array simple as []. instead pass shares & verified separately below
    // So that we're not duplicating creator keys (space in the tx)
    pub creator_shares: Vec<u8>,
    pub creator_verified: Vec<bool>,
}

impl TMetadataArgs {
    // Can't use the default from trait because need extra arg
    pub fn into(self, creators: &[AccountInfo]) -> MetadataArgs {
        MetadataArgs {
            name: self.name.clone(),
            symbol: self.symbol.clone(),
            uri: self.uri.clone(),
            seller_fee_basis_points: self.seller_fee_basis_points.clone(),
            primary_sale_happened: self.primary_sale_happened.clone(),
            is_mutable: self.is_mutable.clone(),
            edition_nonce: self.edition_nonce.clone(),
            token_standard: self.token_standard.clone().map(TokenStandard::from),
            collection: self.collection.clone().map(Collection::from),
            uses: self.uses.clone().map(Uses::from),
            token_program_version: TokenProgramVersion::from(self.token_program_version.clone()),
            creators: creators
                .iter()
                .enumerate()
                .map(|(i, c)| Creator {
                    address: c.key(),
                    verified: self.creator_verified[i],
                    share: self.creator_shares[i],
                })
                .collect::<Vec<_>>(),
        }
    }
}
