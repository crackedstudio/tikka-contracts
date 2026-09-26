#c[cfg(test)]
//! Integration-test module root for the raffle instance.  This file re-exports
//! `crate::*`, sets up shared testutils (budget, events, ledger, register,
//! StellarAssetClient), and declares the submodules `budget`, `fairness`, etc.

extern crate std;
use std::vec;

use crate::*;
use soroban_sdk::{
	testutils::{budget::Budget, Address as _, Events, Ledger, Register},
	token::StellarAssetClient,
	Address, BytesN, Env, String,
};
use crate::events;

pub mod budget;
pub mod fairness;
pub mod draw;
pub mod invariants;
pub mod ttl;
