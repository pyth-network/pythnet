use std::fmt::Debug;

pub mod keccak256;
pub mod prime;

// /// The types of values stored in a `Accumulator` must implement this trait,
// /// in order for them to be able to fed to a Ring `Context` when computing the hash of a leaf
// ///
// /// Default instnace for types that already implements `AsRef<[u8]>` is provided
// ///
// /// ## Example
// /// Here is example of how to implement `Hashable` for a type that does not (or cannot) implement `AsRef<[u8]>`:
// ///
// /// ```ignore
// /// impl Hashable for PublicKey {
// ///   fn update_context(&self, context: &mut Context) {
// ///      let bytes: Vec<u8> = self.to_bytes();
// ///.     context.update(&bytes)
// ///   }
// /// }
// ///```
// pub trait Hashable {
//     fn to_hash<H: Hasher>(&self, hasher: H) -> H::Hash;
// }
//
// //Blank implementation for Hashable for any type that implements AsRef<[u8]>
// impl<T: AsRef<[u8]>> Hashable for T {
//     fn to_hash<H: Hasher>(&self, hasher: H) -> H::Hash {
//         hasher.hash(self.as_ref())
//     }
// }
/// The type of values stored in an `Accumulator` must implement
/// this trait, in order for them to be able to be fed
/// to a `Hasher` when computing the hash of a value
///
/// A default instance for types that already implements
/// `AsRef<[u8]>` is provided.
///
/// ## Example
///
/// Here is an example of how to implement `Hashable` for a type
/// that does not (or cannot) implement `AsRef<[u8]>`:
///
/// ```ignore
/// impl<H: Hasher> Hashable<H> for PublicKey {
///     fn to_hash(&self) -> H::Hash {
///         let bytes: Vec<u8> = self.to_bytes();
///         H::hash(&[&bytes])
///     }
/// }
/// ```
// pub trait Hashable<H: Hasher> {
//     fn to_hash(&self) -> H::Hash;
// }
//
// //Blank implementation for Hashable for any type that implements AsRef<[u8]>
// impl<H: Hasher, T: AsRef<[u8]>> Hashable<H> for T {
//     fn to_hash(&self) -> H::Hash {
//         <H as Hasher>::hashv(&[self.as_ref()])
//     }
// }

/// Hasher is a trait used to provide a hashing algorithm for the library.
///
/// # Example
///
/// This example shows how to implement the sha256 algorithm
///
/// ```ignore
/// use crate::accumulators::Hasher;
/// use sha2::{Sha256, Digest, digest::FixedOutput};
///
/// #[derive(Clone)]
/// pub struct Sha256Algorithm {}
///
/// impl Hasher for Sha256Algorithm {
///     type Hash = [u8; 32];
///
///     fn hash(data: &[u8]) -> [u8; 32] {
///         let mut hasher = Sha256::new();
///
///         hasher.update(data);
///         <[u8; 32]>::from(hasher.finalize_fixed())
///     }
/// }
/// ```
pub trait Hasher: Clone + Default + Debug + serde::Serialize {
    /// This type is used as a hash type in the library.
    /// It is recommended to use fixed size u8 array as a hash type. For example,
    /// for sha256 the type would be `[u8; 32]`, representing 32 bytes,
    /// which is the size of the sha256 digest. Also, fixed sized arrays of `u8`
    /// by default satisfy all trait bounds required by this type.
    ///
    /// # Trait bounds
    /// `Copy` is required as the hash needs to be copied to be concatenated/propagated
    /// when constructing nodes.
    /// `PartialEq` is required to compare equality when verifying proof
    /// `Into<Vec<u8>>` is required to be able to serialize proof
    /// `TryFrom<Vec<u8>>` is required to parse hashes from a serialized proof
    /// `Default` is required to be able to create a default hash
    // TODO: use Digest trait from digest crate?
    // type Hash: Copy + PartialEq + Into<Vec<u8>> + TryFrom<Vec<u8>> + Default;
    type Hash: Copy
        + PartialEq
        + Default
        + Eq
        + Default
        + Debug
        + AsRef<[u8]>
        + serde::Serialize
        + for<'a> serde::de::Deserialize<'a>;
    fn hashv<T: AsRef<[u8]>>(data: &[T]) -> Self::Hash;
}
