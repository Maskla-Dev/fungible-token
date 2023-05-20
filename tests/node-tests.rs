use ft_io::*;
use fungible_token::WASM_BINARY_OPT;
use gclient::{EventListener, EventProcessor, GearApi, Result};
use gstd::{ActorId, Encode};

const USERS: &[u64] = &[3, 4, 5];

#[tokio::test]
#[ignore]
async fn mint_test() -> Result<()> {
    let api = GearApi::dev().await?;

    let mut listener = api.subscribe().await?; // Subscribing for events.

    // Checking that blocks still running.
    assert!(listener.blocks_running().await?);

    // Init
    let init_ft = Initialize::Config(InitConfig {
        name: String::from("MyToken"),
        symbol: String::from("MTK"),
        decimals: 18,
    })
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
async fn set_state_on_init() -> Result<()> {
    let api = GearApi::dev().await?;

    let mut listener = api.subscribe().await?; // Subscribing for events.

    // Checking that blocks still running.
    assert!(listener.blocks_running().await?);

    assert!(init_state_with_len(&api, &mut listener, 1, 1).await?);
    assert!(init_state_with_len(&api, &mut listener, 10, 1).await?);
    assert!(init_state_with_len(&api, &mut listener, 100, 1).await?);

    for n in 1..=18u64 {
        assert!(init_state_with_len(&api, &mut listener, n * 1_000, 1).await?);
    }
    // No passed because 'Transaction would exhaust the block limits'
    assert!(init_state_with_len(&api, &mut listener, 100_000, 1)
        .await
        .is_err());

    assert!(init_state_with_len(&api, &mut listener, 1, 1).await?);
    assert!(init_state_with_len(&api, &mut listener, 10, 10).await?);
    assert!(init_state_with_len(&api, &mut listener, 100, 100).await?);

    // No passed because 'Not enught gas to continue execution'
    assert!(!init_state_with_len(&api, &mut listener, 1000, 1000).await?);

    Ok(())
}

async fn init_state_with_len(
    api: &GearApi,
    listener: &mut EventListener,
    balances_len: u64,
    allowances_len: u64,
) -> Result<bool> {
    let new_ft = create_fungible_token(balances_len, allowances_len);

    // Init
    let init_ft = Initialize::State(new_ft).encode();

    let gas_info = api
        .calculate_upload_gas(None, WASM_BINARY_OPT.to_vec(), init_ft.clone(), 0, true)
        .await?;

    println!(
        "Init with gas for balances len {:0>6}, allowances len {:0>6}: {:?}, total len bytes: {}",
        balances_len,
        allowances_len,
        gas_info,
        init_ft.len()
    );

    let (message_id, _program_id, _hash) = api
        .upload_program_bytes(
            WASM_BINARY_OPT.to_vec(),
            gclient::now_micros().to_le_bytes(),
            init_ft,
            gas_info.min_limit,
            0,
        )
        .await?;

    Ok(listener.message_processed(message_id).await?.succeed())
}

#[tokio::test]
#[ignore]
async fn migrate_by_handle() -> Result<()> {
    let api = GearApi::dev().await?;

    let mut listener = api.subscribe().await?; // Subscribing for events.

    // Checking that blocks still running.
    assert!(listener.blocks_running().await?);

    // Init first token
    let ft_io = create_fungible_token(377, 1);

    // Init second token
    let init_ft = Initialize::State(ft_io).encode();

    let gas_info = api
        .calculate_upload_gas(None, WASM_BINARY_OPT.to_vec(), init_ft.clone(), 0, true)
        .await?;

    let (message_id, program_id_1, _hash) = api
        .upload_program_bytes(
            WASM_BINARY_OPT.to_vec(),
            gclient::now_micros().to_le_bytes(),
            init_ft,
            250_000_000_000,
            0,
        )
        .await?;

    assert!(listener.message_processed(message_id).await?.succeed());

    // Init second token
    let init_ft = Initialize::Config(InitConfig {
        name: String::from("token 2"),
        symbol: String::from("t2"),
        decimals: 18,
    })
    .encode();

    let gas_info = api
        .calculate_upload_gas(None, WASM_BINARY_OPT.to_vec(), init_ft.clone(), 0, true)
        .await?;

    let (message_id, program_id_2, _hash) = api
        .upload_program_bytes(
            WASM_BINARY_OPT.to_vec(),
            gclient::now_micros().to_le_bytes(),
            init_ft,
            250_000_000_000,
            0,
        )
        .await?;

    assert!(listener.message_processed(message_id).await?.succeed());

    for _ in 0..67 {
        // Migrate from program_1 to program_2
        let migrate_state = FTAction::MigrateState(
            ActorId::from_slice(program_id_2.encode().as_slice()).expect("Can't create ActorId"),
        );

        let gas_info = api
            .calculate_handle_gas(None, program_id_1, migrate_state.encode(), 0, true)
            .await
            .unwrap();

        let (message_id, _) = api
            .send_message(program_id_1, migrate_state, gas_info.min_limit, 0)
            .await?;

        assert!(listener.message_processed(message_id).await?.succeed());
    }

    Ok(())
}

// #[tokio::test]
// #[ignore]
// async fn upgrade_by_handle() -> Result<()> {
//     let api = GearApi::dev().await?;

//     let mut listener = api.subscribe().await?; // Subscribing for events.

//     // Checking that blocks still running.
//     assert!(listener.blocks_running().await?);

//     // Init
//     let init_ft = Initialize::Config(InitConfig {
//         name: String::from("MyToken"),
//         symbol: String::from("MTK"),
//         decimals: 18,
//     })
//     .encode();

//     let gas_info = api
//         .calculate_upload_gas(None, WASM_BINARY_OPT.to_vec(), init_ft.clone(), 0, true)
//         .await?;

//     let (message_id, program_id, _hash) = api
//         .upload_program_bytes(
//             WASM_BINARY_OPT.to_vec(),
//             gclient::now_micros().to_le_bytes(),
//             init_ft,
//             gas_info.min_limit,
//             0,
//         )
//         .await?;

//     assert!(listener.message_processed(message_id).await?.succeed());

//     print_gas_info(&api, program_id.into_bytes(), 1, 0).await;
//     print_gas_info(&api, program_id.into_bytes(), 10, 0).await;
//     print_gas_info(&api, program_id.into_bytes(), 100, 0).await;
//     print_gas_info(&api, program_id.into_bytes(), 1000, 0).await;
//     print_gas_info(&api, program_id.into_bytes(), 10_000, 0).await;
//     // No passed because Not enough gas to continue execution
//     // print_gas_info(&api, program_id.into_bytes(), 100_000, 0).await;

//     print_gas_info(&api, program_id.into_bytes(), 1, 1).await;
//     print_gas_info(&api, program_id.into_bytes(), 10, 1).await;
//     print_gas_info(&api, program_id.into_bytes(), 100, 1).await;
//     print_gas_info(&api, program_id.into_bytes(), 1000, 1).await;
//     print_gas_info(&api, program_id.into_bytes(), 10_000, 1).await;
//     // No passed because Not enough gas to continue execution
//     // print_gas_info(&api, program_id.into_bytes(), 100_000, 1).await;

//     upgrade_with_balances(&api, &mut listener, program_id.into_bytes(), 1).await?;
//     upgrade_with_balances(&api, &mut listener, program_id.into_bytes(), 10).await?;
//     upgrade_with_balances(&api, &mut listener, program_id.into_bytes(), 100).await?;
//     upgrade_with_balances(&api, &mut listener, program_id.into_bytes(), 1000).await?;
//     upgrade_with_balances(&api, &mut listener, program_id.into_bytes(), 10_000).await?;

//     // No passed because Not enough gas to continue execution
//     let is_err = upgrade_with_balances(&api, &mut listener, program_id.into_bytes(), 100_000)
//         .await
//         .is_err();
//     assert!(is_err);

//     upgrade_with_len(&api, &mut listener, program_id.into_bytes(), 1, 1).await?;
//     upgrade_with_len(&api, &mut listener, program_id.into_bytes(), 10, 1).await?;
//     upgrade_with_len(&api, &mut listener, program_id.into_bytes(), 100, 1).await?;
//     upgrade_with_len(&api, &mut listener, program_id.into_bytes(), 1000, 1).await?;
//     upgrade_with_len(&api, &mut listener, program_id.into_bytes(), 10_000, 1).await?;

//     // No passed because Not enough gas to continue execution
//     let is_err = upgrade_with_len(&api, &mut listener, program_id.into_bytes(), 100_000, 1)
//         .await
//         .is_err();
//     assert!(is_err);

//     upgrade_with_len(&api, &mut listener, program_id.into_bytes(), 1, 1).await?;
//     upgrade_with_len(&api, &mut listener, program_id.into_bytes(), 10, 10).await?;
//     upgrade_with_len(&api, &mut listener, program_id.into_bytes(), 100, 100).await?;

//     // No passed because Not enough gas to continue execution
//     let is_err = upgrade_with_len(&api, &mut listener, program_id.into_bytes(), 1000, 1000)
//         .await
//         .is_err();
//     assert!(is_err);

//     upgrade_max_gas(&api, &mut listener, program_id.into_bytes(), 1, 1).await?;
//     upgrade_max_gas(&api, &mut listener, program_id.into_bytes(), 10, 1).await?;
//     upgrade_max_gas(&api, &mut listener, program_id.into_bytes(), 100, 1).await?;
//     upgrade_max_gas(&api, &mut listener, program_id.into_bytes(), 1000, 1).await?;
//     upgrade_max_gas(&api, &mut listener, program_id.into_bytes(), 10_000, 1).await?;

//     // No passed because "Transaction would exhaust the block limits"
//     let is_err = upgrade_max_gas(&api, &mut listener, program_id.into_bytes(), 100_000, 1)
//         .await
//         .is_err();
//     assert!(is_err);

//     upgrade_max_gas(&api, &mut listener, program_id.into_bytes(), 1, 1).await?;
//     upgrade_max_gas(&api, &mut listener, program_id.into_bytes(), 10, 10).await?;
//     upgrade_max_gas(&api, &mut listener, program_id.into_bytes(), 100, 100).await?;

//     // No passed because Not enough gas to continue execution
//     assert!(!upgrade_max_gas(&api, &mut listener, program_id.into_bytes(), 1000, 1000).await?);

//     Ok(())
// }

fn create_fungible_token(balances_len: u64, allowances_len: u64) -> IoFungibleToken {
    let balances = (0..balances_len).map(|v| (v.into(), v as u128)).collect();
    let allowances = (0..allowances_len)
        .map(|v| (v.into(), vec![(0.into(), v as u128); 1]))
        .collect();

    IoFungibleToken {
        name: "new-token".to_string(),
        symbol: "NT".to_string(),
        total_supply: 1_000_000,
        balances,
        allowances,
        decimals: 18,
        migration_state: None,
    }
}

// async fn print_gas_info(
//     api: &GearApi,
//     program_id: [u8; 32],
//     balances_len: u64,
//     allowances_len: u64,
// ) {
//     let new_ft = create_fungible_token(balances_len, allowances_len);
//     let upgrade = FTAction::UpgradeState(new_ft).encode();

//     let gas_info = api
//         .calculate_handle_gas(None, program_id.into(), upgrade.clone(), 0, true)
//         .await
//         .unwrap();

//     println!(
//         "Gas for balances len {:0>6}, allowances len {:0>6}: {:?}, total bytes: {}",
//         balances_len,
//         allowances_len,
//         gas_info,
//         upgrade.len()
//     );
// }

// async fn upgrade_max_gas(
//     api: &GearApi,
//     listener: &mut EventListener,
//     program_id: [u8; 32],
//     balances_len: u64,
//     allowances_len: u64,
// ) -> Result<bool> {
//     let balances = (0..balances_len).map(|v| (v.into(), v as u128)).collect();
//     let allowances = (0..allowances_len)
//         .map(|v| (v.into(), vec![(0.into(), v as u128); 1]))
//         .collect();

//     let new_ft = IoFungibleToken {
//         name: "new-token".to_string(),
//         symbol: "NT".to_string(),
//         total_supply: 1_000_000,
//         balances,
//         allowances,
//         decimals: 18,
//     };
//     let upgrade = FTAction::UpgradeState(new_ft);

//     println!(
//         "For balances len {:0>6}, allowances len {:0>6} with Max Gas 250_000_000_000, total bytes: {}",
//         balances_len, allowances_len, upgrade.encode().len()
//     );

//     let (message_id, _) = api
//         .send_message(program_id.into(), upgrade, 250_000_000_000, 0)
//         .await?;

//     Ok(listener.message_processed(message_id).await?.succeed())
// }

// async fn upgrade_with_len(
//     api: &GearApi,
//     listener: &mut EventListener,
//     program_id: [u8; 32],
//     balances_len: u64,
//     allowances_len: u64,
// ) -> Result<()> {
//     let balances = (0..balances_len).map(|v| (v.into(), v as u128)).collect();
//     let allowances = (0..allowances_len)
//         .map(|v| (v.into(), vec![(0.into(), v as u128); 1]))
//         .collect();

//     let new_ft = IoFungibleToken {
//         name: "new-token".to_string(),
//         symbol: "NT".to_string(),
//         total_supply: 1_000_000,
//         balances,
//         allowances,
//         decimals: 18,
//     };

//     let upgrade = FTAction::UpgradeState(new_ft);

//     let gas_info = api
//         .calculate_handle_gas(None, program_id.into(), upgrade.encode(), 0, true)
//         .await?;

//     println!(
//         "For balances len {:0>6}, allowances len {:0>6}: {:?}",
//         balances_len, allowances_len, gas_info
//     );

//     let (message_id, _) = api
//         .send_message(program_id.into(), upgrade, gas_info.min_limit, 0)
//         .await?;

//     assert!(listener.message_processed(message_id).await?.succeed());

//     assert!(listener.blocks_running().await?);

//     Ok(())
// }

// async fn upgrade_with_balances(
//     api: &GearApi,
//     listener: &mut EventListener,
//     program_id: [u8; 32],
//     balances_len: u64,
// ) -> Result<()> {
//     upgrade_with_len(api, listener, program_id, balances_len, 0).await
// }
