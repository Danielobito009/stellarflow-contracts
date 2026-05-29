#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, Events},
    token::{StellarAssetClient, TokenClient},
    Address, Env,
};

use crate::{Error, RelayerStakingEscrowClient};

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

/// Deploy a fresh SAC token and return (token_address, admin_address).
/// In soroban-sdk 20, register_stellar_asset_contract returns Address directly.
fn create_token(env: &Env) -> (Address, Address) {
    let admin = Address::generate(env);
    let token_address = env.register_stellar_asset_contract(admin.clone());
    (token_address, admin)
}

/// Deploy the escrow contract and return its client.
fn create_escrow(env: &Env) -> RelayerStakingEscrowClient {
    let contract_id = env.register_contract(None, crate::RelayerStakingEscrow);
    RelayerStakingEscrowClient::new(env, &contract_id)
}

/// Mint `amount` tokens to `recipient`. Must be called within mock_all_auths scope.
fn mint(env: &Env, token: &Address, recipient: &Address, amount: i128) {
    StellarAssetClient::new(env, token).mint(recipient, &amount);
}

// ---------------------------------------------------------------------------
// initialize
// ---------------------------------------------------------------------------

#[test]
fn test_initialize_stores_token() {
    let env = Env::default();
    env.mock_all_auths();
    let (token, _admin) = create_token(&env);
    let escrow = create_escrow(&env);

    escrow.initialize(&token);

    assert_eq!(escrow.get_token(), token);
}

#[test]
fn test_initialize_twice_returns_error() {
    let env = Env::default();
    env.mock_all_auths();
    let (token, _admin) = create_token(&env);
    let escrow = create_escrow(&env);

    escrow.initialize(&token);
    let result = escrow.try_initialize(&token);

    assert_eq!(result, Err(Ok(Error::AlreadyInitialized)));
}

// ---------------------------------------------------------------------------
// deposit_stake
// ---------------------------------------------------------------------------

#[test]
fn test_deposit_stake_increases_balance() {
    let env = Env::default();
    env.mock_all_auths();

    let (token, _admin) = create_token(&env);
    let escrow = create_escrow(&env);
    escrow.initialize(&token);

    let relayer = Address::generate(&env);
    mint(&env, &token, &relayer, 1_000);

    escrow.deposit_stake(&relayer, &500);

    assert_eq!(escrow.get_stake(&relayer), 500);
}

#[test]
fn test_deposit_stake_multiple_deposits_accumulate() {
    let env = Env::default();
    env.mock_all_auths();

    let (token, _admin) = create_token(&env);
    let escrow = create_escrow(&env);
    escrow.initialize(&token);

    let relayer = Address::generate(&env);
    mint(&env, &token, &relayer, 2_000);

    escrow.deposit_stake(&relayer, &700);
    escrow.deposit_stake(&relayer, &300);

    assert_eq!(escrow.get_stake(&relayer), 1_000);
}

#[test]
fn test_deposit_stake_zero_amount_returns_error() {
    let env = Env::default();
    env.mock_all_auths();

    let (token, _admin) = create_token(&env);
    let escrow = create_escrow(&env);
    escrow.initialize(&token);

    let relayer = Address::generate(&env);
    let result = escrow.try_deposit_stake(&relayer, &0);

    assert_eq!(result, Err(Ok(Error::InvalidAmount)));
}

#[test]
fn test_deposit_stake_negative_amount_returns_error() {
    let env = Env::default();
    env.mock_all_auths();

    let (token, _admin) = create_token(&env);
    let escrow = create_escrow(&env);
    escrow.initialize(&token);

    let relayer = Address::generate(&env);
    let result = escrow.try_deposit_stake(&relayer, &-100);

    assert_eq!(result, Err(Ok(Error::InvalidAmount)));
}

#[test]
fn test_deposit_stake_requires_relayer_auth() {
    let env = Env::default();

    // Initialize with mocked auths, then clear them.
    let (token, _admin) = {
        env.mock_all_auths();
        create_token(&env)
    };
    let escrow = create_escrow(&env);
    escrow.initialize(&token);

    // Now clear all mocked auths — deposit should fail without relayer signature.
    env.set_auths(&[]);
    let relayer = Address::generate(&env);
    let result = escrow.try_deposit_stake(&relayer, &500);
    assert!(result.is_err());
}

#[test]
fn test_deposit_stake_emits_event() {
    let env = Env::default();
    env.mock_all_auths();

    let (token, _admin) = create_token(&env);
    let escrow = create_escrow(&env);
    escrow.initialize(&token);

    let relayer = Address::generate(&env);
    mint(&env, &token, &relayer, 1_000);

    escrow.deposit_stake(&relayer, &400);

    let events = env.events().all();
    // The last event should be our deposit event.
    let last = events.last().unwrap();
    // topic[0] should be the symbol "deposit"
    let topic: soroban_sdk::Symbol = last.1.get(0).unwrap();
    assert_eq!(topic, soroban_sdk::symbol_short!("deposit"));
}

#[test]
fn test_deposit_stake_before_initialize_returns_error() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow = create_escrow(&env);
    let relayer = Address::generate(&env);

    let result = escrow.try_deposit_stake(&relayer, &100);
    assert_eq!(result, Err(Ok(Error::NotInitialized)));
}

// ---------------------------------------------------------------------------
// get_stake
// ---------------------------------------------------------------------------

#[test]
fn test_get_stake_returns_zero_for_unknown_relayer() {
    let env = Env::default();
    env.mock_all_auths();
    let (token, _admin) = create_token(&env);
    let escrow = create_escrow(&env);
    escrow.initialize(&token);

    let relayer = Address::generate(&env);
    assert_eq!(escrow.get_stake(&relayer), 0);
}

#[test]
fn test_get_stake_independent_per_relayer() {
    let env = Env::default();
    env.mock_all_auths();

    let (token, _admin) = create_token(&env);
    let escrow = create_escrow(&env);
    escrow.initialize(&token);

    let relayer_a = Address::generate(&env);
    let relayer_b = Address::generate(&env);
    mint(&env, &token, &relayer_a, 1_000);
    mint(&env, &token, &relayer_b, 1_000);

    escrow.deposit_stake(&relayer_a, &300);
    escrow.deposit_stake(&relayer_b, &700);

    assert_eq!(escrow.get_stake(&relayer_a), 300);
    assert_eq!(escrow.get_stake(&relayer_b), 700);
}

// ---------------------------------------------------------------------------
// withdraw_stake
// ---------------------------------------------------------------------------

#[test]
fn test_withdraw_stake_reduces_balance() {
    let env = Env::default();
    env.mock_all_auths();

    let (token, _admin) = create_token(&env);
    let escrow = create_escrow(&env);
    escrow.initialize(&token);

    let relayer = Address::generate(&env);
    mint(&env, &token, &relayer, 1_000);

    escrow.deposit_stake(&relayer, &800);
    escrow.withdraw_stake(&relayer, &300);

    assert_eq!(escrow.get_stake(&relayer), 500);
}

#[test]
fn test_withdraw_stake_full_amount() {
    let env = Env::default();
    env.mock_all_auths();

    let (token, _admin) = create_token(&env);
    let escrow = create_escrow(&env);
    escrow.initialize(&token);

    let relayer = Address::generate(&env);
    mint(&env, &token, &relayer, 1_000);

    escrow.deposit_stake(&relayer, &1_000);
    escrow.withdraw_stake(&relayer, &1_000);

    assert_eq!(escrow.get_stake(&relayer), 0);
}

#[test]
fn test_withdraw_stake_exceeds_balance_returns_error() {
    let env = Env::default();
    env.mock_all_auths();

    let (token, _admin) = create_token(&env);
    let escrow = create_escrow(&env);
    escrow.initialize(&token);

    let relayer = Address::generate(&env);
    mint(&env, &token, &relayer, 1_000);

    escrow.deposit_stake(&relayer, &500);
    let result = escrow.try_withdraw_stake(&relayer, &600);

    assert_eq!(result, Err(Ok(Error::InsufficientBalance)));
}

#[test]
fn test_withdraw_stake_zero_amount_returns_error() {
    let env = Env::default();
    env.mock_all_auths();

    let (token, _admin) = create_token(&env);
    let escrow = create_escrow(&env);
    escrow.initialize(&token);

    let relayer = Address::generate(&env);
    mint(&env, &token, &relayer, 1_000);
    escrow.deposit_stake(&relayer, &500);

    let result = escrow.try_withdraw_stake(&relayer, &0);
    assert_eq!(result, Err(Ok(Error::InvalidAmount)));
}

#[test]
fn test_withdraw_stake_requires_relayer_auth() {
    let env = Env::default();
    env.mock_all_auths();

    let (token, _admin) = create_token(&env);
    let escrow = create_escrow(&env);
    escrow.initialize(&token);

    let relayer = Address::generate(&env);
    mint(&env, &token, &relayer, 1_000);
    escrow.deposit_stake(&relayer, &500);

    // Remove all mocked auths — withdrawal should fail without relayer signature.
    env.set_auths(&[]);
    let result = escrow.try_withdraw_stake(&relayer, &200);
    assert!(result.is_err());
}

#[test]
fn test_withdraw_stake_emits_event() {
    let env = Env::default();
    env.mock_all_auths();

    let (token, _admin) = create_token(&env);
    let escrow = create_escrow(&env);
    escrow.initialize(&token);

    let relayer = Address::generate(&env);
    mint(&env, &token, &relayer, 1_000);

    escrow.deposit_stake(&relayer, &600);
    escrow.withdraw_stake(&relayer, &200);

    let events = env.events().all();
    let last = events.last().unwrap();
    let topic: soroban_sdk::Symbol = last.1.get(0).unwrap();
    assert_eq!(topic, soroban_sdk::symbol_short!("withdraw"));
}

#[test]
fn test_withdraw_stake_before_initialize_returns_error() {
    let env = Env::default();
    env.mock_all_auths();

    let escrow = create_escrow(&env);
    let relayer = Address::generate(&env);

    let result = escrow.try_withdraw_stake(&relayer, &100);
    assert_eq!(result, Err(Ok(Error::NotInitialized)));
}

// ---------------------------------------------------------------------------
// Token balance round-trip
// ---------------------------------------------------------------------------

#[test]
fn test_token_balances_move_correctly_on_deposit_and_withdraw() {
    let env = Env::default();
    env.mock_all_auths();

    let (token, _admin) = create_token(&env);
    let escrow = create_escrow(&env);
    escrow.initialize(&token);

    let relayer = Address::generate(&env);
    mint(&env, &token, &relayer, 1_000);

    let token_client = TokenClient::new(&env, &token);

    // Before deposit: relayer holds all tokens.
    assert_eq!(token_client.balance(&relayer), 1_000);
    assert_eq!(token_client.balance(&escrow.address), 0);

    escrow.deposit_stake(&relayer, &600);

    // After deposit: tokens moved to escrow.
    assert_eq!(token_client.balance(&relayer), 400);
    assert_eq!(token_client.balance(&escrow.address), 600);

    escrow.withdraw_stake(&relayer, &250);

    // After partial withdrawal: tokens returned to relayer.
    assert_eq!(token_client.balance(&relayer), 650);
    assert_eq!(token_client.balance(&escrow.address), 350);
}
