use crate::{
    bank::{
        pyth_accumulator::{get_accumulator_keys, ACCUMULATOR_RING_SIZE, ORACLE_PID},
        Bank,
    },
    genesis_utils::{create_genesis_config_with_leader, GenesisConfigInfo},
};
use byteorder::ByteOrder;
use byteorder::{LittleEndian, ReadBytesExt};
use itertools::Itertools;
use pyth_oracle::PythOracleSerialize;
use pyth_oracle::{solana_program::account_info::AccountInfo, PriceAccountFlags};
use pyth_oracle::{PriceAccount, PythAccount};
use pythnet_sdk::{
    accumulators::{merkle::MerkleAccumulator, Accumulator},
    hashers::{keccak256_160::Keccak160, Hasher},
    wormhole::{AccumulatorSequenceTracker, MessageData, PostedMessageUnreliableData},
    ACCUMULATOR_EMITTER_ADDRESS,
};
use solana_sdk::{
    account::{AccountSharedData, ReadableAccount, WritableAccount},
    borsh::{BorshDeserialize, BorshSerialize},
    clock::Epoch,
    epoch_schedule::EpochSchedule,
    feature::{self, Feature},
    feature_set,
    hash::hashv,
    pubkey::Pubkey,
    signature::keypair_from_seed,
    signer::Signer,
};
use std::{io::Read, mem::size_of, sync::Arc};

// Create Message Account Bytes
//
// NOTE: This was serialized by hand, but should be replaced with the pythnet-sdk
// serializer once implemented.
fn create_message_buffer_bytes(msgs: Vec<Vec<u8>>) -> Vec<u8> {
    let mut buffer = Vec::new();
    let preimage = b"account:MessageBuffer";
    buffer.extend_from_slice(&hashv(&[preimage]).to_bytes()[..8]);
    buffer.extend_from_slice(&[0, 1, 10, 2]);
    let mut sums: Vec<u16> = msgs.iter().map(|m| m.len() as u16).collect();
    sums.resize(255, 0u16);
    buffer.extend(
        sums.into_iter()
            .scan(0, |acc, v| {
                *acc += v;
                Some(if v == 0 { v } else { *acc }.to_le_bytes())
            })
            .flatten(),
    );
    buffer.extend(msgs.into_iter().flatten());
    buffer
}

fn get_acc_sequence_tracker(bank: &Bank) -> AccumulatorSequenceTracker {
    let account = bank
        .get_account(&Pubkey::new_from_array(
            pythnet_sdk::pythnet::ACCUMULATOR_SEQUENCE_ADDR,
        ))
        .unwrap();
    AccumulatorSequenceTracker::try_from_slice(&mut account.data()).unwrap()
}

fn get_wormhole_message_account(bank: &Bank, ring_index: u32) -> AccountSharedData {
    let (wormhole_message_pubkey, _bump) = Pubkey::find_program_address(
        &[b"AccumulatorMessage", &ring_index.to_be_bytes()],
        &Pubkey::new_from_array(pythnet_sdk::pythnet::WORMHOLE_PID),
    );
    bank.get_account(&wormhole_message_pubkey)
        .unwrap_or_default()
}

fn get_accumulator_state(bank: &Bank, ring_index: u32) -> Vec<u8> {
    let (accumulator_state_pubkey, _) = Pubkey::find_program_address(
        &[b"AccumulatorState", &ring_index.to_be_bytes()],
        &solana_sdk::system_program::id(),
    );

    let account = bank.get_account(&accumulator_state_pubkey).unwrap();
    account.data().to_vec()
}

#[test]
fn test_update_accumulator_sysvar() {
    let leader_pubkey = solana_sdk::pubkey::new_rand();
    let GenesisConfigInfo {
        mut genesis_config, ..
    } = create_genesis_config_with_leader(5, &leader_pubkey, 3);

    // The genesis create function uses `Develompent` mode which enables all feature flags, so
    // we need to remove the accumulator sysvar in order to test the validator behaves
    // correctly when the feature is disabled. We will re-enable it further into this test.
    genesis_config
        .accounts
        .remove(&feature_set::enable_accumulator_sysvar::id())
        .unwrap();
    genesis_config
        .accounts
        .remove(&feature_set::move_accumulator_to_end_of_block::id())
        .unwrap();

    // Set epoch length to 32 so we can advance epochs quickly. We also skip past slot 0 here
    // due to slot 0 having special handling.
    let slots_in_epoch = 32;
    genesis_config.epoch_schedule = EpochSchedule::new(slots_in_epoch);
    let mut bank = Bank::new_for_tests(&genesis_config);
    bank = new_from_parent(&Arc::new(bank));
    bank = new_from_parent(&Arc::new(bank));

    let message_0 = vec![1u8; 127];
    let message_1 = vec![2u8; 127];
    // insert into message buffer in reverse order to test that accumulator
    // sorts first
    let messages = vec![message_1, message_0];

    let message_buffer_bytes = create_message_buffer_bytes(messages.clone());

    // Create a Message account.
    let price_message_key = keypair_from_seed(&[1u8; 32]).unwrap();
    let mut price_message_account = bank
        .get_account(&price_message_key.pubkey())
        .unwrap_or_default();
    price_message_account.set_lamports(1_000_000_000);
    price_message_account.set_owner(Pubkey::new_from_array(pythnet_sdk::MESSAGE_BUFFER_PID));
    price_message_account.set_data(message_buffer_bytes);

    // Store Message account so the accumulator sysvar updater can find it.
    bank.store_account(&price_message_key.pubkey(), &price_message_account);

    let (price_feed_key, _bump) = Pubkey::find_program_address(&[b"123"], &ORACLE_PID);
    let mut price_feed_account = AccountSharedData::new(42, size_of::<PriceAccount>(), &ORACLE_PID);
    let _ = PriceAccount::initialize(
        &AccountInfo::new(
            &price_feed_key.to_bytes().into(),
            false,
            true,
            &mut 0,
            &mut price_feed_account.data_mut(),
            &ORACLE_PID.to_bytes().into(),
            false,
            Epoch::default(),
        ),
        0,
    )
    .unwrap();
    bank.store_account(&price_feed_key, &price_feed_account);

    // Derive the Wormhole Message Account that will be generated by the sysvar updater.
    let (wormhole_message_pubkey, _bump) = Pubkey::find_program_address(
        &[b"AccumulatorMessage", &(bank.slot() as u32).to_be_bytes()],
        &Pubkey::new_from_array(pythnet_sdk::pythnet::WORMHOLE_PID),
    );

    // Account Data should be empty at this point. Check account data is [].
    let wormhole_message_account = bank
        .get_account(&wormhole_message_pubkey)
        .unwrap_or_default();
    assert_eq!(wormhole_message_account.data().len(), 0);

    // Run accumulator by creating a new bank from parent,the feature is
    // disabled so account data should still be empty. Check account data is
    // still [].
    bank = new_from_parent(&Arc::new(bank));

    let wormhole_message_account = bank
        .get_account(&wormhole_message_pubkey)
        .unwrap_or_default();
    assert_eq!(
        bank.feature_set
            .is_active(&feature_set::enable_accumulator_sysvar::id()),
        false
    );
    assert_eq!(wormhole_message_account.data().len(), 0);

    // Enable Accumulator Feature (42 = random lamport balance, and the meaning of the universe).
    let feature_id = feature_set::enable_accumulator_sysvar::id();
    let feature = Feature {
        activated_at: Some(30),
    };
    bank.store_account(&feature_id, &feature::create_account(&feature, 42));
    bank.compute_active_feature_set(true);
    for _ in 0..slots_in_epoch {
        bank = new_from_parent(&Arc::new(bank));
    }

    // Feature should now be enabled on the new bank as the epoch has changed.
    assert_eq!(
        bank.feature_set
            .is_active(&feature_set::enable_accumulator_sysvar::id()),
        true
    );

    // The current sequence value will be used in the message when the bank advances, so we snapshot
    // it here before advancing the slot so we can assert the correct sequence is present in the message.
    let sequence_tracker_before_bank_advance = get_acc_sequence_tracker(&bank);
    bank = new_from_parent(&Arc::new(bank));

    // get the timestamp & slot for the message
    let ring_index = (bank.slot() % ACCUMULATOR_RING_SIZE as u64) as u32;
    let wormhole_message_account = get_wormhole_message_account(&bank, ring_index);

    assert_ne!(wormhole_message_account.data().len(), 0);

    let wormhole_message =
        PostedMessageUnreliableData::deserialize(&mut wormhole_message_account.data()).unwrap();

    let messages = messages.iter().map(|m| m.as_slice()).collect::<Vec<_>>();
    let accumulator_elements = messages.clone().into_iter().sorted_unstable().dedup();
    let expected_accumulator =
        MerkleAccumulator::<Keccak160>::from_set(accumulator_elements).unwrap();
    let expected_wormhole_message_payload =
        expected_accumulator.serialize(bank.slot(), ACCUMULATOR_RING_SIZE);
    assert_eq!(
        wormhole_message.message.payload,
        expected_wormhole_message_payload
    );

    let expected_wormhole_message = PostedMessageUnreliableData {
        message: MessageData {
            vaa_version: 1,
            consistency_level: 1,
            submission_time: bank.clock().unix_timestamp as u32,
            sequence: sequence_tracker_before_bank_advance.sequence, // sequence is incremented after the message is processed
            emitter_chain: 26,
            emitter_address: ACCUMULATOR_EMITTER_ADDRESS,
            payload: expected_wormhole_message_payload,
            ..Default::default()
        },
    };

    assert_eq!(
        wormhole_message_account.data().to_vec(),
        expected_wormhole_message.try_to_vec().unwrap()
    );

    // verify hashes verify in accumulator
    for msg in messages {
        let msg_hash = Keccak160::hashv(&[[0u8].as_ref(), msg]);
        let msg_proof = expected_accumulator.prove(msg).unwrap();

        assert!(expected_accumulator.nodes.contains(&msg_hash));
        assert!(expected_accumulator.check(msg_proof, msg));
    }

    // verify accumulator state account
    let accumulator_state = get_accumulator_state(&bank, ring_index);
    let acc_state_magic = &accumulator_state[..4];
    let acc_state_slot = LittleEndian::read_u64(&accumulator_state[4..12]);
    let acc_state_ring_size = LittleEndian::read_u32(&accumulator_state[12..16]);

    assert_eq!(acc_state_magic, b"PAS1");
    assert_eq!(acc_state_slot, bank.slot());
    assert_eq!(acc_state_ring_size, ACCUMULATOR_RING_SIZE);

    let mut cursor = std::io::Cursor::new(&accumulator_state[16..]);
    let num_elems = cursor.read_u32::<LittleEndian>().unwrap();
    for _ in 0..(num_elems as usize) {
        let element_len = cursor.read_u32::<LittleEndian>().unwrap();
        let mut element_data = vec![0u8; element_len as usize];
        cursor.read_exact(&mut element_data).unwrap();

        let elem_hash = Keccak160::hashv(&[[0u8].as_ref(), element_data.as_slice()]);
        let elem_proof = expected_accumulator.prove(element_data.as_slice()).unwrap();

        assert!(expected_accumulator.nodes.contains(&elem_hash));
        assert!(expected_accumulator.check(elem_proof, element_data.as_slice()));
    }

    // verify sequence_tracker increments
    assert_eq!(
        get_acc_sequence_tracker(&bank).sequence,
        sequence_tracker_before_bank_advance.sequence + 1
    );

    // verify ring buffer cycles
    let ring_index_before_buffer_cycle = (bank.slot() % ACCUMULATOR_RING_SIZE as u64) as u32;
    let target_slot = bank.slot() + ACCUMULATOR_RING_SIZE as u64;
    // advance ACCUMULATOR_RING_SIZE slots using warp_from_parent since doing large loops
    // with new_from_parent takes a long time. warp_from_parent results in a bank that is frozen.
    bank = Bank::warp_from_parent(&Arc::new(bank), &Pubkey::default(), target_slot);

    // accumulator messages should still be the same before looping around
    let ring_index_after_buffer_cycle = (bank.slot() % ACCUMULATOR_RING_SIZE as u64) as u32;
    assert_eq!(
        ring_index_before_buffer_cycle,
        ring_index_after_buffer_cycle
    );

    let accumulator_state_after_skip = get_accumulator_state(&bank, ring_index_after_buffer_cycle);
    assert_eq!(
        &accumulator_state[16..],
        &accumulator_state_after_skip[16..]
    );

    // insert new message to make sure the update is written in the right position
    // in the ring buffer and overwrites the existing message

    // advance the bank to unfreeze it (to be able to store accounts). see the comment on warp_from_parent above.
    bank = new_from_parent(&Arc::new(bank));

    let wh_sequence_before_acc_update = get_acc_sequence_tracker(&bank).sequence;

    let message_0 = vec![1u8; 127];
    let message_1 = vec![2u8; 127];
    let message_2 = vec![3u8; 254];

    let updated_messages = vec![message_1.clone(), message_2.clone(), message_0.clone()];

    let updated_message_buffer_bytes = create_message_buffer_bytes(updated_messages.clone());
    price_message_account.set_data(updated_message_buffer_bytes);

    // Store Message account so the accumulator sysvar updater can find it.
    bank.store_account(&price_message_key.pubkey(), &price_message_account);

    // Run accumulator, update clock & other sysvars etc
    bank = new_from_parent(&Arc::new(bank));

    let ring_index = (bank.slot() % ACCUMULATOR_RING_SIZE as u64) as u32;
    let updated_wormhole_message_account = get_wormhole_message_account(&bank, ring_index);

    let updated_wormhole_message =
        PostedMessageUnreliableData::deserialize(&mut updated_wormhole_message_account.data())
            .unwrap();

    let updated_messages = updated_messages
        .iter()
        .map(|m| m.as_slice())
        .collect::<Vec<_>>();
    let updated_accumulator_elements = updated_messages
        .clone()
        .into_iter()
        .sorted_unstable()
        .dedup();

    let expected_accumulator =
        MerkleAccumulator::<Keccak160>::from_set(updated_accumulator_elements).unwrap();
    assert_eq!(
        updated_wormhole_message.message.payload,
        expected_accumulator.serialize(bank.slot(), ACCUMULATOR_RING_SIZE)
    );

    let expected_wormhole_message = PostedMessageUnreliableData {
        message: MessageData {
            vaa_version: 1,
            consistency_level: 1,
            submission_time: bank.clock().unix_timestamp as u32,
            sequence: wh_sequence_before_acc_update,
            emitter_chain: 26,
            emitter_address: ACCUMULATOR_EMITTER_ADDRESS,
            payload: expected_accumulator.serialize(bank.slot(), ACCUMULATOR_RING_SIZE),
            ..Default::default()
        },
    };

    assert_eq!(
        updated_wormhole_message_account.data(),
        expected_wormhole_message.try_to_vec().unwrap()
    );

    // TODO: Should be done as additional tests.
    //
    // 1. Verify the accumulator state stays intact after the bank is advanced.
    //      done in this test but can be moved to a separate test.
    // 2. Intentionally add corrupted accounts that do not appear in the accumulator.
    // 3. Check if message offset is > message size to prevent validator crash.
}

fn new_from_parent(parent: &Arc<Bank>) -> Bank {
    Bank::new_from_parent(parent, &Pubkey::default(), parent.slot() + 1)
}

#[test]
fn test_update_accumulator_end_of_block() {
    let leader_pubkey = solana_sdk::pubkey::new_rand();
    let GenesisConfigInfo {
        mut genesis_config, ..
    } = create_genesis_config_with_leader(5, &leader_pubkey, 3);

    // The genesis create function uses `Develompent` mode which enables all feature flags, so
    // we need to remove the accumulator sysvar in order to test the validator behaves
    // correctly when the feature is disabled. We will re-enable it further into this test.
    genesis_config
        .accounts
        .remove(&feature_set::enable_accumulator_sysvar::id())
        .unwrap();
    genesis_config
        .accounts
        .remove(&feature_set::move_accumulator_to_end_of_block::id())
        .unwrap();

    // Set epoch length to 32 so we can advance epochs quickly. We also skip past slot 0 here
    // due to slot 0 having special handling.
    let slots_in_epoch = 32;
    genesis_config.epoch_schedule = EpochSchedule::new(slots_in_epoch);
    let mut bank = Bank::new_for_tests(&genesis_config);
    bank = new_from_parent(&Arc::new(bank));
    bank = new_from_parent(&Arc::new(bank));

    let message_0 = vec![1u8; 127];
    let message_1 = vec![2u8; 127];
    // insert into message buffer in reverse order to test that accumulator
    // sorts first
    let messages = vec![message_1, message_0];

    let message_buffer_bytes = create_message_buffer_bytes(messages.clone());

    // Create a Message account.
    let price_message_key = keypair_from_seed(&[1u8; 32]).unwrap();
    let mut price_message_account = bank
        .get_account(&price_message_key.pubkey())
        .unwrap_or_default();
    price_message_account.set_lamports(1_000_000_000);
    price_message_account.set_owner(Pubkey::new_from_array(pythnet_sdk::MESSAGE_BUFFER_PID));
    price_message_account.set_data(message_buffer_bytes);

    // Store Message account so the accumulator sysvar updater can find it.
    bank.store_account(&price_message_key.pubkey(), &price_message_account);

    let (price_feed_key, _bump) = Pubkey::find_program_address(&[b"123"], &ORACLE_PID);
    let mut price_feed_account = AccountSharedData::new(42, size_of::<PriceAccount>(), &ORACLE_PID);
    PriceAccount::initialize(
        &AccountInfo::new(
            &price_feed_key.to_bytes().into(),
            false,
            true,
            &mut 0,
            &mut price_feed_account.data_mut(),
            &ORACLE_PID.to_bytes().into(),
            false,
            Epoch::default(),
        ),
        0,
    )
    .unwrap();
    bank.store_account(&price_feed_key, &price_feed_account);

    // Derive the Wormhole Message Account that will be generated by the sysvar updater.
    let (wormhole_message_pubkey, _bump) = Pubkey::find_program_address(
        &[b"AccumulatorMessage", &(bank.slot() as u32).to_be_bytes()],
        &Pubkey::new_from_array(pythnet_sdk::pythnet::WORMHOLE_PID),
    );

    // Account Data should be empty at this point. Check account data is [].
    let wormhole_message_account = bank
        .get_account(&wormhole_message_pubkey)
        .unwrap_or_default();
    assert_eq!(wormhole_message_account.data().len(), 0);

    // Run accumulator by creating a new bank from parent, the feature is
    // disabled so account data should still be empty. Check account data is
    // still [].
    bank = new_from_parent(&Arc::new(bank));

    assert_eq!(
        bank.feature_set
            .is_active(&feature_set::enable_accumulator_sysvar::id()),
        false
    );
    assert_eq!(
        bank.feature_set
            .is_active(&feature_set::move_accumulator_to_end_of_block::id()),
        false
    );

    let wormhole_message_account = bank
        .get_account(&wormhole_message_pubkey)
        .unwrap_or_default();
    assert_eq!(wormhole_message_account.data().len(), 0);

    // Enable Accumulator Features (42 = random lamport balance, and the meaning of the universe).
    let feature_id = feature_set::enable_accumulator_sysvar::id();
    let feature = Feature {
        activated_at: Some(30),
    };
    bank.store_account(&feature_id, &feature::create_account(&feature, 42));

    let feature_id = feature_set::move_accumulator_to_end_of_block::id();
    let feature = Feature {
        activated_at: Some(30),
    };
    bank.store_account(&feature_id, &feature::create_account(&feature, 42));

    bank.compute_active_feature_set(true);
    for _ in 0..slots_in_epoch {
        bank = new_from_parent(&Arc::new(bank));
    }

    // Features should now be enabled on the new bank as the epoch has changed.
    assert_eq!(
        bank.feature_set
            .is_active(&feature_set::enable_accumulator_sysvar::id()),
        true
    );
    assert_eq!(
        bank.feature_set
            .is_active(&feature_set::move_accumulator_to_end_of_block::id()),
        true
    );

    // The current sequence value will be used in the message when the bank advances, so we snapshot
    // it here before freezing the bank so we can assert the correct sequence is present in the message.
    let sequence_tracker_before_bank_freeze = get_acc_sequence_tracker(&bank);
    // Freeze the bank to make sure accumulator is updated
    bank.freeze();

    // get the timestamp & slot for the message
    let ring_index = (bank.slot() % ACCUMULATOR_RING_SIZE as u64) as u32;
    let wormhole_message_account = get_wormhole_message_account(&bank, ring_index);

    assert_ne!(wormhole_message_account.data().len(), 0);

    let wormhole_message =
        PostedMessageUnreliableData::deserialize(&mut wormhole_message_account.data()).unwrap();

    let messages = messages.iter().map(|m| m.as_slice()).collect::<Vec<_>>();
    let accumulator_elements = messages.clone().into_iter().sorted_unstable().dedup();
    let expected_accumulator =
        MerkleAccumulator::<Keccak160>::from_set(accumulator_elements).unwrap();
    let expected_wormhole_message_payload =
        expected_accumulator.serialize(bank.slot(), ACCUMULATOR_RING_SIZE);
    assert_eq!(
        wormhole_message.message.payload,
        expected_wormhole_message_payload
    );

    let expected_wormhole_message = PostedMessageUnreliableData {
        message: MessageData {
            vaa_version: 1,
            consistency_level: 1,
            submission_time: bank.clock().unix_timestamp as u32,
            sequence: sequence_tracker_before_bank_freeze.sequence, // sequence is incremented after the message is processed
            emitter_chain: 26,
            emitter_address: ACCUMULATOR_EMITTER_ADDRESS,
            payload: expected_wormhole_message_payload,
            ..Default::default()
        },
    };

    assert_eq!(
        wormhole_message_account.data().to_vec(),
        expected_wormhole_message.try_to_vec().unwrap()
    );

    // verify hashes verify in accumulator
    for msg in messages {
        let msg_hash = Keccak160::hashv(&[[0u8].as_ref(), msg]);
        let msg_proof = expected_accumulator.prove(msg).unwrap();

        assert!(expected_accumulator.nodes.contains(&msg_hash));
        assert!(expected_accumulator.check(msg_proof, msg));
    }

    // verify accumulator state account
    let accumulator_state = get_accumulator_state(&bank, ring_index);
    let acc_state_magic = &accumulator_state[..4];
    let acc_state_slot = LittleEndian::read_u64(&accumulator_state[4..12]);
    let acc_state_ring_size = LittleEndian::read_u32(&accumulator_state[12..16]);

    assert_eq!(acc_state_magic, b"PAS1");
    assert_eq!(acc_state_slot, bank.slot());
    assert_eq!(acc_state_ring_size, ACCUMULATOR_RING_SIZE);

    let mut cursor = std::io::Cursor::new(&accumulator_state[16..]);
    let num_elems = cursor.read_u32::<LittleEndian>().unwrap();
    for _ in 0..(num_elems as usize) {
        let element_len = cursor.read_u32::<LittleEndian>().unwrap();
        let mut element_data = vec![0u8; element_len as usize];
        cursor.read_exact(&mut element_data).unwrap();

        let elem_hash = Keccak160::hashv(&[[0u8].as_ref(), element_data.as_slice()]);
        let elem_proof = expected_accumulator.prove(element_data.as_slice()).unwrap();

        assert!(expected_accumulator.nodes.contains(&elem_hash));
        assert!(expected_accumulator.check(elem_proof, element_data.as_slice()));
    }

    // verify sequence_tracker increments
    assert_eq!(
        get_acc_sequence_tracker(&bank).sequence,
        sequence_tracker_before_bank_freeze.sequence + 1
    );

    // verify ring buffer cycles
    let ring_index_before_buffer_cycle = (bank.slot() % ACCUMULATOR_RING_SIZE as u64) as u32;
    let target_slot = bank.slot() + ACCUMULATOR_RING_SIZE as u64;
    // advance ACCUMULATOR_RING_SIZE slots using warp_from_parent since doing large loops
    // with new_from_parent takes a long time. warp_from_parent results in a bank that is frozen.
    bank = Bank::warp_from_parent(&Arc::new(bank), &Pubkey::default(), target_slot);

    // accumulator messages should still be the same before looping around
    let ring_index_after_buffer_cycle = (bank.slot() % ACCUMULATOR_RING_SIZE as u64) as u32;
    assert_eq!(
        ring_index_before_buffer_cycle,
        ring_index_after_buffer_cycle
    );

    let accumulator_state_after_skip = get_accumulator_state(&bank, ring_index_after_buffer_cycle);
    assert_eq!(
        &accumulator_state[16..],
        &accumulator_state_after_skip[16..]
    );

    // insert new message to make sure the update is written in the right position
    // in the ring buffer and overwrites the existing message

    // advance the bank to unfreeze it (to be able to store accounts). see the comment on warp_from_parent above.
    bank = new_from_parent(&Arc::new(bank));

    let wh_sequence_before_acc_update = get_acc_sequence_tracker(&bank).sequence;

    let message_0 = vec![1u8; 127];
    let message_1 = vec![2u8; 127];
    let message_2 = vec![3u8; 254];

    let updated_messages = vec![message_1.clone(), message_2.clone(), message_0.clone()];

    let updated_message_buffer_bytes = create_message_buffer_bytes(updated_messages.clone());
    price_message_account.set_data(updated_message_buffer_bytes);

    // Store Message account so the accumulator sysvar updater can find it.
    bank.store_account(&price_message_key.pubkey(), &price_message_account);

    // Freeze the bank to run accumulator
    bank.freeze();

    let ring_index = (bank.slot() % ACCUMULATOR_RING_SIZE as u64) as u32;
    let updated_wormhole_message_account = get_wormhole_message_account(&bank, ring_index);

    let updated_wormhole_message =
        PostedMessageUnreliableData::deserialize(&mut updated_wormhole_message_account.data())
            .unwrap();

    let updated_messages = updated_messages
        .iter()
        .map(|m| m.as_slice())
        .collect::<Vec<_>>();
    let updated_accumulator_elements = updated_messages
        .clone()
        .into_iter()
        .sorted_unstable()
        .dedup();

    let expected_accumulator =
        MerkleAccumulator::<Keccak160>::from_set(updated_accumulator_elements).unwrap();
    assert_eq!(
        updated_wormhole_message.message.payload,
        expected_accumulator.serialize(bank.slot(), ACCUMULATOR_RING_SIZE)
    );

    let expected_wormhole_message = PostedMessageUnreliableData {
        message: MessageData {
            vaa_version: 1,
            consistency_level: 1,
            submission_time: bank.clock().unix_timestamp as u32,
            sequence: wh_sequence_before_acc_update,
            emitter_chain: 26,
            emitter_address: ACCUMULATOR_EMITTER_ADDRESS,
            payload: expected_accumulator.serialize(bank.slot(), ACCUMULATOR_RING_SIZE),
            ..Default::default()
        },
    };

    assert_eq!(
        updated_wormhole_message_account.data(),
        expected_wormhole_message.try_to_vec().unwrap()
    );
}

// This test will
#[test]
fn test_accumulator_v2() {
    let leader_pubkey = solana_sdk::pubkey::new_rand();
    let GenesisConfigInfo {
        mut genesis_config, ..
    } = create_genesis_config_with_leader(5, &leader_pubkey, 3);

    // Set epoch length to 32 so we can advance epochs quickly. We also skip past slot 0 here
    // due to slot 0 having special handling.
    let slots_in_epoch = 32;
    genesis_config.epoch_schedule = EpochSchedule::new(slots_in_epoch);
    let mut bank = Bank::new_for_tests(&genesis_config);

    bank = new_from_parent(&Arc::new(bank)); // Advance slot 1.
    bank = new_from_parent(&Arc::new(bank)); // Advance slot 2.

    let generate_price = |seeds, generate_buffers: bool| {
        let (price_feed_key, _bump) = Pubkey::find_program_address(&[seeds], &ORACLE_PID);
        let mut price_feed_account =
            AccountSharedData::new(42, size_of::<PriceAccount>(), &ORACLE_PID);

        let messages = {
            let price_feed_info_key = &price_feed_key.to_bytes().into();
            let price_feed_info_lamports = &mut 0;
            let price_feed_info_owner = &ORACLE_PID.to_bytes().into();
            let price_feed_info_data = price_feed_account.data_mut();
            let price_feed_info = AccountInfo::new(
                price_feed_info_key,
                false,
                true,
                price_feed_info_lamports,
                price_feed_info_data,
                price_feed_info_owner,
                false,
                Epoch::default(),
            );

            let mut price_account = PriceAccount::initialize(&price_feed_info, 0).unwrap();
            if !generate_buffers {
                price_account.flags.insert(
                    PriceAccountFlags::ACCUMULATOR_V2 | PriceAccountFlags::MESSAGE_BUFFER_CLEARED,
                );
            }

            vec![
                price_account
                    .as_price_feed_message(&price_feed_key.to_bytes().into())
                    .to_bytes(),
                price_account
                    .as_twap_message(&price_feed_key.to_bytes().into())
                    .to_bytes(),
            ]
        };

        bank.store_account(&price_feed_key, &price_feed_account);

        if generate_buffers {
            // Insert into message buffer in reverse order to test that accumulator
            // sorts first.
            let message_buffer_bytes = create_message_buffer_bytes(messages.clone());

            // Create a Message account.
            let price_message_key = keypair_from_seed(&[1u8; 32]).unwrap();
            let mut price_message_account = bank
                .get_account(&price_message_key.pubkey())
                .unwrap_or_default();

            price_message_account.set_lamports(1_000_000_000);
            price_message_account
                .set_owner(Pubkey::new_from_array(pythnet_sdk::MESSAGE_BUFFER_PID));
            price_message_account.set_data(message_buffer_bytes);

            // Store Message account so the accumulator sysvar updater can find it.
            bank.store_account(&price_message_key.pubkey(), &price_message_account);
        }

        (price_feed_key, messages)
    };

    // TODO: New test functionality here.
    // 1. Create Price Feed Accounts owned by ORACLE_PUBKEY
    // 2. Populate Price Feed Accounts
    // 3. Call update_v2()
    //    - Cases:
    //      - No V1 Messages, Only Price Accounts with no V2
    //      - No V1 Messages, Some Price Accounts with no V2
    //      - Some V1 Messages, No Price Accounts with no V2
    //      - Some V1 Messages, Some Price Accounts with no V2
    //      - Simulate PriceUpdate that WOULD trigger a real V1 aggregate before End of Slot
    //      - Simulate PriceUpdate that doesn't trigger a real V1 aggregate, only V2.

    assert!(bank
        .feature_set
        .is_active(&feature_set::enable_accumulator_sysvar::id()));
    assert!(bank
        .feature_set
        .is_active(&feature_set::move_accumulator_to_end_of_block::id()));
    assert!(bank
        .feature_set
        .is_active(&feature_set::undo_move_accumulator_to_end_of_block::id()));
    assert!(bank
        .feature_set
        .is_active(&feature_set::redo_move_accumulator_to_end_of_block::id()));

    let prices_with_messages = [
        generate_price(b"seeds_1", false),
        generate_price(b"seeds_2", false),
        generate_price(b"seeds_3", false),
        generate_price(b"seeds_4", false),
    ];

    let messages = prices_with_messages
        .iter()
        .map(|(_, messages)| messages)
        .flatten()
        .map(|message| &message[..]);

    // Trigger Aggregation. We freeze instead of new_from_parent so
    // we can keep access to the bank.
    let sequence_tracker_before_bank_freeze = get_acc_sequence_tracker(&bank);
    bank.freeze();

    // Get the wormhole message generated by freezed. We don't need
    // to offset the ring index as our test is always below 10K slots.
    let wormhole_message_account = get_wormhole_message_account(&bank, bank.slot() as u32);
    assert_ne!(wormhole_message_account.data().len(), 0);
    PostedMessageUnreliableData::deserialize(&mut wormhole_message_account.data()).unwrap();

    // Create MerkleAccumulator by hand to verify that the Wormhole message
    // contents are correctg.
    let expected_accumulator =
        MerkleAccumulator::<Keccak160>::from_set(messages.clone().sorted_unstable().dedup())
            .unwrap();

    let expected_wormhole_message_payload =
        expected_accumulator.serialize(bank.slot(), ACCUMULATOR_RING_SIZE);

    let expected_wormhole_message = PostedMessageUnreliableData {
        message: MessageData {
            vaa_version: 1,
            consistency_level: 1,
            submission_time: bank.clock().unix_timestamp as u32,
            sequence: sequence_tracker_before_bank_freeze.sequence, // sequence is incremented after the message is processed
            emitter_chain: 26,
            emitter_address: ACCUMULATOR_EMITTER_ADDRESS,
            payload: expected_wormhole_message_payload,
            ..Default::default()
        },
    };

    assert_eq!(
        wormhole_message_account.data().to_vec(),
        expected_wormhole_message.try_to_vec().unwrap()
    );

    // Verify hashes in accumulator.
    for msg in messages {
        let msg_hash = Keccak160::hashv(&[[0u8].as_ref(), msg]);
        let msg_proof = expected_accumulator.prove(msg).unwrap();
        assert!(expected_accumulator.nodes.contains(&msg_hash));
        assert!(expected_accumulator.check(msg_proof, msg));
    }

    // Verify accumulator state account.
    let accumulator_state = get_accumulator_state(&bank, bank.slot() as u32);
    let acc_state_magic = &accumulator_state[..4];
    let acc_state_slot = LittleEndian::read_u64(&accumulator_state[4..12]);
    let acc_state_ring_size = LittleEndian::read_u32(&accumulator_state[12..16]);

    assert_eq!(acc_state_magic, b"PAS1");
    assert_eq!(acc_state_slot, bank.slot());
    assert_eq!(acc_state_ring_size, ACCUMULATOR_RING_SIZE);

    // Verify the messages within the accumulator state account
    // were in the accumulator as well.
    let mut cursor = std::io::Cursor::new(&accumulator_state[16..]);
    let num_elems = cursor.read_u32::<LittleEndian>().unwrap();
    for _ in 0..(num_elems as usize) {
        let element_len = cursor.read_u32::<LittleEndian>().unwrap();
        let mut element_data = vec![0u8; element_len as usize];
        cursor.read_exact(&mut element_data).unwrap();

        let elem_hash = Keccak160::hashv(&[[0u8].as_ref(), element_data.as_slice()]);
        let elem_proof = expected_accumulator.prove(element_data.as_slice()).unwrap();

        assert!(expected_accumulator.nodes.contains(&elem_hash));
        assert!(expected_accumulator.check(elem_proof, element_data.as_slice()));
    }

    // Verify sequence_tracker increments for wormhole to accept it.
    assert_eq!(
        get_acc_sequence_tracker(&bank).sequence,
        sequence_tracker_before_bank_freeze.sequence + 1
    );
}
#[test]
fn test_get_accumulator_keys() {
    use pythnet_sdk::{pythnet, ACCUMULATOR_EMITTER_ADDRESS, MESSAGE_BUFFER_PID};
    let accumulator_keys: Vec<Pubkey> = get_accumulator_keys()
        .iter()
        .map(|(_, pk_res)| *pk_res.as_ref().unwrap())
        .collect();
    let expected_pyth_keys = vec![
        Pubkey::new_from_array(MESSAGE_BUFFER_PID),
        Pubkey::new_from_array(ACCUMULATOR_EMITTER_ADDRESS),
        Pubkey::new_from_array(pythnet::ACCUMULATOR_SEQUENCE_ADDR),
        Pubkey::new_from_array(pythnet::WORMHOLE_PID),
    ];
    assert_eq!(accumulator_keys, expected_pyth_keys);
}
