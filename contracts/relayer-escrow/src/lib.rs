//! Relayer Staking Collateral Escrow
//!
//! Allows relayers to deposit and withdraw a SAC-compatible token as collateral.
//! Staked balances are stored per-relayer in persistent storage so they survive
//! ledger TTL extensions. The contract holds the tokens in its own account and
//! enforces authorization on every state-changing call.

#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, token, Address, Env,
};

// ---------------------------------------------------------------------------
// Storage keys
// ---------------------------------------------------------------------------

/// Storage key enum used for all contract state.
///
/// `Token`  — instance storage (one per contract, cheap to read on every call).
/// `Stake`  — persistent storage (one slot per relayer, survives ledger TTL).
#[contracttype]
pub enum DataKey {
    /// The SAC token address accepted by this escrow.
    Token,
    /// Staked balance for a specific relayer.
    Stake(Address),
}

// ---------------------------------------------------------------------------
// Error codes
// ---------------------------------------------------------------------------

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    /// Contract has already been initialized.
    AlreadyInitialized = 1,
    /// Contract has not been initialized yet.
    NotInitialized = 2,
    /// Amount must be greater than zero.
    InvalidAmount = 3,
    /// Withdrawal amount exceeds the relayer's staked balance.
    InsufficientBalance = 4,
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct RelayerStakingEscrow;

#[contractimpl]
impl RelayerStakingEscrow {
    // -----------------------------------------------------------------------
    // Admin / setup
    // -----------------------------------------------------------------------

    /// Initialize the escrow with the SAC token it will accept.
    ///
    /// Must be called exactly once. Subsequent calls return `Error::AlreadyInitialized`.
    pub fn initialize(env: Env, token: Address) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Token) {
            return Err(Error::AlreadyInitialized);
        }

        env.storage().instance().set(&DataKey::Token, &token);

        // Emit initialization event so indexers can track contract lifecycle.
        env.events().publish(
            (soroban_sdk::symbol_short!("init"),),
            (token,),
        );

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Core staking functions
    // -----------------------------------------------------------------------

    /// Deposit `amount` tokens from `relayer` into the escrow.
    ///
    /// - Requires authorization from `relayer`.
    /// - Transfers tokens from the relayer's account into this contract.
    /// - Adds `amount` to the relayer's persistent staked balance.
    /// - Emits a `deposit` event on success.
    pub fn deposit_stake(env: Env, relayer: Address, amount: i128) -> Result<(), Error> {
        // Guard: contract must be initialized.
        if !env.storage().instance().has(&DataKey::Token) {
            return Err(Error::NotInitialized);
        }

        // Guard: amount must be positive.
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        // Require the relayer to sign this transaction.
        relayer.require_auth();

        // Pull the token address from instance storage.
        let token_address: Address = env
            .storage()
            .instance()
            .get(&DataKey::Token)
            .unwrap(); // safe: we checked has() above

        // Transfer tokens from the relayer into this contract.
        let token_client = token::TokenClient::new(&env, &token_address);
        token_client.transfer(&relayer, &env.current_contract_address(), &amount);

        // Update the relayer's staked balance in persistent storage.
        let key = DataKey::Stake(relayer.clone());
        let current: i128 = env.storage().persistent().get(&key).unwrap_or(0);
        env.storage().persistent().set(&key, &(current + amount));

        // Emit deposit event: topic = "deposit", data = (relayer, amount).
        env.events().publish(
            (soroban_sdk::symbol_short!("deposit"),),
            (relayer, amount),
        );

        Ok(())
    }

    /// Withdraw `amount` tokens from the escrow back to `relayer`.
    ///
    /// - Requires authorization from `relayer`.
    /// - Fails with `Error::InsufficientBalance` if the relayer has less than `amount` staked.
    /// - Emits a `withdraw` event on success.
    pub fn withdraw_stake(env: Env, relayer: Address, amount: i128) -> Result<(), Error> {
        // Guard: contract must be initialized.
        if !env.storage().instance().has(&DataKey::Token) {
            return Err(Error::NotInitialized);
        }

        // Guard: amount must be positive.
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        // Require the relayer to sign this transaction.
        relayer.require_auth();

        // Check the relayer has enough staked.
        let key = DataKey::Stake(relayer.clone());
        let current: i128 = env.storage().persistent().get(&key).unwrap_or(0);
        if amount > current {
            return Err(Error::InsufficientBalance);
        }

        // Pull the token address from instance storage.
        let token_address: Address = env
            .storage()
            .instance()
            .get(&DataKey::Token)
            .unwrap(); // safe: we checked has() above

        // Update balance before the external call (checks-effects-interactions).
        let new_balance = current - amount;
        env.storage().persistent().set(&key, &new_balance);

        // Transfer tokens from this contract back to the relayer.
        let token_client = token::TokenClient::new(&env, &token_address);
        token_client.transfer(&env.current_contract_address(), &relayer, &amount);

        // Emit withdrawal event: topic = "withdraw", data = (relayer, amount).
        env.events().publish(
            (soroban_sdk::symbol_short!("withdraw"),),
            (relayer, amount),
        );

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Read-only queries
    // -----------------------------------------------------------------------

    /// Return the staked balance for `relayer`. Returns 0 if never deposited.
    pub fn get_stake(env: Env, relayer: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::Stake(relayer))
            .unwrap_or(0)
    }

    /// Return the SAC token address this escrow was initialized with.
    pub fn get_token(env: Env) -> Result<Address, Error> {
        env.storage()
            .instance()
            .get(&DataKey::Token)
            .ok_or(Error::NotInitialized)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

mod test;
