use ft_io::*;
use fungible_token::WASM_BINARY_OPT;
use gclient::{EventListener, EventProcessor, GearApi, Result};
use gstd::Encode;

const USERS: &[u64] = &[3, 4, 5];

#[tokio::test]
#[ignore]
async fn mint_test() -> Result<()> {
    let api = GearApi::dev().await?;

    let mut listener = api.subscribe().await?; // Subscribing for events.

    // Checking that blocks still running.
    assert!(listener.blocks_running().await?);

    // Init
    let init_nft = InitConfig {
        name: String::from("MyToken"),
        symbol: String::from("MTK"),
        decimals: 18,
    }
    .encode();

    let gas_info = api
        .calculate_upload_gas(None, WASM_BINARY_OPT.to_vec(), init_nft.clone(), 0, true)
        .await?;

    let (message_id, program_id, _hash) = api
        .upload_program_bytes(
            WASM_BINARY_OPT.to_vec(),
            gclient::now_micros().to_le_bytes(),
            init_nft,
            gas_info.min_limit,
            0,
        )
        .await?;

    assert!(listener.message_processed(message_id).await?.succeed());

    // Mint
    let mint = FTAction::Mint(1_000_000);
    let gas_info = api
        .calculate_handle_gas(None, program_id, mint.encode(), 0, true)
        .await?;

    let (message_id, _) = api
        .send_message(program_id, mint, gas_info.min_limit, 0)
        .await?;

    assert!(listener.message_processed(message_id).await?.succeed());

    assert!(listener.blocks_running().await?);

    // Approve
    let approve = FTAction::Approve {
        to: USERS[1].into(),
        amount: 500,
    };

    let gas_info = api
        .calculate_handle_gas(None, program_id, approve.encode(), 0, true)
        .await?;

    let (message_id, _) = api
        .send_message(program_id, approve, gas_info.min_limit, 0)
        .await?;

    assert!(listener.message_processed(message_id).await?.succeed());

    assert!(listener.blocks_running().await?);

    Ok(())
}

#[tokio::test]
#[ignore]
async fn migration_test() -> Result<()> {
    let api = GearApi::dev().await?;

    let mut listener = api.subscribe().await?; // Subscribing for events.

    // Checking that blocks still running.
    assert!(listener.blocks_running().await?);

    // Init
    let init_ft = InitConfig {
        name: String::from("MyToken"),
        symbol: String::from("MTK"),
        decimals: 18,
    }
    .encode();

    let gas_info = api
        .calculate_upload_gas(None, WASM_BINARY_OPT.to_vec(), init_ft.clone(), 0, true)
        .await?;

    let (message_id, program_id, _hash) = api
        .upload_program_bytes(
            WASM_BINARY_OPT.to_vec(),
            gclient::now_micros().to_le_bytes(),
            init_ft,
            gas_info.min_limit,
            0,
        )
        .await?;

    assert!(listener.message_processed(message_id).await?.succeed());

    print_gas_info(&api, program_id.into_bytes(), 1, 0).await;
    print_gas_info(&api, program_id.into_bytes(), 10, 0).await;
    print_gas_info(&api, program_id.into_bytes(), 100, 0).await;
    print_gas_info(&api, program_id.into_bytes(), 1000, 0).await;
    print_gas_info(&api, program_id.into_bytes(), 10_000, 0).await;
    // No passed because Not enough gas to continue execution
    // print_gas_info(&api, program_id.into_bytes(), 100_000, 0).await;

    print_gas_info(&api, program_id.into_bytes(), 1, 1).await;
    print_gas_info(&api, program_id.into_bytes(), 10, 1).await;
    print_gas_info(&api, program_id.into_bytes(), 100, 1).await;
    print_gas_info(&api, program_id.into_bytes(), 1000, 1).await;
    print_gas_info(&api, program_id.into_bytes(), 10_000, 1).await;
    // No passed because Not enough gas to continue execution
    // print_gas_info(&api, program_id.into_bytes(), 100_000, 1).await;

    test_migration_with_balances(&api, &mut listener, program_id.into_bytes(), 1).await?;
    test_migration_with_balances(&api, &mut listener, program_id.into_bytes(), 10).await?;
    test_migration_with_balances(&api, &mut listener, program_id.into_bytes(), 100).await?;
    test_migration_with_balances(&api, &mut listener, program_id.into_bytes(), 1000).await?;
    test_migration_with_balances(&api, &mut listener, program_id.into_bytes(), 10_000).await?;
    // No passed because Not enough gas to continue execution
    // test_migration_with_balances(&api, &mut listener, program_id.into_bytes(), 100_000).await?;

    test_migration_with_len(&api, &mut listener, program_id.into_bytes(), 1, 1).await?;
    test_migration_with_len(&api, &mut listener, program_id.into_bytes(), 10, 1).await?;
    test_migration_with_len(&api, &mut listener, program_id.into_bytes(), 100, 1).await?;
    test_migration_with_len(&api, &mut listener, program_id.into_bytes(), 1000, 1).await?;
    test_migration_with_len(&api, &mut listener, program_id.into_bytes(), 10_000, 1).await?;
    // No passed because Not enough gas to continue execution
    // test_migration_with_len(&api, &mut listener, program_id.into_bytes(), 100_000, 1).await?;

    test_migration_with_len(&api, &mut listener, program_id.into_bytes(), 1, 1).await?;
    test_migration_with_len(&api, &mut listener, program_id.into_bytes(), 10, 10).await?;
    test_migration_with_len(&api, &mut listener, program_id.into_bytes(), 100, 100).await?;
    // No passed because Not enough gas to continue execution
    // test_migration_with_len(&api, &mut listener, program_id.into_bytes(), 1000, 1000).await?;
    // test_migration_with_len(&api, &mut listener, program_id.into_bytes(), 10_000, 10_000).await?;
    // test_migration_with_len(&api, &mut listener, program_id.into_bytes(), 100_000, 100_000).await?;

    test_migration_max_gas(&api, &mut listener, program_id.into_bytes(), 1, 1).await?;
    test_migration_max_gas(&api, &mut listener, program_id.into_bytes(), 10, 1).await?;
    test_migration_max_gas(&api, &mut listener, program_id.into_bytes(), 100, 1).await?;
    test_migration_max_gas(&api, &mut listener, program_id.into_bytes(), 1000, 1).await?;
    test_migration_max_gas(&api, &mut listener, program_id.into_bytes(), 10_000, 1).await?;
    // No passed because "Transaction would exhaust the block limits"
    // test_migration_max_gas(&api, &mut listener, program_id.into_bytes(), 100_000, 1).await?;
    // test_migration_max_gas(&api, &mut listener, program_id.into_bytes(), 1_000_000, 1).await?;

    test_migration_max_gas(&api, &mut listener, program_id.into_bytes(), 1, 1).await?;
    test_migration_max_gas(&api, &mut listener, program_id.into_bytes(), 10, 10).await?;
    test_migration_max_gas(&api, &mut listener, program_id.into_bytes(), 100, 100).await?;
    test_migration_max_gas(&api, &mut listener, program_id.into_bytes(), 1000, 1000).await?;
    // No passed because Not enough gas to continue execution
    // test_migration_max_gas(&api, &mut listener, program_id.into_bytes(), 10000, 10000).await?;

    Ok(())
}

async fn print_gas_info(
    api: &GearApi,
    program_id: [u8; 32],
    balances_len: u64,
    allowances_len: u64,
) {
    let balances = (0..balances_len).map(|v| (v.into(), v as u128)).collect();
    let allowances = (0..allowances_len)
        .map(|v| (v.into(), vec![(0.into(), v as u128); 1]))
        .collect();

    let new_ft = IoFungibleToken {
        name: "new-token".to_string(),
        symbol: "NT".to_string(),
        total_supply: 1_000_000,
        balances,
        allowances,
        decimals: 18,
    };

    let migrate = FTAction::MigrateFullState(new_ft);

    let gas_info = api
        .calculate_handle_gas(None, program_id.into(), migrate.encode(), 0, true)
        .await
        .unwrap();

    println!(
        "Gas for balances len {:0>6}, allowances len {:0>6}: {:?}",
        balances_len, allowances_len, gas_info
    );
}

async fn test_migration_max_gas(
    api: &GearApi,
    listener: &mut EventListener,
    program_id: [u8; 32],
    balances_len: u64,
    allowances_len: u64,
) -> Result<()> {
    let balances = (0..balances_len).map(|v| (v.into(), v as u128)).collect();
    let allowances = (0..allowances_len)
        .map(|v| (v.into(), vec![(0.into(), v as u128); 1]))
        .collect();

    let new_ft = IoFungibleToken {
        name: "new-token".to_string(),
        symbol: "NT".to_string(),
        total_supply: 1_000_000,
        balances,
        allowances,
        decimals: 18,
    };
    println!(
        "For balances len {:0>6}, allowances len {:0>6} with Max Gas 250_000_000_000",
        balances_len, allowances_len
    );
    let migrate = FTAction::MigrateFullState(new_ft);

    let (message_id, _) = api
        .send_message(program_id.into(), migrate, 250_000_000_000, 0)
        .await?;

    assert!(listener.message_processed(message_id).await?.succeed());

    assert!(listener.blocks_running().await?);

    Ok(())
}

async fn test_migration_with_len(
    api: &GearApi,
    listener: &mut EventListener,
    program_id: [u8; 32],
    balances_len: u64,
    allowances_len: u64,
) -> Result<()> {
    let balances = (0..balances_len).map(|v| (v.into(), v as u128)).collect();
    let allowances = (0..allowances_len)
        .map(|v| (v.into(), vec![(0.into(), v as u128); 1]))
        .collect();

    let new_ft = IoFungibleToken {
        name: "new-token".to_string(),
        symbol: "NT".to_string(),
        total_supply: 1_000_000,
        balances,
        allowances,
        decimals: 18,
    };

    let migrate = FTAction::MigrateFullState(new_ft);

    let gas_info = api
        .calculate_handle_gas(None, program_id.into(), migrate.encode(), 0, true)
        .await
        .unwrap();

    println!(
        "For balances len {:0>6}, allowances len {:0>6}: {:?}",
        balances_len, allowances_len, gas_info
    );

    let (message_id, _) = api
        .send_message(program_id.into(), migrate, gas_info.min_limit, 0)
        .await?;

    assert!(listener.message_processed(message_id).await?.succeed());

    assert!(listener.blocks_running().await?);

    Ok(())
}

async fn test_migration_with_balances(
    api: &GearApi,
    listener: &mut EventListener,
    program_id: [u8; 32],
    balances_len: u64,
) -> Result<()> {
    test_migration_with_len(api, listener, program_id, balances_len, 0).await
}
