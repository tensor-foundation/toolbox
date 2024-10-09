use solana_program::pubkey::Pubkey;
use tensor_macros::pubkey;

fn main() {
    let invalid = "11111111111111111111111111111111";
    let _pubkey: Pubkey = pubkey!(invalid); // Should fail: not a string literal
}
