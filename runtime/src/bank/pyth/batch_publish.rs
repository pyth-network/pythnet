use {
    super::accumulator::BATCH_PUBLISH_PID,
    crate::{
        accounts_index::{IndexKey, ScanConfig, ScanError},
        bank::Bank,
    },
    log::{info, warn},
    pyth_oracle::{
        find_publisher_index, get_status_for_conf_price_ratio, solana_program::pubkey::Pubkey,
        OracleError, PriceAccount,
    },
    pyth_price_publisher::accounts::publisher_prices as publisher_prices_account,
    solana_sdk::{account::ReadableAccount, clock::Slot},
    std::collections::HashMap,
    thiserror::Error,
};

#[derive(Debug, Error)]
pub enum HandleBatchPublishError {
    #[error("failed to get program accounts: {0}")]
    GetProgramAccounts(#[from] ScanError),
}

#[derive(Debug)]
pub struct PublisherPriceValue {
    pub publisher: Pubkey,
    pub trading_status: u32,
    pub price: i64,
    pub confidence: u64,
}

pub fn extract_batch_publish_prices(
    bank: &Bank,
) -> Result<HashMap<u32, Vec<PublisherPriceValue>>, HandleBatchPublishError> {
    assert!(
        bank.account_indexes_include_key(&*BATCH_PUBLISH_PID),
        "Oracle program account index missing"
    );

    let publisher_prices_accounts = bank
        .get_filtered_indexed_accounts(
            &IndexKey::ProgramId(*BATCH_PUBLISH_PID),
            |account| account.owner() == &*BATCH_PUBLISH_PID,
            &ScanConfig::new(true),
            None,
        )
        .map_err(HandleBatchPublishError::GetProgramAccounts)?;

    let mut all_prices = HashMap::<u32, Vec<PublisherPriceValue>>::new();
    let mut num_found_accounts = 0;
    let mut num_found_prices = 0;
    for (account_key, account) in publisher_prices_accounts {
        if !publisher_prices_account::format_matches(account.data()) {
            continue;
        }
        let (header, prices) = match publisher_prices_account::read(account.data()) {
            Ok(r) => r,
            Err(err) => {
                warn!("invalid publisher prices account {}: {}", account_key, err);
                continue;
            }
        };
        num_found_accounts += 1;
        if header.slot != bank.slot() {
            // Updates from earlier slots have already been applied.
            continue;
        }
        let publisher = header.publisher.into();
        for price in prices {
            let value = PublisherPriceValue {
                publisher,
                trading_status: price.trading_status(),
                price: price.price,
                confidence: price.confidence,
            };
            all_prices
                .entry(price.feed_index())
                .or_default()
                .push(value);
            num_found_prices += 1;
        }
    }
    info!(
        "pyth batch publish: found {} prices in {} accounts at slot {}",
        num_found_prices,
        num_found_accounts,
        bank.slot()
    );
    Ok(all_prices)
}

pub fn apply_published_prices(
    price_data: &mut PriceAccount,
    new_prices: &mut HashMap<u32, Vec<PublisherPriceValue>>,
    slot: Slot,
) -> bool {
    if price_data.feed_index == 0 {
        return false;
    }
    let mut any_update = false;
    for new_price in new_prices
        .remove(&price_data.feed_index)
        .unwrap_or_default()
    {
        match apply_published_price(price_data, &new_price, slot) {
            Ok(()) => {
                any_update = true;
            }
            Err(err) => {
                warn!(
                    "failed to apply publisher price to price feed {}: {}",
                    price_data.feed_index, err
                );
            }
        }
    }
    any_update
}

#[derive(Debug, Error)]
enum ApplyPublishedPriceError {
    #[error("publisher {1} is not allowed to publish prices for feed {0}")]
    NoPermission(u32, Pubkey),
    #[error("bad conf price ratio: {0}")]
    BadConfPriceRatio(#[from] OracleError),
    #[error("invalid publishers num_")]
    InvalidPublishersNum,
    #[error("invalid publisher index")]
    InvalidPublisherIndex,
}

fn apply_published_price(
    price_data: &mut PriceAccount,
    new_price: &PublisherPriceValue,
    slot: Slot,
) -> Result<(), ApplyPublishedPriceError> {
    let publishers = price_data
        .comp_
        .get(..price_data.num_.try_into().unwrap())
        .ok_or(ApplyPublishedPriceError::InvalidPublishersNum)?;

    let publisher_index = find_publisher_index(publishers, &new_price.publisher).ok_or(
        ApplyPublishedPriceError::NoPermission(price_data.feed_index, new_price.publisher),
    )?;

    // IMPORTANT: If the publisher does not meet the price/conf
    // ratio condition, its price will not count for the next
    // aggregate.
    let status: u32 = get_status_for_conf_price_ratio(
        new_price.price,
        new_price.confidence,
        new_price.trading_status,
    )?;

    let publisher_price = &mut price_data
        .comp_
        .get_mut(publisher_index)
        .ok_or(ApplyPublishedPriceError::InvalidPublisherIndex)?
        .latest_;
    publisher_price.price_ = new_price.price;
    publisher_price.conf_ = new_price.confidence;
    publisher_price.status_ = status;
    publisher_price.pub_slot_ = slot;
    Ok(())
}
