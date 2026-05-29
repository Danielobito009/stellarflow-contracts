//! Token Escrow Handshake Verification
//!
//! Implements a two-step withdrawal flow to prevent tokens from being
//! released to an unconfirmed destination:
//!
//! 1. `initiate_unlock` — deducts stake and starts a 2-day cooldown timer.
//! 2. `claim_unlock`    — after cooldown, relayer explicitly claims tokens.
//!
//! This handshake ensures the relayer actively confirms the destination
//! before tokens leave the escrow, making accidental loss impossible.

#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, token, Address, Env,
};

// ---------------------------------------------------------------------------
// Storage keys
// ---------------------------------------------------------------------------

#[contracttype]
pub enum DataKey {
    /// SAC token address — instance storage (one per contract).
    Token,
    /// Staked balance per relayer — persistent storage.
    Stake(Address),
    /// Pending unlock request per relayer — persistent storage.
    UnlockRequest(Address),
}

// ---------------------------------------------------------------------------
// Structs
// ---------------------------------------------------------------------------

/// A pending unlock request created by `initiate_unlock`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnlockRequest {
    /// Amount of tokens queued for withdrawal.
    pub amount: i128,
    /// Ledger timestamp after which `claim_unlock` is valid.
    pub unlock_time: u64,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum EscrowError {
    /// Contract has not been initialized yet.
    NotInitialized = 1,
    /// Contract has already been initialized.
    AlreadyInitialized = 2,
    /// Relayer does not have enough staked balance.
    InsufficientStake = 3,
    /// A pending unlock request already exists for this relayer.
    PendingUnlockExists = 4,
    /// No unlock request found for this relayer.
    NoUnlockRequest = 5,
    /// The 2-day cooldown period has not elapsed yet.
    CooldownNotElapsed = 6,
    /// Amount must be greater than zero.
    InvalidAmount = 7,
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

/// Cooldown period in seconds (2 days).
const COOLDOWN_SECONDS: u64 = 172_800;

#[contract]
pub struct TokenEscrowHandshake;

#[contractimpl]
impl TokenEscrowHandshake {
    // -----------------------------------------------------------------------
    // Setup
    // -----------------------------------------------------------------------

    /// Store the SAC token address. Can only be called once.
    pub fn initialize(env: Env, token: Address) -> Result<(), EscrowError> {
        if env.storage().instance().has(&DataKey::Token) {
            return Err(EscrowError::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Token, &token);

        env.events().publish(
            (soroban_sdk::symbol_short!("init"),),
            (token,),
        );
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Step 0 — deposit
    // -----------------------------------------------------------------------

    /// Transfer `amount` tokens from `relayer` into the escrow.
    ///
    /// Requires authorization from `relayer`.
    pub fn deposit_stake(env: Env, relayer: Address, amount: i128) -> Result<(), EscrowError> {
        Self::assert_initialized(&env)?;

        if amount <= 0 {
            return Err(EscrowError::InvalidAmount);
        }

        relayer.require_auth();

        let token_address = Self::token(&env);
        token::TokenClient::new(&env, &token_address)
            .transfer(&relayer, &env.current_contract_address(), &amount);

        // Update balance after transfer (safe: transfer panics on failure).
        let key = DataKey::Stake(relayer.clone());
        let current: i128 = env.storage().persistent().get(&key).unwrap_or(0);
        env.storage().persistent().set(&key, &(current + amount));

        env.events().publish(
            (soroban_sdk::symbol_short!("deposit"),),
            (relayer, amount),
        );
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Step 1 — initiate unlock
    // -----------------------------------------------------------------------

    /// Begin the two-day cooldown for withdrawing `amount` tokens.
    ///
    /// - Deducts `amount` from the relayer's stake immediately.
    /// - Stores an `UnlockRequest` with `unlock_time = now + 172800s`.
    /// - Errors if a pending unlock already exists.
    pub fn initiate_unlock(env: Env, relayer: Address, amount: i128) -> Result<(), EscrowError> {
        Self::assert_initialized(&env)?;

        if amount <= 0 {
            return Err(EscrowError::InvalidAmount);
        }

        relayer.require_auth();

        // Reject if a pending unlock already exists.
        if env
            .storage()
            .persistent()
            .has(&DataKey::UnlockRequest(relayer.clone()))
        {
            return Err(EscrowError::PendingUnlockExists);
        }

        // Check sufficient stake.
        let stake_key = DataKey::Stake(relayer.clone());
        let current_stake: i128 = env.storage().persistent().get(&stake_key).unwrap_or(0);
        if amount > current_stake {
            return Err(EscrowError::InsufficientStake);
        }

        // Checks-effects: deduct stake BEFORE writing unlock request.
        env.storage()
            .persistent()
            .set(&stake_key, &(current_stake - amount));

        let unlock_time = env.ledger().timestamp() + COOLDOWN_SECONDS;
        env.storage().persistent().set(
            &DataKey::UnlockRequest(relayer.clone()),
            &UnlockRequest { amount, unlock_time },
        );

        env.events().publish(
            (soroban_sdk::symbol_short!("unlock"),),
            (relayer, amount, unlock_time),
        );
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Step 2 — claim unlock
    // -----------------------------------------------------------------------

    /// Claim tokens after the cooldown has elapsed.
    ///
    /// - Verifies the unlock request exists and cooldown has passed.
    /// - Removes the request from storage (checks-effects).
    /// - Transfers tokens from the contract to the relayer.
    pub fn claim_unlock(env: Env, relayer: Address) -> Result<(), EscrowError> {
        Self::assert_initialized(&env)?;

        relayer.require_auth();

        let request_key = DataKey::UnlockRequest(relayer.clone());

        // Load the pending request — error if none.
        let request: UnlockRequest = env
            .storage()
            .persistent()
            .get(&request_key)
            .ok_or(EscrowError::NoUnlockRequest)?;

        // Verify cooldown has elapsed.
        if env.ledger().timestamp() < request.unlock_time {
            return Err(EscrowError::CooldownNotElapsed);
        }

        let amount = request.amount;

        // Checks-effects: remove request BEFORE token transfer.
        env.storage().persistent().remove(&request_key);

        // Transfer tokens from escrow to relayer.
        let token_address = Self::token(&env);
        token::TokenClient::new(&env, &token_address)
            .transfer(&env.current_contract_address(), &relayer, &amount);

        env.events().publish(
            (soroban_sdk::symbol_short!("claim"),),
            (relayer, amount),
        );
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Read-only queries
    // -----------------------------------------------------------------------

    /// Returns the relayer's current staked balance (0 if never deposited).
    pub fn get_stake(env: Env, relayer: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::Stake(relayer))
            .unwrap_or(0)
    }

    /// Returns the pending unlock request for `relayer`, or `None`.
    pub fn get_unlock_request(env: Env, relayer: Address) -> Option<UnlockRequest> {
        env.storage()
            .persistent()
            .get(&DataKey::UnlockRequest(relayer))
    }

    /// Returns the SAC token address, or errors if not initialized.
    pub fn get_token(env: Env) -> Result<Address, EscrowError> {
        env.storage()
            .instance()
            .get(&DataKey::Token)
            .ok_or(EscrowError::NotInitialized)
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    fn assert_initialized(env: &Env) -> Result<(), EscrowError> {
        if !env.storage().instance().has(&DataKey::Token) {
            return Err(EscrowError::NotInitialized);
        }
        Ok(())
    }

    fn token(env: &Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Token)
            .unwrap() // safe: always called after assert_initialized
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

mod test;
