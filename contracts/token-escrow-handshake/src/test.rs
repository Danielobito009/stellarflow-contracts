#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, Events, Ledger},
    token::{StellarAssetClient, TokenClient},
    Address, Env,
};

use crate::{EscrowError, TokenEscrowHandshakeClient, UnlockRequest};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const COOLDOWN: u64 = 172_800; // 2 days in seconds

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

/// Register a SAC token and return (token_address, admin_address).
fn create_token(env: &Env) -> (Address, Address) {
    let admin = Address::generate(env);
    let token = env.register_stellar_asset_contract(admin.clone());
    (token, admin)
}

/// Register the escrow contract and return its client.
fn create_escrow(env: &Env) -> TokenEscrowHandshakeClient {
    let id = env.register_contract(None, crate::TokenEscrowHandshake);
    TokenEscrowHandshakeClient::new(env, &id)
}

/// Mint tokens to an address. Must be called inside mock_all_auths scope.
fn mint(env: &Env, token: &Address, recipient: &Address, amount: i128) {
    StellarAssetClient::new(env, token).mint(recipient, &amount);
}

/// Advance the ledger timestamp by `delta` seconds.
fn advance_time(env: &Env, delta: u64) {
    env.ledger().with_mut(|li| {
        li.timestamp += delta;
    });
}

// ---------------------------------------------------------------------------
// initialize
// ---------------------------------------------------------------------------

#[test]
fn test_initialize_stores_token() {
    let env = Env::default();
    env.mock_all_auths();
    let (token, _) = create_token(&env);
    let escrow = create_escrow(&env);

    escrow.initialize(&token);

    assert_eq!(escrow.get_token(), token);
}

#[test]
fn test_initialize_twice_errors() {
    let env = Env::default();
    env.mock_all_auths();
    let (token, _) = create_token(&env);
    let escrow = create_escrow(&env);

    escrow.initialize(&token);
    assert_eq!(
        escrow.try_initialize(&token),
        Err(Ok(EscrowError::AlreadyInitialized))
    );
}

// ---------------------------------------------------------------------------
// deposit_stake
// ---------------------------------------------------------------------------

#[test]
fn test_deposit_stake_increases_balance() {
    let env = Env::default();
    env.mock_all_auths();

    let (token, _) = create_token(&env);
    let escrow = create_escrow(&env);
    escrow.initialize(&token);

    let relayer = Address::generate(&env);
    mint(&env, &token, &relayer, 1_000);

    escrow.deposit_stake(&relayer, &600);

    assert_eq!(escrow.get_stake(&relayer), 600);
}

#[test]
fn test_deposit_stake_accumulates() {
    let env = Env::default();
    env.mock_all_auths();

    let (token, _) = create_token(&env);
    let escrow = create_escrow(&env);
    escrow.initialize(&token);

    let relayer = Address::generate(&env);
    mint(&env, &token, &relayer, 2_000);

    escrow.deposit_stake(&relayer, &500);
    escrow.deposit_stake(&relayer, &300);

    assert_eq!(escrow.get_stake(&relayer), 800);
}

#[test]
fn test_deposit_zero_errors() {
    let env = Env::default();
    env.mock_all_auths();

    let (token, _) = create_token(&env);
    let escrow = create_escrow(&env);
    escrow.initialize(&token);

    let relayer = Address::generate(&env);
    assert_eq!(
        escrow.try_deposit_stake(&relayer, &0),
        Err(Ok(EscrowError::InvalidAmount))
    );
}

#[test]
fn test_deposit_requires_auth() {
    let env = Env::default();

    let (token, _) = {
        env.mock_all_auths();
        create_token(&env)
    };
    let escrow = create_escrow(&env);
    escrow.initialize(&token);

    env.set_auths(&[]);
    let relayer = Address::generate(&env);
    assert!(escrow.try_deposit_stake(&relayer, &100).is_err());
}

#[test]
fn test_deposit_moves_tokens_to_escrow() {
    let env = Env::default();
    env.mock_all_auths();

    let (token, _) = create_token(&env);
    let escrow = create_escrow(&env);
    escrow.initialize(&token);

    let relayer = Address::generate(&env);
    mint(&env, &token, &relayer, 1_000);

    let tc = TokenClient::new(&env, &token);
    assert_eq!(tc.balance(&relayer), 1_000);
    assert_eq!(tc.balance(&escrow.address), 0);

    escrow.deposit_stake(&relayer, &700);

    assert_eq!(tc.balance(&relayer), 300);
    assert_eq!(tc.balance(&escrow.address), 700);
}

#[test]
fn test_deposit_emits_event() {
    let env = Env::default();
    env.mock_all_auths();

    let (token, _) = create_token(&env);
    let escrow = create_escrow(&env);
    escrow.initialize(&token);

    let relayer = Address::generate(&env);
    mint(&env, &token, &relayer, 500);
    escrow.deposit_stake(&relayer, &500);

    let events = env.events().all();
    let last = events.last().unwrap();
    let topic: soroban_sdk::Symbol = last.1.get(0).unwrap();
    assert_eq!(topic, soroban_sdk::symbol_short!("deposit"));
}

// ---------------------------------------------------------------------------
// initiate_unlock
// ---------------------------------------------------------------------------

#[test]
fn test_initiate_unlock_deducts_stake_and_stores_request() {
    let env = Env::default();
    env.mock_all_auths();

    let (token, _) = create_token(&env);
    let escrow = create_escrow(&env);
    escrow.initialize(&token);

    let relayer = Address::generate(&env);
    mint(&env, &token, &relayer, 1_000);
    escrow.deposit_stake(&relayer, &1_000);

    let before_ts = env.ledger().timestamp();
    escrow.initiate_unlock(&relayer, &400);

    // Stake should be reduced.
    assert_eq!(escrow.get_stake(&relayer), 600);

    // Unlock request should be stored with correct values.
    let req: UnlockRequest = escrow.get_unlock_request(&relayer).unwrap();
    assert_eq!(req.amount, 400);
    assert_eq!(req.unlock_time, before_ts + COOLDOWN);
}

#[test]
fn test_initiate_unlock_insufficient_stake_errors() {
    let env = Env::default();
    env.mock_all_auths();

    let (token, _) = create_token(&env);
    let escrow = create_escrow(&env);
    escrow.initialize(&token);

    let relayer = Address::generate(&env);
    mint(&env, &token, &relayer, 500);
    escrow.deposit_stake(&relayer, &500);

    assert_eq!(
        escrow.try_initiate_unlock(&relayer, &600),
        Err(Ok(EscrowError::InsufficientStake))
    );
}

#[test]
fn test_initiate_unlock_double_request_errors() {
    let env = Env::default();
    env.mock_all_auths();

    let (token, _) = create_token(&env);
    let escrow = create_escrow(&env);
    escrow.initialize(&token);

    let relayer = Address::generate(&env);
    mint(&env, &token, &relayer, 1_000);
    escrow.deposit_stake(&relayer, &1_000);

    escrow.initiate_unlock(&relayer, &300);

    // Second initiate_unlock should fail.
    assert_eq!(
        escrow.try_initiate_unlock(&relayer, &200),
        Err(Ok(EscrowError::PendingUnlockExists))
    );
}

#[test]
fn test_initiate_unlock_requires_auth() {
    let env = Env::default();
    env.mock_all_auths();

    let (token, _) = create_token(&env);
    let escrow = create_escrow(&env);
    escrow.initialize(&token);

    let relayer = Address::generate(&env);
    mint(&env, &token, &relayer, 500);
    escrow.deposit_stake(&relayer, &500);

    env.set_auths(&[]);
    assert!(escrow.try_initiate_unlock(&relayer, &200).is_err());
}

#[test]
fn test_initiate_unlock_emits_event() {
    let env = Env::default();
    env.mock_all_auths();

    let (token, _) = create_token(&env);
    let escrow = create_escrow(&env);
    escrow.initialize(&token);

    let relayer = Address::generate(&env);
    mint(&env, &token, &relayer, 500);
    escrow.deposit_stake(&relayer, &500);
    escrow.initiate_unlock(&relayer, &500);

    let events = env.events().all();
    let last = events.last().unwrap();
    let topic: soroban_sdk::Symbol = last.1.get(0).unwrap();
    assert_eq!(topic, soroban_sdk::symbol_short!("unlock"));
}

// ---------------------------------------------------------------------------
// claim_unlock
// ---------------------------------------------------------------------------

#[test]
fn test_claim_unlock_before_cooldown_errors() {
    let env = Env::default();
    env.mock_all_auths();

    let (token, _) = create_token(&env);
    let escrow = create_escrow(&env);
    escrow.initialize(&token);

    let relayer = Address::generate(&env);
    mint(&env, &token, &relayer, 1_000);
    escrow.deposit_stake(&relayer, &1_000);
    escrow.initiate_unlock(&relayer, &1_000);

    // Advance time by less than the cooldown.
    advance_time(&env, COOLDOWN - 1);

    assert_eq!(
        escrow.try_claim_unlock(&relayer),
        Err(Ok(EscrowError::CooldownNotElapsed))
    );
}

#[test]
fn test_claim_unlock_after_cooldown_succeeds() {
    let env = Env::default();
    env.mock_all_auths();

    let (token, _) = create_token(&env);
    let escrow = create_escrow(&env);
    escrow.initialize(&token);

    let relayer = Address::generate(&env);
    mint(&env, &token, &relayer, 1_000);
    escrow.deposit_stake(&relayer, &1_000);
    escrow.initiate_unlock(&relayer, &1_000);

    // Advance time past the cooldown.
    advance_time(&env, COOLDOWN);

    let tc = TokenClient::new(&env, &token);
    assert_eq!(tc.balance(&relayer), 0);

    escrow.claim_unlock(&relayer);

    // Tokens returned to relayer.
    assert_eq!(tc.balance(&relayer), 1_000);
    assert_eq!(tc.balance(&escrow.address), 0);

    // Unlock request removed.
    assert!(escrow.get_unlock_request(&relayer).is_none());
}

#[test]
fn test_claim_unlock_exactly_at_cooldown_boundary_succeeds() {
    let env = Env::default();
    env.mock_all_auths();

    let (token, _) = create_token(&env);
    let escrow = create_escrow(&env);
    escrow.initialize(&token);

    let relayer = Address::generate(&env);
    mint(&env, &token, &relayer, 500);
    escrow.deposit_stake(&relayer, &500);
    escrow.initiate_unlock(&relayer, &500);

    // Advance exactly to the unlock_time.
    advance_time(&env, COOLDOWN);

    escrow.claim_unlock(&relayer);
    assert_eq!(TokenClient::new(&env, &token).balance(&relayer), 500);
}

#[test]
fn test_claim_unlock_no_request_errors() {
    let env = Env::default();
    env.mock_all_auths();

    let (token, _) = create_token(&env);
    let escrow = create_escrow(&env);
    escrow.initialize(&token);

    let relayer = Address::generate(&env);

    assert_eq!(
        escrow.try_claim_unlock(&relayer),
        Err(Ok(EscrowError::NoUnlockRequest))
    );
}

#[test]
fn test_claim_unlock_requires_auth() {
    let env = Env::default();
    env.mock_all_auths();

    let (token, _) = create_token(&env);
    let escrow = create_escrow(&env);
    escrow.initialize(&token);

    let relayer = Address::generate(&env);
    mint(&env, &token, &relayer, 500);
    escrow.deposit_stake(&relayer, &500);
    escrow.initiate_unlock(&relayer, &500);
    advance_time(&env, COOLDOWN);

    env.set_auths(&[]);
    assert!(escrow.try_claim_unlock(&relayer).is_err());
}

#[test]
fn test_claim_unlock_emits_event() {
    let env = Env::default();
    env.mock_all_auths();

    let (token, _) = create_token(&env);
    let escrow = create_escrow(&env);
    escrow.initialize(&token);

    let relayer = Address::generate(&env);
    mint(&env, &token, &relayer, 800);
    escrow.deposit_stake(&relayer, &800);
    escrow.initiate_unlock(&relayer, &800);
    advance_time(&env, COOLDOWN);
    escrow.claim_unlock(&relayer);

    let events = env.events().all();
    let last = events.last().unwrap();
    let topic: soroban_sdk::Symbol = last.1.get(0).unwrap();
    assert_eq!(topic, soroban_sdk::symbol_short!("claim"));
}

// ---------------------------------------------------------------------------
// Full flow
// ---------------------------------------------------------------------------

#[test]
fn test_full_deposit_unlock_claim_flow() {
    let env = Env::default();
    env.mock_all_auths();

    let (token, _) = create_token(&env);
    let escrow = create_escrow(&env);
    escrow.initialize(&token);

    let relayer = Address::generate(&env);
    mint(&env, &token, &relayer, 2_000);

    let tc = TokenClient::new(&env, &token);

    // Deposit 2000.
    escrow.deposit_stake(&relayer, &2_000);
    assert_eq!(escrow.get_stake(&relayer), 2_000);
    assert_eq!(tc.balance(&escrow.address), 2_000);

    // Initiate unlock for 1500.
    escrow.initiate_unlock(&relayer, &1_500);
    assert_eq!(escrow.get_stake(&relayer), 500);
    assert_eq!(escrow.get_unlock_request(&relayer).unwrap().amount, 1_500);

    // Cooldown not elapsed — claim fails.
    advance_time(&env, COOLDOWN - 10);
    assert_eq!(
        escrow.try_claim_unlock(&relayer),
        Err(Ok(EscrowError::CooldownNotElapsed))
    );

    // Advance past cooldown — claim succeeds.
    advance_time(&env, 10);
    escrow.claim_unlock(&relayer);

    assert_eq!(tc.balance(&relayer), 1_500);
    assert_eq!(tc.balance(&escrow.address), 500); // 500 still staked
    assert!(escrow.get_unlock_request(&relayer).is_none());
}

#[test]
fn test_two_relayers_are_independent() {
    let env = Env::default();
    env.mock_all_auths();

    let (token, _) = create_token(&env);
    let escrow = create_escrow(&env);
    escrow.initialize(&token);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    mint(&env, &token, &alice, 1_000);
    mint(&env, &token, &bob, 1_000);

    escrow.deposit_stake(&alice, &1_000);
    escrow.deposit_stake(&bob, &1_000);

    escrow.initiate_unlock(&alice, &1_000);

    // Bob's stake is unaffected.
    assert_eq!(escrow.get_stake(&bob), 1_000);
    assert!(escrow.get_unlock_request(&bob).is_none());

    advance_time(&env, COOLDOWN);
    escrow.claim_unlock(&alice);

    let tc = TokenClient::new(&env, &token);
    assert_eq!(tc.balance(&alice), 1_000);
    assert_eq!(escrow.get_stake(&bob), 1_000);
}
