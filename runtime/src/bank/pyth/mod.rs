use {
    accumulator::{
        ACCUMULATOR_EMITTER_ADDR, ACCUMULATOR_SEQUENCE_ADDR, BATCH_PUBLISH_PID, MESSAGE_BUFFER_PID,
        ORACLE_PID, STAKE_CAPS_PARAMETERS_ADDR, WORMHOLE_PID,
    },
    solana_sdk::pubkey::Pubkey,
};

pub mod accumulator;
mod batch_publish;

#[cfg(test)]
mod tests;

/// Get all pyth related pubkeys from environment variables
/// or return default if the variable is not set.
pub fn get_pyth_keys() -> Vec<(&'static str, Pubkey)> {
    vec![
        ("MESSAGE_BUFFER_PID", *MESSAGE_BUFFER_PID),
        ("ACCUMULATOR_EMITTER_ADDR", *ACCUMULATOR_EMITTER_ADDR),
        ("ACCUMULATOR_SEQUENCE_ADDR", *ACCUMULATOR_SEQUENCE_ADDR),
        ("WORMHOLE_PID", *WORMHOLE_PID),
        ("ORACLE_PID", *ORACLE_PID),
        ("STAKE_CAPS_PARAMETERS_ADDR", *STAKE_CAPS_PARAMETERS_ADDR),
        ("BATCH_PUBLISH_PID", *BATCH_PUBLISH_PID),
    ]
}
