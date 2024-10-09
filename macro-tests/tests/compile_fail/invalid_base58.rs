use solana_program::pubkey::Pubkey;
use solana_pubkey_macro::pubkey;

fn main() {
    let _pubkey: Pubkey = pubkey!("Not a valid base58 string"); // Should fail: invalid base58
}
