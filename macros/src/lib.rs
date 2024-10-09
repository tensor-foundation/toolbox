use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, LitStr};

#[proc_macro]
pub fn pubkey(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as LitStr);
    let base58_str = input.value();

    let decoded = bs58::decode(base58_str)
        .into_vec()
        .expect("Failed to decode base58 string");

    if decoded.len() != 32 {
        panic!(
            "Invalid Pubkey length: expected 32 bytes, got {}",
            decoded.len()
        );
    }

    let bytes: Vec<u8> = decoded;

    let expanded = quote! {
        Pubkey::new_from_array([
            #(#bytes),*
        ])
    };

    TokenStream::from(expanded)
}
