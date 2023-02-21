//! Methods for working with price_proof accounts

pub use solana_program::accumulator::*;
use {
    crate::account::{AccountSharedData, WritableAccount},
    solana_program::pubkey::Pubkey,
};

/// Serialize a `PriceProof` into an `Account`'s data.
pub fn to_account(price_proof: &DummyPriceProof, account: &mut AccountSharedData) -> Option<()> {
    bincode::serialize_into(account.data_as_mut_slice(), price_proof).ok()
}

pub fn create_account(
    price_proof: &DummyPriceProof,
    lamports: u64,
    owner: &Pubkey,
) -> AccountSharedData {
    // let data_len = PriceProof::size_of().max(bincode::serialized_size(price_proof).unwrap() as usize);
    let data_len = bincode::serialized_size(&price_proof).unwrap() as usize;

    let mut account = AccountSharedData::new(lamports, data_len, owner);
    to_account(price_proof, &mut account).unwrap();
    // TODO: what to set for rent epoch here
    // account.rent_epoch = rent_epoch;
    account
}

pub fn create_account2(
    price_proof: &PriceProofs,
    lamports: u64,
    owner: &Pubkey,
) -> AccountSharedData {
    // let data_len = PriceProof::size_of().max(bincode::serialized_size(price_proof).unwrap() as usize);
    let data_len = bincode::serialized_size(&price_proof).unwrap() as usize;

    let mut account = AccountSharedData::new(lamports, data_len, owner);
    to_account2(price_proof, &mut account).unwrap();
    // TODO: what to set for rent epoch here
    // account.rent_epoch = rent_epoch;
    account
}

pub fn to_account2(price_proof: &PriceProofs, account: &mut AccountSharedData) -> Option<()> {
    bincode::serialize_into(account.data_as_mut_slice(), price_proof).ok()
}

// pub fn create_accumulator_vaa_account(
//     posted_message: &PostedMessageUnreliableData,
//     lamports: u64,
//     owner: &Pubkey,
// ) -> AccountSharedData {
//     AccountSharedData::new(lamports, 10, owner);
// }
//
// pub fn to_accumulator_vaa_account(
//     posted_message: &PostedMessageUnreliableData,
//     account: &mut AccountSharedData,
// ) -> Option<()> {
//     posted_message
//         .serialize(&mut account.data_as_mut_slice())
//         .ok()
// }
