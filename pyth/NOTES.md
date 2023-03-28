Pending Tasks:

- [] implement API for:
  - [] serializing "root" as well as an entire accumulator
  - [] Accumulator Proofs
    - [] MerkleAccumulator/MulAccumulator.items if not keeping as slice of bytes
- [] sync up if geyser plugin will handle generation of proof pdas
- [] implement parallelized fetching of price feed ids/accounts
  future optimization. not needed at currently magnitude of accounts to fetch (< 1000)
- [] error handling during accumulator/proof generation (`unwrap()`)
- [] adjust `measure!` logs in `bank.rs`
- [] add back serde-related tests
- [] ciborium/protobuf/flatbuffer for projection/compressing wire format
  - verkle tree, rsa/bilinear accumulator?
- [] bank.rs: get_price_accounts() -> get_accumulator_accounts()
  - [] "mapping"(vec) of accounts to read
  - [] pyth-admin - add/remove from address book(accumulator account list)

## Notes

1. Accumulator & Proof Serialization

```
   merkle_tree:
		`accumulator.accumulator.get_root()`(root hash) goes into VAA
		proof_pda_v1: {
			Vec<(Key, MerkleTreeProof)>
			where
			TODO: key for handling projections
			Key = PriceId(Pubkey)
			MerkleTreeProof = Vec<MerkleNode<Hasher>>
		}
		this wastes a lot of space due to duplication of nodes

		proof_pda_v2: {
			accumulator: MerkleTree,
			proofs: Vec<(PriceId, MerkleTreeProof_v2)>
			where MerkleTreeProof_v2 = usize = index of leaf to reconstruct MerklePath(on-demand generation)
		}

	mul_accumulator:
		`accumulator.accumulator` goes into VAA (product of hash of all items)
		proof_pda: {
			Vec<(Key, MulProof)>
			where MulProof = accumulator.accumulator/bytes as u128
		}
```

2. Implement the `hashers::Hasher` for the hashing algorithm to be used in an Accumulator

```rust
//This example shows how to implement the sha256 algorithm


use crate::accumulators::Hasher;
use sha3::{Sha3_256, Digest, digest::FixedOutput};

#[derive(Clone, Default, Debug, serde::Serialize)]
pub struct Sha256Algorithm {}

impl Hasher for Sha256Algorithm {
    type Hash = [u8; 32];

    fn hashv<T: AsRef<[u8]>>(data: &[T]) {
        let mut hasher = Sha3_256::new();
        for d in data {
            hasher.update(d);
       }
        <[u8; 32]>::from(hasher.finalize_fixed())
    }
}
```

3.
