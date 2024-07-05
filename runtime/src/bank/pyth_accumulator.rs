use std::env::{self, VarError};

use solana_sdk::pubkey::Pubkey;

use crate::accounts_index::{ScanConfig, ScanError};

use super::Bank;

#[derive(Debug, thiserror::Error)]
pub enum AccumulatorUpdateV2Error {
    #[error("no oracle pubkey")]
    NoOraclePubkey,
    #[error("get_program_accounts failed to return accounts: {0}")]
    GetProgramAccounts(#[from] ScanError),
}

lazy_static! {
    static ref ORACLE_PUBKEY: Option<Pubkey> = match env::var("PYTH_ORACLE_PUBKEY") {
        Ok(value) => Some(
            value
                .parse()
                .expect("invalid value of PYTH_ORACLE_PUBKEY env var")
        ),
        Err(VarError::NotPresent) => None,
        Err(VarError::NotUnicode(err)) => {
            panic!("invalid value of PYTH_ORACLE_PUBKEY env var: {err:?}");
        }
    };
}

pub fn update_v2(bank: &Bank) -> std::result::Result<(), AccumulatorUpdateV2Error> {
    let Some(oracle_pubkey) = &*ORACLE_PUBKEY else {
        return Err(AccumulatorUpdateV2Error::NoOraclePubkey);
    };

    let accounts = bank
        .get_program_accounts(oracle_pubkey, &ScanConfig::new(true))
        .map_err(AccumulatorUpdateV2Error::GetProgramAccounts)?;

    //...
    Ok(())
}
