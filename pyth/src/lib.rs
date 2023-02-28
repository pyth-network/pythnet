//! A type to hold data for the [`Accumulator` sysvar][sv].
//!
//! TODO: replace this with an actual link if needed
//! [sv]: https://docs.pythnetwork.org/developing/runtime-facilities/sysvars#accumulator
//!
//! The sysvar ID is declared in [`sysvar::accumulator`].
//!
//! [`sysvar::accumulator`]: crate::sysvar::accumulator

use std::ops::Deref;
use {
    accumulators::merkle::MerkleTree,
    borsh::{BorshDeserialize, BorshSerialize},
    hex::FromHexError,
    pyth::{
        PayloadId, P2W_FORMAT_HDR_SIZE, P2W_FORMAT_VER_MAJOR, P2W_FORMAT_VER_MINOR, PACC2W_MAGIC,
    },
    serde::{Deserialize, Serialize, Serializer},
    std::{
        fmt,
        io::{Error, ErrorKind::InvalidData, Read, Write},
        mem,
        ops::DerefMut,
    },
};

pub mod accumulators;
pub mod pyth;
pub mod wormhole;

pub(crate) type RawPubkey = [u8; 32];
pub(crate) type Hash = [u8; 32];
pub(crate) type PriceId = RawPubkey;

// TODO:
//  1. decide what will be pulled out into a "pythnet" crate and what needs to remain in here
//      a. be careful of cyclic dependencies
//      b. git submodules?

/*** Dummy Field(s) for now just to test updating the sysvar ***/
pub type Slot = u64;

// TODO:
//  1. this needs to store all relevant information that will go into the
//  proof - everything but the unused fields
//  2. this should eventually be generic
// #[repr(transparent)]
// pub struct AccumulatorPrice(u32);
// TODO: check if this is correct repr
//  might need to use #[repr(align(x))]
//  -> see pyth-client/program/rust/tests/test_utils.rs AccountSetup
#[repr(C)]
pub struct AccumulatorPrice {
    pub price_type: u32,
}

pub struct AccumulatorValue<V: std::hash::Hash> {
    pub value: V,
}

/**
bless chatGPT

This implementation uses std::mem::transmute to cast
a reference to the AccumulatorPrice struct to a
byte array of the same size, and then creates a slice from the resulting pointer.
The size of the slice is the size of the struct in bytes,
which is the sum of the sizes of the u32 and u64 fields.

Note that this implementation still uses unsafe code
to create a raw pointer to the AccumulatorPrice struct,
so you need to ensure that the pointer is valid and that
the memory it points to is properly aligned and initialized.

struct AccumulatorPrice {
    price_type: u32,
    price: u64,
}

impl AsRef<[u8]> for AccumulatorPrice {
    fn as_ref(&self) -> &[u8] {
        unsafe {
            std::slice::from_raw_parts(
                std::mem::transmute::<&AccumulatorPrice, &[u8; std::mem::size_of::<AccumulatorPrice>()]>(self).as_ptr(),
                std::mem::size_of::<AccumulatorPrice>(),
            )
        }
    }
}
*/

impl AsRef<[u8]> for AccumulatorPrice {
    fn as_ref(&self) -> &[u8] {
        unsafe {
            std::slice::from_raw_parts(
                &self.price_type as *const u32 as *const u8,
                std::mem::size_of::<u32>(),
            )
        }
    }
}

/** using `sdk/program/src/slot_hashes.rs` as a reference **/

//
// impl FromIterator<(PriceId, PriceHash<'_>)> for PriceProofs<'_> {
//     fn from_iter<I: for<'a> IntoIterator<Item = (PriceId, PriceHash<'a>)>>(iter: 'a I) -> Self {
//         Self(iter.into_iter().collect())
//     }
//     // fn from_iter<I: IntoIterator<Item = (PriceId, PriceHash)>>(iter: I) -> Self {
//     //     Self(iter.into_iter().collect())
//     // }
// }

// impl Deref for PriceProofs<'_> {
//     type Target<'a> = Vec<PriceProof<'a>>;
//     fn deref(&self) -> &Self::Target {
//         &self.0
//     }
// }
// pub fn load_checked<'a, T: PythAccount>(
//     account: &'a AccountInfo,
//     version: u32,
// ) -> Result<RefMut<'a, T>, ProgramError> {
//     pyth_assert(
//         account.data_len() >= T::MINIMUM_SIZE,
//         OracleError::AccountTooSmall.into(),
//     )?;
//
//     {
//         let account_header = load_account_as::<AccountHeader>(account)?;
//         pyth_assert(
//             account_header.magic_number == PC_MAGIC
//                 && account_header.version == version
//                 && account_header.account_type == T::ACCOUNT_TYPE,
//             OracleError::InvalidAccountHeader.into(),
//         )?;
//     }
//
//     load_account_as_mut::<T>(account)
// }

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccumulatorAttestation<P: serde::Serialize> {
    // pub accumulator_root: P,
    // pub accumulator: MerkleTree,
    pub accumulator: P,

    #[serde(serialize_with = "use_to_string")]
    pub ring_buffer_idx: u64,
    #[serde(serialize_with = "use_to_string")]
    pub height: u64,
    // TODO: Go back to UnixTimestamp.
    pub timestamp: i64,
}

pub type ErrBox = Box<dyn std::error::Error>;

// from pyth-crosschain/wormhole_attester/sdk/rust/src/lib.rs
impl<P: serde::Serialize + for<'a> serde::Deserialize<'a>> AccumulatorAttestation<P> {
    pub fn serialize(&self) -> Result<Vec<u8>, ErrBox> {
        // magic
        let mut buf = PACC2W_MAGIC.to_vec();

        // major_version
        buf.extend_from_slice(&P2W_FORMAT_VER_MAJOR.to_be_bytes()[..]);

        // minor_version
        buf.extend_from_slice(&P2W_FORMAT_VER_MINOR.to_be_bytes()[..]);

        // hdr_size
        buf.extend_from_slice(&P2W_FORMAT_HDR_SIZE.to_be_bytes()[..]);

        // // payload_id
        buf.push(PayloadId::AccumulationAttestation as u8);

        // Header is over. NOTE: If you need to append to the header,
        // make sure that the number of bytes after hdr_size is
        // reflected in the P2W_FORMAT_HDR_SIZE constant.

        // n_attestations
        // buf.extend_from_slice(&(self.price_attestations.len() as u16).to_be_bytes()[..]);

        // TODO: is u16 enough?
        //
        // buf.extend_from_slice(&(self.accumulator.merkle_tree.leaf_count as u16).to_be_bytes()[..]);

        let AccumulatorAttestation {
            // accumulator_root: accumulator_root,
            accumulator,
            ring_buffer_idx,
            height,
            timestamp,
        } = self;

        // let mut accumulator_buf = Vec::with_capacity(accumulator.merkle_tree.leaf_count);
        //TODO: decide on pyth-accumulator-over-wormhole serialization format.

        let mut serialized_acc = bincode::serialize(&accumulator).unwrap();

        // TODO: always 32?
        buf.extend_from_slice(&(serialized_acc.len() as u16).to_be_bytes()[..]);

        buf.append(&mut serialized_acc);
        buf.extend_from_slice(&ring_buffer_idx.to_be_bytes()[..]);
        buf.extend_from_slice(&height.to_be_bytes()[..]);
        buf.extend_from_slice(&timestamp.to_be_bytes()[..]);

        Ok(buf)
    }

    //TODO: update this for accumulator attest
    pub fn deserialize(mut bytes: impl Read) -> Result<Self, ErrBox> {
        let mut magic_vec = vec![0u8; PACC2W_MAGIC.len()];
        bytes.read_exact(magic_vec.as_mut_slice())?;

        if magic_vec.as_slice() != PACC2W_MAGIC {
            return Err(
                format!("Invalid magic {magic_vec:02X?}, expected {PACC2W_MAGIC:02X?}",).into(),
            );
        }

        let mut major_version_vec = vec![0u8; mem::size_of_val(&P2W_FORMAT_VER_MAJOR)];
        bytes.read_exact(major_version_vec.as_mut_slice())?;
        let major_version = u16::from_be_bytes(major_version_vec.as_slice().try_into()?);

        // Major must match exactly
        if major_version != P2W_FORMAT_VER_MAJOR {
            return Err(format!(
                "Unsupported format major_version {major_version}, expected {P2W_FORMAT_VER_MAJOR}"
            )
            .into());
        }

        let mut minor_version_vec = vec![0u8; mem::size_of_val(&P2W_FORMAT_VER_MINOR)];
        bytes.read_exact(minor_version_vec.as_mut_slice())?;
        let minor_version = u16::from_be_bytes(minor_version_vec.as_slice().try_into()?);

        // Only older minors are not okay for this codebase
        if minor_version < P2W_FORMAT_VER_MINOR {
            return Err(format!(
                "Unsupported format minor_version {minor_version}, expected {P2W_FORMAT_VER_MINOR} or more"
            )
            .into());
        }

        // Read header size value
        let mut hdr_size_vec = vec![0u8; mem::size_of_val(&P2W_FORMAT_HDR_SIZE)];
        bytes.read_exact(hdr_size_vec.as_mut_slice())?;
        let hdr_size = u16::from_be_bytes(hdr_size_vec.as_slice().try_into()?);

        // Consume the declared number of remaining header
        // bytes. Remaining header fields must be read from hdr_buf
        let mut hdr_buf = vec![0u8; hdr_size as usize];
        bytes.read_exact(hdr_buf.as_mut_slice())?;

        let mut payload_id_vec = vec![0u8; mem::size_of::<PayloadId>()];
        hdr_buf
            .as_slice()
            .read_exact(payload_id_vec.as_mut_slice())?;

        if payload_id_vec[0] != PayloadId::AccumulationAttestation as u8 {
            return Err(format!(
                "Invalid Payload ID {}, expected {}",
                payload_id_vec[0],
                PayloadId::AccumulationAttestation as u8,
            )
            .into());
        }

        // Header consumed, continue with remaining fields
        // let mut accum_len_vec = vec![0u8; mem::size_of::<u16>()];
        // bytes.read_exact(accum_len_vec.as_mut_slice())?;
        // let accum_len = u16::from_be_bytes(accum_len_vec.as_slice().try_into()?);
        //
        // // let accum_vec = Vec::with_capacity(accum_len_vec as usize);
        // let mut accum_vec = vec![0u8; accum_len as usize];
        // bytes.read_exact(accum_vec.as_mut_slice())?;
        // let accumulator =
        //     match <MerkleTree as BorshDeserialize>::deserialize(&mut accum_vec.as_slice()) {
        //         Ok(acc) => acc,
        //         Err(e) => return Err(format!("AccumulatorDeserialization failed: {}", e).into()),
        //     };

        let mut accum_len_vec = vec![0u8; mem::size_of::<u16>()];
        bytes.read_exact(accum_len_vec.as_mut_slice())?;
        let accum_len = u16::from_be_bytes(accum_len_vec.as_slice().try_into()?);

        // let accum_vec = Vec::with_capacity(accum_len_vec as usize);
        let mut accum_vec = vec![0u8; accum_len as usize];
        bytes.read_exact(accum_vec.as_mut_slice())?;
        let accumulator = match bincode::deserialize(accum_vec.as_slice()) {
            Ok(acc) => acc,
            Err(e) => return Err(format!("AccumulatorDeserialization failed: {}", e).into()),
        };

        let mut ring_buff_idx_vec = vec![0u8; mem::size_of::<u64>()];
        bytes.read_exact(ring_buff_idx_vec.as_mut_slice())?;
        let ring_buffer_idx = u64::from_be_bytes(ring_buff_idx_vec.as_slice().try_into()?);

        let mut height_vec = vec![0u8; mem::size_of::<u64>()];
        bytes.read_exact(height_vec.as_mut_slice())?;
        let height = u64::from_be_bytes(height_vec.as_slice().try_into()?);

        let mut timestamp_vec = vec![0u8; mem::size_of::<i64>()];
        bytes.read_exact(timestamp_vec.as_mut_slice())?;
        let timestamp = i64::from_be_bytes(timestamp_vec.as_slice().try_into()?);

        Ok(Self {
            accumulator,
            ring_buffer_idx,
            height,
            timestamp,
        })
    }
}

pub fn use_to_string<T, S>(val: &T, s: S) -> Result<S::Ok, S::Error>
where
    T: ToString,
    S: Serializer,
{
    s.serialize_str(&val.to_string())
}

pub fn pubkey_to_hex<S>(val: &Identifier, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    s.serialize_str(&hex::encode(val.to_bytes()))
}

#[derive(
    Copy,
    Clone,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    BorshSerialize,
    BorshDeserialize,
    serde::Serialize,
    serde::Deserialize,
)]
#[repr(C)]
pub struct Identifier(#[serde(with = "hex")] [u8; 32]);

impl Identifier {
    pub fn new(bytes: [u8; 32]) -> Identifier {
        Identifier(bytes)
    }

    pub fn to_bytes(&self) -> [u8; 32] {
        self.0
    }

    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    pub fn from_hex<T: AsRef<[u8]>>(s: T) -> Result<Identifier, FromHexError> {
        let mut bytes = [0u8; 32];
        hex::decode_to_slice(s, &mut bytes)?;
        Ok(Identifier::new(bytes))
    }
}

impl fmt::Debug for Identifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{}", self.to_hex())
    }
}

impl fmt::Display for Identifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{}", self.to_hex())
    }
}

impl AsRef<[u8]> for Identifier {
    fn as_ref(&self) -> &[u8] {
        &self.0[..]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accumulators::Accumulator;
    // use crate::accumulator::PayloadId::AccumulationAttestation;
    use crate::pyth::*;

    pub fn new_unique_pubkey() -> RawPubkey {
        use rand::Rng;
        rand::thread_rng().gen::<[u8; 32]>()
    }

    impl Default for pyth::AccountHeader {
        fn default() -> Self {
            Self {
                magic_number: crate::pyth::PC_MAGIC,
                version: 0,
                account_type: crate::pyth::PC_ACCTYPE_PRICE,
                size: 0,
            }
        }
    }

    // TODO: - keep current cfg_attr or implement it all here?
    // impl Default for crate::pyth::PriceInfo {
    //     fn default() -> Self {
    //         Self {
    //            todo!()
    //         }
    //     }
    // }
    // impl Default for crate::pyth::PriceEma {
    //     fn default() -> Self {
    //         Self {
    //            todo!()
    //         }
    //     }
    // }
    // impl Default for crate::pyth::PriceComponent {
    //     fn default() -> Self {
    //         Self {
    //            todo!()
    //          }
    //     }
    // }
    //
    // impl Default for crate::pyth::PriceAccount {
    //     fn default() -> Self {
    //         Self {
    //             ..Default::default()
    //         }
    //     }
    // }
    //

    impl AccountHeader {
        fn new(account_type: u32) -> Self {
            Self {
                account_type,
                ..AccountHeader::default()
            }
        }
    }

    // only using the price_type field for hashing for merkle tree.
    fn generate_price_account(price_type: u32) -> (RawPubkey, PriceAccount) {
        (
            new_unique_pubkey(),
            PriceAccount {
                price_type,
                header: AccountHeader::new(PC_ACCTYPE_PRICE),
                ..PriceAccount::default()
            },
        )
    }

    #[test]
    fn test_pa_default() {
        println!("testing pa");
        let acct_header = AccountHeader::default();
        println!("acct_header.acct_type: {}", acct_header.account_type);
        let pa = PriceAccount::default();
        println!("price_account.price_type: {}", pa.price_type);
    }

    #[test]
    fn test_new_accumulator() {
        let price_accts_and_keys = (0..2)
            .map(|i| generate_price_account(i))
            .collect::<Vec<_>>();
        let t = price_accts_and_keys
            .iter()
            .map(|(pk, pa)| (*pk, pa))
            .into_iter();
        let acc = MerkleTree::new_merkle(t);
        println!("acc: {acc:#?}\nproofs:{:?}", acc.proof())
    }

    #[test]
    fn test_accumulator_attest_serde() -> Result<(), ErrBox> {
        // let price_accts_and_keys: Vec<(Pubkey, PriceAccount)> =
        //     (0..2).map(|i| generate_price_account(i)).collect();
        // let price_accts: Vec<&PriceAccount> =
        //     price_accts_and_keys.iter().map(|(_, pa)| pa).collect();
        let price_accts_and_keys = (0..2)
            .map(|i| generate_price_account(i))
            .collect::<Vec<_>>();
        let accum_input = price_accts_and_keys
            .iter()
            .map(|(pk, pa)| (*pk, pa))
            .into_iter();
        // let (accumulator, proofs) = MerkleTree::new_merkle(accum_input);
        let accumulator = MerkleTree::new_merkle(accum_input);

        // arbitrary values
        let ring_buffer_idx = 17;
        let height = 28;
        let timestamp = 294;

        let accumulator_attest = AccumulatorAttestation {
            accumulator: accumulator.root,
            ring_buffer_idx,
            height,
            timestamp,
        };

        println!("accumulator attest hex struct:  {accumulator_attest:#02X?}");

        let serialized = accumulator_attest.serialize()?;
        println!("accumulator attest hex bytes: {serialized:02X?}");

        let deserialized = AccumulatorAttestation::deserialize(serialized.as_slice())?;

        println!("deserialized accumulator attest hex struct:  {deserialized:#02X?}");
        assert_eq!(accumulator_attest, deserialized);
        Ok(())
    }

    #[test]
    fn test_wormhole_unreliable_message_serialize() {
        let price_accts_and_keys = (0..2)
            .map(|i| generate_price_account(i))
            .collect::<Vec<_>>();
        let accum_input = price_accts_and_keys
            .iter()
            .map(|(pk, pa)| (*pk, pa))
            .into_iter();
        // let (accumulator, proofs) = MerkleTree::new_merkle(accum_input);

        // let (accumulator, proofs) = MerkleTree::new_merkle(accum_input);
        let accumulator = MerkleTree::new_merkle(accum_input);
        // arbitrary values
        let ring_buffer_idx = 17;
        let height = 28;
        let timestamp = 294;

        let accumulator_attestation = AccumulatorAttestation {
            accumulator: accumulator.root,
            ring_buffer_idx,
            height,
            timestamp,
        };

        let msg_data = crate::wormhole::PostedMessageUnreliableData {
            message: crate::wormhole::MessageData {
                vaa_version: 1,
                consistency_level: 1,
                vaa_time: 1u32,
                vaa_signature_account: new_unique_pubkey(),
                submission_time: 1u32,
                nonce: 0,
                //TODO: handle this
                sequence: 500,
                emitter_chain: 26,
                //TODO: handle this
                emitter_address: new_unique_pubkey(),
                payload: accumulator_attestation.serialize().unwrap(),
            },
        };

        let mut account_data = vec![];
        msg_data.serialize(&mut account_data).unwrap();
        println!("account_data: {account_data:02X?}");

        let deserialized =
            crate::wormhole::PostedMessageUnreliableData::deserialize(&mut account_data.as_slice())
                .unwrap();

        assert_eq!(
            msg_data.message.vaa_signature_account,
            deserialized.message.vaa_signature_account
        );
        assert_eq!(
            msg_data.message.emitter_chain,
            deserialized.message.emitter_chain
        );
        assert_eq!(
            msg_data.message.emitter_address,
            deserialized.message.emitter_address
        );
        let original_accumulator_root = accumulator.root;
        let msg_data_accum: AccumulatorAttestation<Hash> =
            AccumulatorAttestation::deserialize(&mut msg_data.message.payload.as_slice()).unwrap();
        let deserialized_msg_data_accum: AccumulatorAttestation<Hash> =
            AccumulatorAttestation::deserialize(&mut deserialized.message.payload.as_slice())
                .unwrap();
        println!(
            r"
                original_accumulator_root: {:?}, 
                msg_data_accum.accumulator: {:?},
                deserialized_msg_data_accum.accumulator: {:?}
            ",
            original_accumulator_root,
            msg_data_accum.accumulator,
            deserialized_msg_data_accum.accumulator
        );
        assert_eq!(original_accumulator_root, msg_data_accum.accumulator);
        assert_eq!(msg_data_accum, deserialized_msg_data_accum);
    }
}
