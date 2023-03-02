//! Methods for working with pyth accounts

use {
    crate::account::{AccountSharedData, WritableAccount},
    solana_program::pubkey::Pubkey,
    solana_sdk_macro::pubkey,
};

/// Pubkey::find_program_address(&[b"emitter"], &sysvar::accumulator::id());
pub const ACCUMULATOR_EMITTER_ADDR: Pubkey = pubkey!("G9LV2mp9ua1znRAfYwZz5cPiJMAbo1T6mbjdQsDZuMJg");
/// Pubkey::find_program_address(&[b"Sequence", &emitter_pda_key.to_bytes()], &WORMHOLE_PID);
pub const ACCUMULATOR_SEQUENCE_ADDR: Pubkey = pubkey!("HiqU8jiyUoFbRjf4YFAKRFWq5NZykEYC6mWhXXnoszJR");
pub const PYTH_PID: Pubkey = pubkey!("FsJ3A3u2vn5cTVofAjvy6y5kwABJAqYWpe4975bi2epH");

pub mod price_proofs {
    use super::*;
    use solana_pyth::accumulators::merkle::PriceProofs;

    pub fn to_account(price_proof: &PriceProofs, account: &mut AccountSharedData) -> Option<()> {
        bincode::serialize_into(account.data_as_mut_slice(), price_proof).ok()
    }

    pub fn create_account(
        price_proof: &PriceProofs,
        data_len: usize,
        lamports: u64,
        owner: &Pubkey,
    ) -> AccountSharedData {
        let mut account = AccountSharedData::new(lamports, data_len, owner);
        to_account(price_proof, &mut account).unwrap();
        // TODO: what to set for rent epoch here
        // account.rent_epoch = rent_epoch;
        account
    }
}

pub mod wormhole {
    use super::*;
    use crate::account::Account;
    use borsh::BorshSerialize;
    use solana_pyth::wormhole::{AccumulatorSequenceTracker, PostedMessageUnreliableData};
    use solana_sdk_macro::pubkey;

    pub const WORMHOLE_PID: Pubkey = pubkey!("worm2ZoG2kUd4vFXhvjh93UUH596ayRfgQ2MgjNMTth");

    pub fn create_account(
        message_data: PostedMessageUnreliableData,
        data_len: usize,
        lamports: u64,
        owner: &Pubkey,
    ) -> AccountSharedData {
        let mut account = AccountSharedData::new(lamports, data_len, owner);
        account.set_data(message_data.try_to_vec().unwrap());
        // TODO: what to set for rent epoch here
        // account.rent_epoch = rent_epoch;
        account
    }

    pub fn create_seq_tracker_account(
        sequence_tracker_account: AccumulatorSequenceTracker,
        data_len: usize,
        lamports: u64,
        owner: &Pubkey,
    ) -> AccountSharedData {
        let mut account = AccountSharedData::new(lamports, data_len, owner);
        account.set_data(sequence_tracker_account.try_to_vec().unwrap());
        account
    }
}
