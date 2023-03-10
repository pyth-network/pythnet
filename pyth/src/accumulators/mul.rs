use crate::accumulators::Accumulator2;
use crate::hashers::prime::PrimeHasher;
use crate::hashers::Hasher;
use std::hash::Hash;

pub struct MulAccumulator<H: Hasher> {
    pub accumulator: H::Hash,
    pub items: Vec<H::Hash>,
}

impl<'a> Accumulator2<'a> for MulAccumulator<PrimeHasher> {
    type Proof = <PrimeHasher as Hasher>::Hash;

    fn prove(&self, item: &[u8]) -> Option<Self::Proof> {
        let bytes = PrimeHasher::hash(item);
        Some(self.accumulator / bytes as u128)
    }

    fn verify(&self, proof: Self::Proof, item: &[u8]) -> Option<bool> {
        let bytes = PrimeHasher::hash(item);
        Some(proof * bytes as u128 == self.accumulator)
    }

    fn from_set(items: impl Iterator<Item = &'a &'a [u8]>) -> Option<Self> {
        let primes: Vec<u128> = items.map(|i| PrimeHasher::hash(i)).collect();
        Some(Self {
            items: primes.clone(),
            accumulator: primes.into_iter().reduce(|acc, v| acc * v)?,
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_membership() {
        let mut set: HashSet<&[u8]> = HashSet::new();

        // Create some random elements (converted to bytes). All accumulators store arbitrary bytes so
        // that we can target any account (or subset of accounts).
        let item_a = 33usize.to_be_bytes();
        let item_b = 54usize.to_be_bytes();
        let item_c = 2usize.to_be_bytes();
        let item_d = 88usize.to_be_bytes();

        // Insert the bytes into the Accumulate type.
        set.insert(&item_a);
        set.insert(&item_b);
        set.insert(&item_c);

        println!();

        // Create an Accumulator. Test Membership.
        {
            let accumulator = MulAccumulator::<PrimeHasher>::from_set(set.iter()).unwrap();
            let proof = accumulator.prove(&item_a).unwrap();
            println!("Mul:");
            println!("Proof:  {:?}", accumulator.verify(proof, &item_a));
            println!("Proof:  {:?}", accumulator.verify(proof, &item_d));
            assert!(accumulator.verify(proof, &item_a));
            assert!(!accumulator.verify(proof, &item_d));
        }
    }
}
