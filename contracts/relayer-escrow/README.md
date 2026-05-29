# Relayer Staking Collateral Escrow

A Soroban smart contract on the Stellar network that lets relayers deposit and withdraw SAC-compatible tokens as collateral. Staked balances are stored per-relayer in persistent storage and the contract enforces authorization on every state-changing call.

---

## Contract Functions

| Function | Description |
|---|---|
| `initialize(token)` | One-time setup — stores the SAC token address |
| `deposit_stake(relayer, amount)` | Transfers tokens from relayer into escrow, updates balance |
| `withdraw_stake(relayer, amount)` | Returns tokens from escrow to relayer |
| `get_stake(relayer)` | Returns the relayer's current staked balance |
| `get_token()` | Returns the SAC token address |

---

## Storage Layout

| Key | Type | Description |
|---|---|---|
| `Token` | Instance | SAC token address (set once at init) |
| `Stake(Address)` | Persistent | Staked balance per relayer |

---

## Security Properties

- `relayer.require_auth()` is called before any state change
- Amount validation (`> 0`) before any transfer
- Withdrawal is bounded by the relayer's stored balance
- Balance is updated **before** the outbound token transfer (checks-effects-interactions pattern)

---

## Prerequisites

- [Rust](https://rustup.rs/) with `wasm32-unknown-unknown` target
- [Stellar CLI](https://developers.stellar.org/docs/tools/developer-tools/cli/install-cli)

```bash
rustup target add wasm32-unknown-unknown
cargo install --locked stellar-cli --features opt
```

---

## Build

From the workspace root:

```bash
stellar contract build
```

Or from this contract's directory:

```bash
stellar contract build
```

The compiled WASM lands at:
```
target/wasm32v1-none/release/relayer_staking_escrow.wasm
```

---

## Run Tests

```bash
cargo test -p relayer_staking_escrow
```

---

## Deploy to Stellar Testnet

### 1. Create and fund a testnet identity

```bash
stellar keys generate --global alice --network testnet
stellar keys fund alice --network testnet
```

### 2. Deploy the contract

```bash
stellar contract deploy \
  --wasm target/wasm32v1-none/release/relayer_staking_escrow.wasm \
  --source alice \
  --network testnet
```

This prints a contract ID, e.g. `CXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX`.

### 3. Initialize with a SAC token

Replace `<CONTRACT_ID>` and `<TOKEN_ADDRESS>` with your values:

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source alice \
  --network testnet \
  -- initialize \
  --token <TOKEN_ADDRESS>
```

### 4. Deposit stake

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source alice \
  --network testnet \
  -- deposit_stake \
  --relayer <RELAYER_ADDRESS> \
  --amount 1000000
```

### 5. Check balance

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --network testnet \
  -- get_stake \
  --relayer <RELAYER_ADDRESS>
```

### 6. Withdraw stake

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source alice \
  --network testnet \
  -- withdraw_stake \
  --relayer <RELAYER_ADDRESS> \
  --amount 500000
```

---

## Testnet RPC

```
https://soroban-testnet.stellar.org
```

Network passphrase: `Test SDF Network ; September 2015`

---

## Events

| Topic | Data | Emitted by |
|---|---|---|
| `init` | `(token_address)` | `initialize` |
| `deposit` | `(relayer, amount)` | `deposit_stake` |
| `withdraw` | `(relayer, amount)` | `withdraw_stake` |
