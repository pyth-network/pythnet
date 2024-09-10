use {
    crate::{
        bank::pyth::{
            accumulator::{BATCH_PUBLISH_PID, ORACLE_PID},
            tests::{create_new_bank_for_tests_with_index, new_from_parent},
        },
        genesis_utils::{create_genesis_config_with_leader, GenesisConfigInfo},
    },
    bytemuck::{cast_slice, checked::from_bytes},
    pyth_oracle::{
        solana_program::account_info::AccountInfo, PriceAccount, PriceAccountFlags, PythAccount,
    },
    pyth_price_store::{
        accounts::{
            buffer::{self, BufferedPrice},
            publisher_config,
        },
        instruction::PUBLISHER_CONFIG_SEED,
    },
    solana_sdk::{
        account::{AccountSharedData, ReadableAccount, WritableAccount},
        clock::Epoch,
        epoch_schedule::EpochSchedule,
        feature_set,
        pubkey::Pubkey,
        signature::keypair_from_seed,
        signer::Signer,
    },
    std::{mem::size_of, sync::Arc},
};

#[test]
fn test_batch_publish() {
    let leader_pubkey = solana_sdk::pubkey::new_rand();
    let GenesisConfigInfo {
        mut genesis_config, ..
    } = create_genesis_config_with_leader(5, &leader_pubkey, 3);

    // Set epoch length to 32 so we can advance epochs quickly. We also skip past slot 0 here
    // due to slot 0 having special handling.
    let slots_in_epoch = 32;
    genesis_config.epoch_schedule = EpochSchedule::new(slots_in_epoch);
    let mut bank = create_new_bank_for_tests_with_index(&genesis_config);

    let generate_publisher = |seed, seed2, new_prices| {
        let publisher_key = keypair_from_seed(seed).unwrap();

        let (publisher_config_key, _bump) = Pubkey::find_program_address(
            &[
                PUBLISHER_CONFIG_SEED.as_bytes(),
                &publisher_key.pubkey().to_bytes(),
            ],
            &BATCH_PUBLISH_PID,
        );
        let publisher_buffer_key =
            Pubkey::create_with_seed(&leader_pubkey, seed2, &BATCH_PUBLISH_PID).unwrap();

        let mut publisher_config_account =
            AccountSharedData::new(42, publisher_config::SIZE, &BATCH_PUBLISH_PID);

        publisher_config::create(
            publisher_config_account.data_mut(),
            publisher_key.pubkey().to_bytes(),
            publisher_buffer_key.to_bytes(),
        )
        .unwrap();
        bank.store_account(&publisher_config_key, &publisher_config_account);

        let mut publisher_buffer_account =
            AccountSharedData::new(42, buffer::size(100), &BATCH_PUBLISH_PID);
        {
            let (header, prices) = buffer::create(
                publisher_buffer_account.data_mut(),
                publisher_key.pubkey().to_bytes(),
            )
            .unwrap();
            buffer::update(header, prices, bank.slot(), cast_slice(new_prices)).unwrap();
        }
        bank.store_account(&publisher_buffer_key, &publisher_buffer_account);

        publisher_key
    };

    let publishers = [
        generate_publisher(
            &[1u8; 32],
            "seed1",
            &[
                BufferedPrice::new(1, 1, 10, 2).unwrap(),
                BufferedPrice::new(2, 1, 20, 3).unwrap(),
                // Attempt to publish with price_index == 0,
                // but it will not be applied.
                BufferedPrice {
                    trading_status_and_feed_index: 0,
                    price: 30,
                    confidence: 35,
                },
            ],
        ),
        generate_publisher(
            &[2u8; 32],
            "seed2",
            &[
                BufferedPrice::new(1, 1, 15, 2).unwrap(),
                BufferedPrice::new(2, 1, 25, 3).unwrap(),
            ],
        ),
    ];

    let generate_price = |seeds, index| {
        let (price_feed_key, _bump) = Pubkey::find_program_address(&[seeds], &ORACLE_PID);
        let mut price_feed_account =
            AccountSharedData::new(42, size_of::<PriceAccount>(), &ORACLE_PID);

        let messages = {
            let price_feed_info_key = &price_feed_key.to_bytes().into();
            let price_feed_info_lamports = &mut 0;
            let price_feed_info_owner = &ORACLE_PID.to_bytes().into();
            let price_feed_info_data = price_feed_account.data_mut();
            let price_feed_info = AccountInfo::new(
                price_feed_info_key,
                false,
                true,
                price_feed_info_lamports,
                price_feed_info_data,
                price_feed_info_owner,
                false,
                Epoch::default(),
            );

            let mut price_account = PriceAccount::initialize(&price_feed_info, 0).unwrap();
            price_account.flags.insert(
                PriceAccountFlags::ACCUMULATOR_V2 | PriceAccountFlags::MESSAGE_BUFFER_CLEARED,
            );
            price_account.feed_index = index;
            price_account.comp_[0].pub_ = publishers[0].pubkey().to_bytes().into();
            price_account.comp_[1].pub_ = publishers[1].pubkey().to_bytes().into();
            price_account.num_ = 2;
        };

        bank.store_account(&price_feed_key, &price_feed_account);
        (price_feed_key, messages)
    };

    assert!(bank
        .feature_set
        .is_active(&feature_set::enable_accumulator_sysvar::id()));
    assert!(bank
        .feature_set
        .is_active(&feature_set::move_accumulator_to_end_of_block::id()));
    assert!(bank
        .feature_set
        .is_active(&feature_set::undo_move_accumulator_to_end_of_block::id()));
    assert!(bank
        .feature_set
        .is_active(&feature_set::redo_move_accumulator_to_end_of_block::id()));

    let prices_with_messages = [
        generate_price(b"seeds_1", 1),
        generate_price(b"seeds_2", 2),
        generate_price(b"seeds_3", 3),
        generate_price(b"seeds_4", 0),
    ];

    bank = new_from_parent(&Arc::new(bank)); // Advance slot 1.
    bank = new_from_parent(&Arc::new(bank)); // Advance slot 2.

    let new_price_feed1_account = bank.get_account(&prices_with_messages[0].0).unwrap();
    let new_price_feed1_data: &PriceAccount = from_bytes(new_price_feed1_account.data());
    assert_eq!(new_price_feed1_data.comp_[0].latest_.price_, 10);
    assert_eq!(new_price_feed1_data.comp_[0].latest_.conf_, 2);
    assert_eq!(new_price_feed1_data.comp_[0].latest_.status_, 1);
    assert_eq!(new_price_feed1_data.comp_[1].latest_.price_, 15);

    let new_price_feed2_account = bank.get_account(&prices_with_messages[1].0).unwrap();
    let new_price_feed2_data: &PriceAccount = from_bytes(new_price_feed2_account.data());
    assert_eq!(new_price_feed2_data.comp_[0].latest_.price_, 20);
    assert_eq!(new_price_feed2_data.comp_[1].latest_.price_, 25);

    let new_price_feed4_account = bank.get_account(&prices_with_messages[3].0).unwrap();
    let new_price_feed4_data: &PriceAccount = from_bytes(new_price_feed4_account.data());
    assert_eq!(new_price_feed4_data.comp_[0].latest_.price_, 0);
}
