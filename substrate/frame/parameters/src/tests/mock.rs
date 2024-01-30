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

#![cfg(test)]

//! Test that the `pallet_example_basic` can use the parameters pallet as storage.

use frame_support::{
	construct_runtime, derive_impl,
	dynamic_params::{dynamic_pallet_params, dynamic_params},
	traits::EnsureOriginWithArg,
};

use crate::*;

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl frame_system::Config for Runtime {
	type Block = frame_system::mocking::MockBlock<Runtime>;
	type AccountData = pallet_balances::AccountData<<Self as pallet_balances::Config>::Balance>;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig as pallet_balances::DefaultConfig)]
impl pallet_balances::Config for Runtime {
	type ReserveIdentifier = [u8; 8];
	type AccountStore = System;
}

#[docify::export]
#[dynamic_params(RuntimeParameters, crate::Parameters::<Runtime>)]
pub mod dynamic_params {
	use super::*;

	#[dynamic_pallet_params]
	pub mod pallet1 {
		#[codec(index = 0)]
		pub static Key1: u64 = 0;
		#[codec(index = 1)]
		pub static Key2: u32 = 1;
		#[codec(index = 2)]
		pub static Key3: u128 = 2;
	}

	#[dynamic_pallet_params]
	pub mod pallet2 {
		#[codec(index = 0)]
		pub static Key1: u64 = 0;
		#[codec(index = 1)]
		pub static Key2: u32 = 2;
		#[codec(index = 2)]
		pub static Key3: u128 = 4;
	}
}

#[docify::export]
mod custom_origin {
	use super::*;
	pub struct ParamsManager;

	impl EnsureOriginWithArg<RuntimeOrigin, RuntimeParametersKey> for ParamsManager {
		type Success = ();

		fn try_origin(
			origin: RuntimeOrigin,
			key: &RuntimeParametersKey,
		) -> Result<Self::Success, RuntimeOrigin> {
			// Account 123 is allowed to set parameters in benchmarking only:
			#[cfg(feature = "runtime-benchmarks")]
			if ensure_signed(origin.clone()).map_or(false, |acc| acc == 123) {
				return Ok(());
			}

			match key {
				RuntimeParametersKey::Pallet1(_) => ensure_root(origin.clone()),
				RuntimeParametersKey::Pallet2(_) => ensure_signed(origin.clone()).map(|_| ()),
			}
			.map_err(|_| origin)
		}

		#[cfg(feature = "runtime-benchmarks")]
		fn try_successful_origin(_key: &RuntimeParametersKey) -> Result<RuntimeOrigin, ()> {
			Ok(RuntimeOrigin::signed(123))
		}
	}
}

#[docify::export(impl_config)]
impl Config for Runtime {
	type AggregratedKeyValue = RuntimeParameters;
	type AdminOrigin = custom_origin::ParamsManager;

	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = ();
}

#[docify::export(usage)]
impl pallet_example_basic::Config for Runtime {
	// Use the dynamic key in the pallet config:
	type MagicNumber = dynamic_params::pallet1::Key1;

	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = ();
}

construct_runtime!(
	pub enum Runtime {
		System: frame_system,
		PalletParameters: crate,
		Example: pallet_example_basic,
		Balances: pallet_balances,
	}
);

pub fn new_test_ext() -> sp_io::TestExternalities {
	sp_io::TestExternalities::new(Default::default())
}
