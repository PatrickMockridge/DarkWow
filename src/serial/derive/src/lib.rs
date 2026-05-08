/* This file is part of DarkWow
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * DarkWow is a tool for people and nations to establish sovereignty
 * according to human rights law. See the UN Declaration on the Rights
 * of Indigenous Peoples and associated documents:
 * https://documents.un.org/doc/undoc/gen/g26/031/70/pdf/g2603170.pdf
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

extern crate proc_macro;
use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use proc_macro_crate::{crate_name, FoundCrate};
use quote::quote;
use syn::{Ident, ItemEnum, ItemStruct, ItemUnion};

#[cfg(feature = "async")]
use dwow_derive_internal::{async_enum_de, async_enum_ser, async_struct_de, async_struct_ser};

use dwow_derive_internal::{enum_de, enum_ser, struct_de, struct_ser};

#[proc_macro_derive(SerialEncodable, attributes(skip_serialize))]
pub fn darkfi_serialize(input: TokenStream) -> TokenStream {
    let found_crate = crate_name("dwow-serial").expect("dwow-serial is found in Cargo.toml");

    let found_crate = match found_crate {
        FoundCrate::Name(name) => name,
        FoundCrate::Itself => "crate".to_string(),
    };

    let cratename = Ident::new(&found_crate, Span::call_site());

    let res: syn::Result<TokenStream2> = if let Ok(input) = syn::parse::<ItemStruct>(input.clone())
    {
        let sync_tokens = struct_ser(&input, cratename.clone()).unwrap();
        #[cfg(feature = "async")]
        let async_tokens = async_struct_ser(&input, cratename).unwrap();
        #[cfg(not(feature = "async"))]
        let async_tokens = quote! {};

        Ok(quote! {
            #sync_tokens
            #async_tokens
        })
    } else if let Ok(input) = syn::parse::<ItemEnum>(input.clone()) {
        let sync_tokens = enum_ser(&input, cratename.clone()).unwrap();
        #[cfg(feature = "async")]
        let async_tokens = async_enum_ser(&input, cratename).unwrap();
        #[cfg(not(feature = "async"))]
        let async_tokens = quote! {};

        Ok(quote! {
            #sync_tokens
            #async_tokens
        })
    } else if let Ok(_input) = syn::parse::<ItemUnion>(input) {
        todo!()
    } else {
        // Derive macros can only be defined on structs, enums, and unions.
        unreachable!()
    };

    TokenStream::from(res.unwrap_or_else(|err| err.to_compile_error()))
}

#[proc_macro_derive(SerialDecodable, attributes(skip_serialize))]
pub fn darkfi_deserialize(input: TokenStream) -> TokenStream {
    let found_crate = crate_name("dwow-serial").expect("dwow-serial is found in Cargo.toml");

    let found_crate = match found_crate {
        FoundCrate::Name(name) => name,
        FoundCrate::Itself => "crate".to_string(),
    };

    let cratename = Ident::new(&found_crate, Span::call_site());

    let res: syn::Result<TokenStream2> = if let Ok(input) = syn::parse::<ItemStruct>(input.clone())
    {
        let sync_tokens = struct_de(&input, cratename.clone()).unwrap();
        #[cfg(feature = "async")]
        let async_tokens = async_struct_de(&input, cratename).unwrap();
        #[cfg(not(feature = "async"))]
        let async_tokens = quote! {};

        Ok(quote! {
            #sync_tokens
            #async_tokens
        })
    } else if let Ok(input) = syn::parse::<ItemEnum>(input.clone()) {
        let sync_tokens = enum_de(&input, cratename.clone()).unwrap();
        #[cfg(feature = "async")]
        let async_tokens = async_enum_de(&input, cratename).unwrap();
        #[cfg(not(feature = "async"))]
        let async_tokens = quote! {};

        Ok(quote! {
            #sync_tokens
            #async_tokens
        })
    } else if let Ok(_input) = syn::parse::<ItemUnion>(input) {
        todo!()
    } else {
        // Derive macros can only be defined on structs, enums, and unions.
        unreachable!()
    };

    TokenStream::from(res.unwrap_or_else(|err| err.to_compile_error()))
}
