use {
    super::accumulator::{MESSAGE_BUFFER_PID, ORACLE_PID, PRICE_STORE_PID},
    crate::{
        accounts_db::AccountShrinkThreshold,
        accounts_index::{
            AccountIndex, AccountSecondaryIndexes, AccountSecondaryIndexesIncludeExclude,
        },
        bank::Bank,
    },
    solana_sdk::{genesis_config::GenesisConfig, pubkey::Pubkey},
    std::sync::Arc,
};

mod accumulator_tests;
mod batch_publish_tests;

fn new_from_parent(parent: &Arc<Bank>) -> Bank {
    Bank::new_from_parent(parent, &Pubkey::default(), parent.slot() + 1)
}

fn create_new_bank_for_tests_with_index(genesis_config: &GenesisConfig) -> Bank {
    Bank::new_with_config_for_tests(
        genesis_config,
        AccountSecondaryIndexes {
            keys: Some(AccountSecondaryIndexesIncludeExclude {
                exclude: false,
                keys: [*ORACLE_PID, *MESSAGE_BUFFER_PID, *PRICE_STORE_PID]
                    .into_iter()
                    .collect(),
            }),
            indexes: [AccountIndex::ProgramId].into_iter().collect(),
        },
        false,
        AccountShrinkThreshold::default(),
    )
}
