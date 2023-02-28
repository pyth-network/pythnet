use crate::RawPubkey;
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use std::io::ErrorKind::InvalidData;
use std::io::{Error, Write};
use std::ops::{Deref, DerefMut};

#[repr(transparent)]
#[derive(Default)]
pub struct PostedMessageUnreliableData {
    pub message: MessageData,
}

#[derive(Debug, Default, BorshSerialize, BorshDeserialize, Clone, Serialize, Deserialize)]
pub struct MessageData {
    /// Header of the posted VAA
    pub vaa_version: u8,

    /// Level of consistency requested by the emitter
    pub consistency_level: u8,

    /// Time the vaa was submitted
    pub vaa_time: u32,

    /// Account where signatures are stored
    pub vaa_signature_account: RawPubkey,

    /// Time the posted message was created
    pub submission_time: u32,

    /// Unique nonce for this message
    pub nonce: u32,

    /// Sequence number of this message
    pub sequence: u64,

    /// Emitter of the message
    pub emitter_chain: u16,

    /// Emitter of the message
    pub emitter_address: [u8; 32],

    /// Message payload
    pub payload: Vec<u8>,
}

impl BorshSerialize for PostedMessageUnreliableData {
    fn serialize<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        writer.write_all(b"msu")?;
        BorshSerialize::serialize(&self.message, writer)
    }
}

impl BorshDeserialize for PostedMessageUnreliableData {
    fn deserialize(buf: &mut &[u8]) -> std::io::Result<Self> {
        if buf.len() < 3 {
            return Err(Error::new(InvalidData, "Not enough bytes"));
        }

        let expected = b"msu";
        let magic: &[u8] = &buf[0..3];
        if magic != expected {
            return Err(Error::new(
                InvalidData,
                format!(
                    "Magic mismatch. Expected {:?} but got {:?}",
                    expected, magic
                ),
            ));
        };
        *buf = &buf[3..];
        Ok(PostedMessageUnreliableData {
            message: <MessageData as BorshDeserialize>::deserialize(buf)?,
        })
    }
}

impl Deref for PostedMessageUnreliableData {
    type Target = MessageData;

    fn deref(&self) -> &Self::Target {
        &self.message
    }
}

impl DerefMut for PostedMessageUnreliableData {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.message
    }
}

impl Clone for PostedMessageUnreliableData {
    fn clone(&self) -> Self {
        PostedMessageUnreliableData {
            message: self.message.clone(),
        }
    }
}

#[derive(Default, Clone, Copy, BorshDeserialize, BorshSerialize)]
pub struct AccumulatorSequenceTracker {
    pub sequence: u64,
}
