// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Code for the `#[dynamic_params]`, `#[dynamic_pallet_params]` and `#[dynamic_aggregated_params]`
//! macros.

use frame_support_procedural_tools::generate_access_from_frame_or_crate;
use inflector::Inflector;
use itertools::multiunzip;
use proc_macro2::TokenStream;
use quote::{format_ident, quote, ToTokens};
use syn::{parse2, token, Result, Token};

/// Parse and expand a `#[dynamic_params(..)]` module.
pub fn dynamic_params(attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
	DynamicParamModAttr::parse(attr, item).map(ToTokens::into_token_stream)
}

/// Parse and expand `#[dynamic_pallet_params(..)]` attribute.
pub fn dynamic_pallet_params(attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
	DynamicPalletParamAttr::parse(attr, item).map(ToTokens::into_token_stream)
}

/// Parse and expand `#[dynamic_aggregated_params]` attribute.
pub fn dynamic_aggregated_params(_attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
	parse2::<DynamicParamAggregatedEnum>(item).map(ToTokens::into_token_stream)
}

/// A top `#[dynamic_params(..)]` attribute together with a mod.
#[derive(derive_syn_parse::Parse)]
pub struct DynamicParamModAttr {
	params_mod: syn::ItemMod,
	meta: DynamicParamModAttrMeta,
}

/// The inner meta of a `#[dynamic_params(..)]` attribute.
#[derive(derive_syn_parse::Parse)]
pub struct DynamicParamModAttrMeta {
	name: syn::Ident,
}

impl DynamicParamModAttr {
	pub fn parse(attr: TokenStream, item: TokenStream) -> Result<Self> {
		let params_mod = parse2(item)?;
		let meta = parse2(attr)?;
		Ok(Self { params_mod, meta })
	}

	pub fn inner_mods(&self) -> Vec<syn::ItemMod> {
		self.params_mod.content.as_ref().map_or(Vec::new(), |(_, items)| {
			items
				.iter()
				.filter_map(|i| match i {
					syn::Item::Mod(m) => Some(m),
					_ => None,
				})
				.cloned()
				.collect()
		})
	}
}

impl ToTokens for DynamicParamModAttr {
	fn to_tokens(&self, tokens: &mut TokenStream) {
		let scrate = match crate_access() {
			Ok(path) => path,
			Err(err) => return tokens.extend(err),
		};
		let (params_mod, name) = (&self.params_mod, &self.meta.name);
		let dynam_params_ident = &params_mod.ident;

		let mut quoted_enum = quote! {};
		for m in self.inner_mods() {
			let aggregate_name =
				syn::Ident::new(&m.ident.to_string().to_class_case(), m.ident.span());
			let mod_name = &m.ident;

			quoted_enum.extend(quote! {
				#aggregate_name(#dynam_params_ident::#mod_name::Parameters),
			});
		}

		tokens.extend(quote! {
			#params_mod

			#[#scrate::dynamic_params::dynamic_aggregated_params]
			pub enum #name {
				#quoted_enum
			}
		});
	}
}

pub struct DynamicParamParametersMod {
	aggregate_name: String,
	mod_name: syn::Ident,
	parameter_name: syn::Ident,
}

/// A parsed `#[dynamic_pallet_params(..)]` attribute.
#[derive(derive_syn_parse::Parse)]
pub struct DynamicPalletParamModAttr {
	_pound: Token![#],
	#[bracket]
	_bracket: token::Bracket,
	#[inside(_bracket)]
	meta: DynamicPalletParamModAttrMeta,
}

mod keyword {
	syn::custom_keyword!(dynamic_pallet_params);
}

/// The inner meta of a `#[dynamic_pallet_params(..)]` attribute.
#[derive(derive_syn_parse::Parse)]
pub struct DynamicPalletParamModAttrMeta {
	_keyword: keyword::dynamic_pallet_params,
	#[paren]
	_paren: token::Paren,
	#[inside(_paren)]
	pallet_param_attr: DynamicPalletParamAttr,
}

/// The helper attribute of a `#[dynamic_pallet_params(parameter_pallet, parameter_name)]`
/// attribute.
#[derive(derive_syn_parse::Parse)]
pub struct DynamicPalletParamAttr {
	inner_mod: syn::ItemMod,
	parameter_pallet: syn::Type,
}

impl DynamicPalletParamAttr {
	pub fn parse(attr: TokenStream, item: TokenStream) -> Result<Self> {
		let inner_mod = parse2(item)?;
		let parameter_pallet = parse2(attr)?;

		Ok(Self { inner_mod, parameter_pallet })
	}

	pub fn statics(&self) -> Vec<syn::ItemStatic> {
		self.inner_mod.content.as_ref().map_or(Vec::new(), |(_, items)| {
			items
				.iter()
				.filter_map(|i| match i {
					syn::Item::Static(s) => Some(s),
					_ => None,
				})
				.cloned()
				.collect()
		})
	}
}

impl ToTokens for DynamicPalletParamAttr {
	fn to_tokens(&self, tokens: &mut TokenStream) {
		let scrate = match crate_access() {
			Ok(path) => path,
			Err(err) => return tokens.extend(err),
		};
		let (params_mod, parameter_pallet) = (&self.inner_mod, &self.parameter_pallet);

		let aggregate_name =
			syn::Ident::new(&params_mod.ident.to_string().to_class_case(), params_mod.ident.span());
		let (mod_name, vis) = (&params_mod.ident, &params_mod.vis);
		let statics = self.statics();

		let (key_names, key_values, defaults, attrs, value_types): (
			Vec<_>,
			Vec<_>,
			Vec<_>,
			Vec<_>,
			Vec<_>,
		) = multiunzip(statics.iter().map(|s| {
			(&s.ident, format_ident!("{}Value", s.ident), &s.expr, s.attrs.first(), &s.ty)
		}));

		let key_ident = syn::Ident::new("ParametersKey", params_mod.ident.span());
		let value_ident = syn::Ident::new("ParametersValue", params_mod.ident.span());

		tokens.extend(quote! {
			pub mod #mod_name {
				use super::*;

				#[doc(hidden)]
				#[derive(
					Clone,
					PartialEq,
					Eq,
					#scrate::__private::codec::Encode,
					#scrate::__private::codec::Decode,
					#scrate::__private::codec::MaxEncodedLen,
					#scrate::__private::RuntimeDebug,
					#scrate::__private::scale_info::TypeInfo
				)]
				#vis enum Parameters {
					#(
						#attrs
						#key_names(#key_names, Option<#value_types>),
					)*
				}

				#[doc(hidden)]
				#[derive(
					Clone,
					PartialEq,
					Eq,
					#scrate::__private::codec::Encode,
					#scrate::__private::codec::Decode,
					#scrate::__private::codec::MaxEncodedLen,
					#scrate::__private::RuntimeDebug,
					#scrate::__private::scale_info::TypeInfo
				)]
				#vis enum #key_ident {
					#(
						#attrs
						#key_names(#key_names),
					)*
				}

				#[doc(hidden)]
				#[derive(
					Clone,
					PartialEq,
					Eq,
					#scrate::__private::codec::Encode,
					#scrate::__private::codec::Decode,
					#scrate::__private::codec::MaxEncodedLen,
					#scrate::__private::RuntimeDebug,
					#scrate::__private::scale_info::TypeInfo
				)]
				#vis enum #value_ident {
					#(
						#attrs
						#key_names(#value_types),
					)*
				}

				impl #scrate::traits::AggregratedKeyValue for Parameters {
					type AggregratedKey = #key_ident;
					type AggregratedValue = #value_ident;

					fn into_parts(self) -> (Self::AggregratedKey, Option<Self::AggregratedValue>) {
						match self {
							#(
								Parameters::#key_names(key, value) => {
									(#key_ident::#key_names(key), value.map(#value_ident::#key_names))
								},
							)*
						}
					}
				}

				#(
					#[doc(hidden)]
					#[derive(
						Clone,
						PartialEq,
						Eq,
						#scrate::__private::codec::Encode,
						#scrate::__private::codec::Decode,
						#scrate::__private::codec::MaxEncodedLen,
						#scrate::__private::RuntimeDebug,
						#scrate::__private::scale_info::TypeInfo
					)]
					#vis struct #key_names;

					impl #scrate::__private::Get<#value_types> for #key_names {
						fn get() -> #value_types {
							match
								<#parameter_pallet as
									#scrate::storage::StorageMap<RuntimeParametersKey, RuntimeParametersValue>
								>::get(RuntimeParametersKey::#aggregate_name(#key_ident::#key_names(#key_names)))
							{
								Some(RuntimeParametersValue::#aggregate_name(
									#value_ident::#key_names(inner))) => inner,
								Some(_) => {
									#scrate::defensive!("Unexpected value type at key - returning default");
									#defaults
								},
								None => #defaults,
							}
						}
					}

					impl #scrate::traits::Key for #key_names {
						type Value = #value_types;
						type WrappedValue = #key_values;
					}

					impl From<#key_names> for #key_ident {
						fn from(key: #key_names) -> Self {
							#key_ident::#key_names(key)
						}
					}

					impl TryFrom<#key_ident> for #key_names {
						type Error = ();

						fn try_from(key: #key_ident) -> Result<Self, Self::Error> {
							match key {
								#key_ident::#key_names(key) => Ok(key),
								_ => Err(()),
							}
						}
					}

					#[doc(hidden)]
					#[derive(
						Clone,
						PartialEq,
						Eq,
						#scrate::sp_runtime::RuntimeDebug,
					)]
					#vis struct #key_values(pub #value_types);

					impl From<#key_values> for #value_ident {
						fn from(value: #key_values) -> Self {
							#value_ident::#key_names(value.0)
						}
					}

					impl From<(#key_names, #value_types)> for Parameters {
						fn from((key, value): (#key_names, #value_types)) -> Self {
							Parameters::#key_names(key, Some(value))
						}
					}

					impl From<#key_names> for Parameters {
						fn from(key: #key_names) -> Self {
							Parameters::#key_names(key, None)
						}
					}

					impl TryFrom<#value_ident> for #key_values {
						type Error = ();

						fn try_from(value: #value_ident) -> Result<Self, Self::Error> {
							match value {
								#value_ident::#key_names(value) => Ok(#key_values(value)),
								_ => Err(()),
							}
						}
					}

					impl From<#key_values> for #value_types {
						fn from(value: #key_values) -> Self {
							value.0
						}
					}
				)*
			}
		});
	}
}

#[derive(derive_syn_parse::Parse)]
pub struct DynamicParamAggregatedEnum {
	aggregated_enum: syn::ItemEnum,
}

impl ToTokens for DynamicParamAggregatedEnum {
	fn to_tokens(&self, tokens: &mut TokenStream) {
		let scrate = match crate_access() {
			Ok(path) => path,
			Err(err) => return tokens.extend(err),
		};
		let params_enum = &self.aggregated_enum;
		let (name, vis) = (&params_enum.ident, &params_enum.vis);

		let (mut indices, mut param_names, mut param_types): (Vec<_>, Vec<_>, Vec<_>) =
			Default::default();
		for (i, variant) in params_enum.variants.iter().enumerate() {
			indices.push(i);
			param_names.push(&variant.ident);

			param_types.push(match &variant.fields {
				syn::Fields::Unnamed(fields) if fields.unnamed.len() == 1 => &fields.unnamed[0].ty,
				_ => {
					*tokens = quote! { compile_error!("Only unnamed enum variants with one inner item are supported") };
					return
				},
			});
		}

		let params_key_ident = format_ident!("{}Key", params_enum.ident);
		let params_value_ident = format_ident!("{}Value", params_enum.ident);

		tokens.extend(quote! {
			#[doc(hidden)]
			#[derive(
				Clone,
				PartialEq,
				Eq,
				#scrate::__private::codec::Encode,
				#scrate::__private::codec::Decode,
				#scrate::__private::codec::MaxEncodedLen,
				#scrate::sp_runtime::RuntimeDebug,
				#scrate::__private::scale_info::TypeInfo
			)]
			#vis enum #name {
				#(
					#[codec(index = #indices)]
					#param_names(#param_types),
				)*
			}

			#[doc(hidden)]
			#[derive(
				Clone,
				PartialEq,
				Eq,
				#scrate::__private::codec::Encode,
				#scrate::__private::codec::Decode,
				#scrate::__private::codec::MaxEncodedLen,
				#scrate::sp_runtime::RuntimeDebug,
				#scrate::__private::scale_info::TypeInfo
			)]
			#vis enum #params_key_ident {
				#(
					#[codec(index = #indices)]
					#param_names(<#param_types as #scrate::traits::AggregratedKeyValue>::AggregratedKey),
				)*
			}

			#[doc(hidden)]
			#[derive(
				Clone,
				PartialEq,
				Eq,
				#scrate::__private::codec::Encode,
				#scrate::__private::codec::Decode,
				#scrate::__private::codec::MaxEncodedLen,
				#scrate::sp_runtime::RuntimeDebug,
				#scrate::__private::scale_info::TypeInfo
			)]
			#vis enum #params_value_ident {
				#(
					#[codec(index = #indices)]
					#param_names(<#param_types as #scrate::traits::AggregratedKeyValue>::AggregratedValue),
				)*
			}

			impl #scrate::traits::AggregratedKeyValue for #name {
				type AggregratedKey = #params_key_ident;
				type AggregratedValue = #params_value_ident;

				fn into_parts(self) -> (Self::AggregratedKey, Option<Self::AggregratedValue>) {
					match self {
						#(
							#name::#param_names(parameter) => {
								let (key, value) = parameter.into_parts();
								(#params_key_ident::#param_names(key), value.map(#params_value_ident::#param_names))
							},
						)*
					}
				}
			}

			#(
				impl ::core::convert::From<<#param_types as #scrate::traits::AggregratedKeyValue>::AggregratedKey> for #params_key_ident {
					fn from(key: <#param_types as #scrate::traits::AggregratedKeyValue>::AggregratedKey) -> Self {
						#params_key_ident::#param_names(key)
					}
				}

				impl ::core::convert::TryFrom<#params_value_ident> for <#param_types as #scrate::traits::AggregratedKeyValue>::AggregratedValue {
					type Error = ();

					fn try_from(value: #params_value_ident) -> Result<Self, Self::Error> {
						match value {
							#params_value_ident::#param_names(value) => Ok(value),
							_ => Err(()),
						}
					}
				}
			)*
		});
	}
}

/// Get access to the current crate and convert the error to a compile error.
fn crate_access() -> core::result::Result<syn::Path, TokenStream> {
	generate_access_from_frame_or_crate("frame-support").map_err(|e| e.to_compile_error())
}

#[test]
fn test_mod_attr_parser() {
	let attr = quote! {
		#[dynamic_pallet_params(pallet_parameters::Parameters::<Test>, Basic)]
	};
	let attr = syn::parse2::<DynamicPalletParamModAttr>(attr).unwrap();
	assert_eq!(attr.meta.pallet_param_attr.parameter_name.to_string(), "Basic");
}
