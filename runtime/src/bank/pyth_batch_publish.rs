use {
    super::{pyth_accumulator::BATCH_PUBLISH_PID, Bank},
    crate::accounts_index::{IndexKey, ScanConfig, ScanError},
    log::warn,
    pyth_oracle::{
        find_publisher_index, get_status_for_conf_price_ratio, solana_program::pubkey::Pubkey,
        OracleError, PriceAccount,
    },
    solana_sdk::{account::ReadableAccount, clock::Slot},
    std::collections::HashMap,
    thiserror::Error,
};

#[allow(dead_code)]
pub mod publisher_prices_account {
    use {
        bytemuck::{cast_slice, from_bytes, from_bytes_mut, Pod, Zeroable},
        solana_sdk::clock::Slot,
        std::mem::size_of,
        thiserror::Error,
    };

    const FORMAT: u32 = 2848712303;

    #[derive(Debug, Clone, Copy, Zeroable, Pod)]
    #[repr(C, packed)]
    pub struct PublisherPricesHeader {
        pub format: u32,
        pub publisher: [u8; 32],
        pub slot: Slot,
        pub num_prices: u32,
    }

    impl PublisherPricesHeader {
        fn new(publisher: [u8; 32]) -> Self {
            PublisherPricesHeader {
                format: FORMAT,
                publisher,
                slot: 0,
                num_prices: 0,
            }
        }
    }

    #[derive(Debug, Clone, Copy, Zeroable, Pod)]
    #[repr(C, packed)]
    pub struct PublisherPrice {
        // 4 high bits: trading status
        // 28 low bits: feed index
        pub trading_status_and_feed_index: u32,
        pub price: i64,
        pub confidence: u64,
    }

    #[derive(Debug, Error)]
    #[error("publisher price data overflow")]
    pub struct PublisherPriceError;

    impl PublisherPrice {
        pub fn new(
            feed_index: u32,
            trading_status: u32,
            price: i64,
            confidence: u64,
        ) -> Result<Self, PublisherPriceError> {
            if feed_index >= (1 << 28) || trading_status >= (1 << 4) {
                return Err(PublisherPriceError);
            }
            Ok(Self {
                trading_status_and_feed_index: (trading_status << 28) | feed_index,
                price,
                confidence,
            })
        }

        pub fn trading_status(&self) -> u32 {
            self.trading_status_and_feed_index >> 28
        }

        pub fn feed_index(&self) -> u32 {
            self.trading_status_and_feed_index & ((1 << 28) - 1)
        }
    }

    #[derive(Debug, Error)]
    pub enum ReadAccountError {
        #[error("data too short")]
        DataTooShort,
        #[error("format mismatch")]
        FormatMismatch,
        #[error("invalid num prices")]
        InvalidNumPrices,
    }

    #[derive(Debug, Error)]
    pub enum ExtendError {
        #[error("not enough space")]
        NotEnoughSpace,
        #[error("invalid length")]
        InvalidLength,
    }

    pub fn read(
        data: &[u8],
    ) -> Result<(&PublisherPricesHeader, &[PublisherPrice]), ReadAccountError> {
        if data.len() < size_of::<PublisherPricesHeader>() {
            return Err(ReadAccountError::DataTooShort);
        }
        let header: &PublisherPricesHeader =
            from_bytes(&data[..size_of::<PublisherPricesHeader>()]);
        if header.format != FORMAT {
            return Err(ReadAccountError::FormatMismatch);
        }
        let prices_bytes = &data[size_of::<PublisherPricesHeader>()..];
        let num_prices: usize = header.num_prices.try_into().unwrap();
        let expected_len = num_prices.saturating_mul(size_of::<PublisherPrice>());
        if expected_len > prices_bytes.len() {
            return Err(ReadAccountError::InvalidNumPrices);
        }
        let prices = cast_slice(&prices_bytes[..expected_len]);
        Ok((header, prices))
    }

    pub fn size(max_prices: usize) -> usize {
        size_of::<PublisherPricesHeader>() + max_prices * size_of::<PublisherPrice>()
    }

    pub fn read_mut(
        data: &mut [u8],
    ) -> Result<(&mut PublisherPricesHeader, &mut [u8]), ReadAccountError> {
        if data.len() < size_of::<PublisherPricesHeader>() {
            return Err(ReadAccountError::DataTooShort);
        }
        let (header, prices) = data.split_at_mut(size_of::<PublisherPricesHeader>());
        let header: &mut PublisherPricesHeader = from_bytes_mut(header);
        if header.format != FORMAT {
            return Err(ReadAccountError::FormatMismatch);
        }
        Ok((header, prices))
    }

    pub fn create(
        data: &mut [u8],
        publisher: [u8; 32],
    ) -> Result<(&mut PublisherPricesHeader, &mut [u8]), ReadAccountError> {
        if data.len() < size_of::<PublisherPricesHeader>() {
            return Err(ReadAccountError::DataTooShort);
        }
        let (header, prices) = data.split_at_mut(size_of::<PublisherPricesHeader>());
        let header: &mut PublisherPricesHeader = from_bytes_mut(header);
        *header = PublisherPricesHeader::new(publisher);
        Ok((header, prices))
    }

    pub fn extend(
        header: &mut PublisherPricesHeader,
        prices: &mut [u8],
        new_prices: &[u8],
    ) -> Result<(), ExtendError> {
        if new_prices.len() % size_of::<PublisherPrice>() != 0 {
            return Err(ExtendError::InvalidLength);
        }
        let num_new_prices = (new_prices.len() / size_of::<PublisherPrice>())
            .try_into()
            .expect("unexpected overflow");
        let num_prices: usize = header.num_prices.try_into().unwrap();
        let start = size_of::<PublisherPrice>() * num_prices;
        let end = size_of::<PublisherPrice>() * num_prices + new_prices.len();
        header.num_prices = header
            .num_prices
            .checked_add(num_new_prices)
            .expect("unexpected overflow");
        prices
            .get_mut(start..end)
            .ok_or(ExtendError::NotEnoughSpace)?
            .copy_from_slice(new_prices);
        Ok(())
    }
}

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
    for (account_key, account) in publisher_prices_accounts {
        let (header, prices) = match publisher_prices_account::read(account.data()) {
            Ok(r) => r,
            Err(err) => {
                warn!("invalid publisher prices account {}: {}", account_key, err);
                continue;
            }
        };
        if header.slot != bank.slot() {
            // Updates from earlier slots have already been applied.
            continue;
        }
        let publisher = header.publisher.into();
        for price in prices {
            all_prices
                .entry(price.feed_index())
                .or_default()
                .push(PublisherPriceValue {
                    publisher,
                    trading_status: price.trading_status(),
                    price: price.price,
                    confidence: price.confidence,
                });
        }
    }
    Ok(all_prices)
}

pub fn apply_published_prices(
    price_data: &mut PriceAccount,
    new_prices: &HashMap<u32, Vec<PublisherPriceValue>>,
    slot: Slot,
) -> bool {
    // TODO: store index here or somewhere else?
    let price_feed_index = price_data.unused_3_ as u32;
    let mut any_update = false;
    for new_price in new_prices.get(&price_feed_index).unwrap_or(&Vec::new()) {
        match apply_published_price(price_data, new_price, slot) {
            Ok(()) => {
                any_update = true;
            }
            Err(err) => {
                warn!(
                    "failed to apply publisher price to price feed {}: {}",
                    price_data.unused_3_ as u32, err
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
        ApplyPublishedPriceError::NoPermission(price_data.unused_3_ as u32, new_price.publisher),
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
