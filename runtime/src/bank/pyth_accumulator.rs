use {
    super::Bank,
    crate::accounts_index::{ScanConfig, ScanError},
    byteorder::LittleEndian,
    byteorder::ReadBytesExt,
    itertools::Itertools,
    log::*,
    pyth_oracle::validator::AggregationError,
    pythnet_sdk::{
        accumulators::{merkle::MerkleAccumulator, Accumulator},
        hashers::keccak256_160::Keccak160,
        wormhole::{AccumulatorSequenceTracker, MessageData, PostedMessageUnreliableData},
    },
    solana_sdk::{
        account::{AccountSharedData, ReadableAccount},
        borsh, feature_set,
        hash::hashv,
        pubkey::Pubkey,
    },
    std::env::{self, VarError},
};

pub const ACCUMULATOR_RING_SIZE: u32 = 10_000;

lazy_static! {
    pub static ref MESSAGE_BUFFER_PID: Pubkey = env_pubkey_or(
        "MESSAGE_BUFFER_PID",
        Pubkey::new_from_array(pythnet_sdk::MESSAGE_BUFFER_PID),
    );
    pub static ref ACCUMULATOR_EMITTER_ADDR: Pubkey = env_pubkey_or(
        "ACCUMULATOR_EMITTER_ADDR",
        Pubkey::new_from_array(pythnet_sdk::ACCUMULATOR_EMITTER_ADDRESS),
    );
    pub static ref ACCUMULATOR_SEQUENCE_ADDR: Pubkey = env_pubkey_or(
        "ACCUMULATOR_SEQUENCE_ADDR",
        Pubkey::new_from_array(pythnet_sdk::pythnet::ACCUMULATOR_SEQUENCE_ADDR),
    );
    pub static ref WORMHOLE_PID: Pubkey = env_pubkey_or(
        "WORMHOLE_PID",
        Pubkey::new_from_array(pythnet_sdk::pythnet::WORMHOLE_PID),
    );
    pub static ref ORACLE_PID: Pubkey = env_pubkey_or(
        "ORACLE_PID",
        "FsJ3A3u2vn5cTVofAjvy6y5kwABJAqYWpe4975bi2epH"
            .parse()
            .unwrap(),
    );
}

/// Accumulator specific error type. It would be nice to use `transaction::Error` but it does
/// not include any `Custom` style variant we can leverage, so we introduce our own.
#[derive(Debug, thiserror::Error)]
pub enum AccumulatorUpdateErrorV1 {
    #[error("get_program_accounts failed to return accounts: {0}")]
    GetProgramAccounts(#[from] ScanError),

    #[error("failed to serialize sequence account")]
    FailedSequenceSerialization,

    #[error("failed to serialize message account")]
    FailedMessageSerialization,

    #[error("io error")]
    Io(#[from] std::io::Error),

    #[error("could not parse Pubkey from environment")]
    InvalidEnvPubkey(#[from] solana_sdk::pubkey::ParsePubkeyError),
}

/// Updates the Accumulator Sysvar at the start of a new slot. See `update_clock` to see a similar
/// sysvar this is based on.
///
/// Note:
/// - Library imports are placed within this function to keep the diff against upstream small.
/// - This update will incur a performance hit on each slot, so must be kept efficient.
/// - Focused on Merkle for initial release but will generalise to more accumulators in future.
pub fn update_accumulator(bank: &Bank) {
    if !bank
        .feature_set
        .is_active(&feature_set::enable_accumulator_sysvar::id())
    {
        info!(
            "Accumulator: Skipping because the feature is disabled. Slot: {}",
            bank.slot()
        );
        return;
    }

    info!("Accumulator: Updating accumulator. Slot: {}", bank.slot());

    if let Err(e) = update_v2(bank) {
        error!("Error updating accumulator v2: {:?}", e);
    }
}

/// Read the pubkey from the environment variable `var` or return `default`
/// if the variable is not set.
fn env_pubkey_or(var: &str, default: Pubkey) -> Pubkey {
    match env::var(var) {
        Ok(value) => value.parse().unwrap_or_else(|err| {
            panic!(
                "failed to parse env var {}={:?} as pubkey: {}",
                var, value, err
            )
        }),
        Err(VarError::NotPresent) => default,
        Err(VarError::NotUnicode(value)) => {
            panic!("invalid value of env var {}={:?}: not unicode", var, value);
        }
    }
}

/// Get all accumulator related pubkeys from environment variables
/// or return default if the variable is not set.
pub fn get_accumulator_keys() -> Vec<(
    &'static str,
    std::result::Result<Pubkey, AccumulatorUpdateErrorV1>,
)> {
    vec![
        ("MESSAGE_BUFFER_PID", Ok(*MESSAGE_BUFFER_PID)),
        ("ACCUMULATOR_EMITTER_ADDR", Ok(*ACCUMULATOR_EMITTER_ADDR)),
        ("ACCUMULATOR_SEQUENCE_ADDR", Ok(*ACCUMULATOR_SEQUENCE_ADDR)),
        ("WORMHOLE_PID", Ok(*WORMHOLE_PID)),
        ("ORACLE_PID", Ok(*ORACLE_PID)),
    ]
}

pub fn update_v1<'a>(
    bank: &Bank,
    v2_messages: &[Vec<u8>],
    use_message_buffers: bool,
) -> std::result::Result<(), AccumulatorUpdateErrorV1> {
    // Use the current Clock to determine the index into the accumulator ring buffer.
    let ring_index = (bank.slot() % 10_000) as u32;

    // Find all accounts owned by the Message Buffer program using get_program_accounts, and
    // extract the account data.

    let message_buffer_accounts;
    let v1_messages = if use_message_buffers {
        message_buffer_accounts = bank
            .get_program_accounts(&MESSAGE_BUFFER_PID, &ScanConfig::new(true))
            .map_err(AccumulatorUpdateErrorV1::GetProgramAccounts)?;

        let preimage = b"account:MessageBuffer";
        let mut expected_sighash = [0u8; 8];
        expected_sighash.copy_from_slice(&hashv(&[preimage]).to_bytes()[..8]);

        // Filter accounts that don't match the Anchor sighash.
        let message_buffer_accounts = message_buffer_accounts.iter().filter(|(_, account)| {
            // Remove accounts that do not start with the expected Anchor sighash.
            let mut sighash = [0u8; 8];
            sighash.copy_from_slice(&account.data()[..8]);
            sighash == expected_sighash
        });

        // This code, using the offsets in each Account, extracts the various data versions from
        // the account. We deduplicate this result because the accumulator expects a set.
        message_buffer_accounts
            .map(|(_, account)| {
                let data = account.data();
                let mut cursor = std::io::Cursor::new(&data);
                let _sighash = cursor.read_u64::<LittleEndian>()?;
                let _bump = cursor.read_u8()?;
                let _version = cursor.read_u8()?;
                let header_len = cursor.read_u16::<LittleEndian>()?;
                let mut header_begin = header_len;
                let mut inputs = Vec::new();
                let mut cur_end_offsets_idx: usize = 0;
                while let Some(end) = cursor.read_u16::<LittleEndian>().ok() {
                    if end == 0 || cur_end_offsets_idx == (u8::MAX as usize) {
                        break;
                    }

                    let end_offset = header_len + end;
                    if end_offset as usize > data.len() {
                        break;
                    }
                    let accumulator_input_data = &data[header_begin as usize..end_offset as usize];
                    inputs.push(accumulator_input_data);
                    header_begin = end_offset;
                    cur_end_offsets_idx += 1;
                }

                Ok(inputs)
            })
            .collect::<std::result::Result<Vec<_>, std::io::Error>>()?
            .into_iter()
            .flatten()
            .sorted_unstable()
            .dedup()
            .collect()
    } else {
        Vec::new()
    };

    let mut messages = v1_messages;
    messages.extend(v2_messages.iter().map(|x| &**x));

    // We now generate a Proof PDA (Owned by the System Program) to store the resulting Proof
    // Set. The derivation includes the ring buffer index to simulate a ring buffer in order
    // for RPC users to select the correct proof for an associated VAA.
    let (accumulator_account, _) = Pubkey::find_program_address(
        &[b"AccumulatorState", &ring_index.to_be_bytes()],
        &solana_sdk::system_program::id(),
    );

    let accumulator_data = {
        let mut data = vec![];
        let acc_state_magic = &mut b"PAS1".to_vec();
        let accounts_data = &mut borsh::BorshSerialize::try_to_vec(&messages)?;
        data.append(acc_state_magic);
        data.append(&mut borsh::BorshSerialize::try_to_vec(&bank.slot())?);
        data.append(&mut borsh::BorshSerialize::try_to_vec(
            &ACCUMULATOR_RING_SIZE,
        )?);
        data.append(accounts_data);
        let owner = solana_sdk::system_program::id();
        let balance = bank.get_minimum_balance_for_rent_exemption(data.len());
        let mut account = AccountSharedData::new(balance, data.len(), &owner);
        account.set_data(data);
        account
    };

    // Generate a Message owned by Wormhole to be sent cross-chain. This short-circuits the
    // Wormhole message generation code that would normally be called, but the Guardian
    // set filters our messages so this does not pose a security risk.
    if let Some(accumulator) = MerkleAccumulator::<Keccak160>::from_set(messages.into_iter()) {
        post_accumulator_attestation(bank, accumulator, ring_index)?;
    }

    // Write the Account Set into `accumulator_state` so that the hermes application can
    // request historical data to prove.
    info!(
        "Accumulator: Writing accumulator state to {:?}",
        accumulator_account
    );
    bank.store_account_and_update_capitalization(&accumulator_account, &accumulator_data);

    Ok(())
}

/// TODO: Safe integer conversion checks if any are missed.
fn post_accumulator_attestation(
    bank: &Bank,
    acc: pythnet_sdk::accumulators::merkle::MerkleAccumulator<
        pythnet_sdk::hashers::keccak256_160::Keccak160,
    >,
    ring_index: u32,
) -> std::result::Result<(), AccumulatorUpdateErrorV1> {
    // Wormhole uses a Sequence account that is incremented each time a message is posted. As
    // we aren't calling Wormhole we need to bump this ourselves. If it doesn't exist, we just
    // create it instead.
    let mut sequence: AccumulatorSequenceTracker = {
        let data = bank
            .get_account_with_fixed_root(&ACCUMULATOR_SEQUENCE_ADDR)
            .unwrap_or_default();
        let data = data.data();
        solana_sdk::borsh::try_from_slice_unchecked(data)
            .unwrap_or(AccumulatorSequenceTracker { sequence: 0 })
    };

    debug!("Accumulator: accumulator sequence: {:?}", sequence.sequence);

    // Generate the Message to emit via Wormhole.
    let message = PostedMessageUnreliableData {
        message: if !bank
            .feature_set
            .is_active(&feature_set::zero_wormhole_message_timestamps::id())
        {
            MessageData {
                vaa_version: 1,
                consistency_level: 1,
                vaa_time: 1u32,
                vaa_signature_account: Pubkey::default().to_bytes(),
                submission_time: bank.clock().unix_timestamp as u32,
                nonce: 0,
                sequence: sequence.sequence,
                emitter_chain: 26,
                emitter_address: ACCUMULATOR_EMITTER_ADDR.to_bytes(),
                payload: acc.serialize(bank.slot(), ACCUMULATOR_RING_SIZE),
            }
        } else {
            // Use Default::default() to ensure zeroed VAA fields.
            MessageData {
                vaa_version: 1,
                consistency_level: 1,
                submission_time: bank.clock().unix_timestamp as u32,
                sequence: sequence.sequence,
                emitter_chain: 26,
                emitter_address: ACCUMULATOR_EMITTER_ADDR.to_bytes(),
                payload: acc.serialize(bank.slot(), ACCUMULATOR_RING_SIZE),
                ..Default::default()
            }
        },
    };

    debug!("Accumulator: Wormhole message data: {:?}", message.message);
    // Now we can bump and write the Sequence account.
    sequence.sequence += 1;
    let sequence = solana_sdk::borsh::BorshSerialize::try_to_vec(&sequence)
        .map_err(|_| AccumulatorUpdateErrorV1::FailedSequenceSerialization)?;
    let sequence_balance = bank.get_minimum_balance_for_rent_exemption(sequence.len());
    let sequence_account = {
        let owner = &WORMHOLE_PID;
        let mut account = AccountSharedData::new(sequence_balance, sequence.len(), owner);
        account.set_data(sequence);
        account
    };

    // Serialize into (and create if necessary) the message account.
    let message = solana_sdk::borsh::BorshSerialize::try_to_vec(&message)
        .map_err(|_| AccumulatorUpdateErrorV1::FailedMessageSerialization)?;
    let message_balance = bank.get_minimum_balance_for_rent_exemption(message.len());
    let message_account = {
        let owner = &WORMHOLE_PID;
        let mut account = AccountSharedData::new(message_balance, message.len(), owner);
        account.set_data(message);
        account
    };

    // The message_pda derivation includes the ring buffer index to simulate a ring buffer in order
    // for RPC users to select the message for an associated VAA.
    let (message_pda, _) = Pubkey::find_program_address(
        &[b"AccumulatorMessage", &ring_index.to_be_bytes()],
        &WORMHOLE_PID,
    );

    bank.store_account_and_update_capitalization(&ACCUMULATOR_SEQUENCE_ADDR, &sequence_account);

    info!("Accumulator: Writing wormhole message to {:?}", message_pda);
    bank.store_account_and_update_capitalization(&message_pda, &message_account);

    Ok(())
}

pub fn update_v2(bank: &Bank) -> std::result::Result<(), AccumulatorUpdateErrorV1> {
    let accounts = bank
        .get_program_accounts(&ORACLE_PID, &ScanConfig::new(true))
        .map_err(AccumulatorUpdateErrorV1::GetProgramAccounts)?;

    let mut any_v1_aggregations = false;
    let mut v2_messages = Vec::new();

    for (pubkey, mut account) in accounts {
        let mut price_account_data = account.data().to_owned();

        // Perform Accumulation
        match pyth_oracle::validator::aggregate_price(
            bank.slot(),
            bank.clock().unix_timestamp,
            &pubkey.to_bytes().into(),
            &mut price_account_data,
        ) {
            Ok(messages) => {
                account.set_data(price_account_data);
                bank.store_account_and_update_capitalization(&pubkey, &account);
                v2_messages.extend(messages);
            }
            Err(err) => match err {
                AggregationError::NotPriceFeedAccount => {}
                AggregationError::V1AggregationMode | AggregationError::AlreadyAggregated => {
                    any_v1_aggregations = true;
                }
            },
        }
    }

    update_v1(bank, &v2_messages, any_v1_aggregations)
}
