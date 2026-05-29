# Token Escrow Handshake Verification

A Soroban smart contract on the Stellar network that enforces a **two-step withdrawal handshake** before releasing staked tokens. This prevents assets from becoming permanently unrecoverable due to a missing or unconfirmed destination.

---

## The Problem

Releasing staked tokens directly to a destination wallet without an explicit confirmation handshake can cause assets to become permanently unrecoverable if the destination is wrong or unresponsive.

## The Solution

A two-step withdrawal flow:

| Step | Function | What happens |
|---|---|---|
| 1 | `initiate_unlock` | Deducts stake, starts 2-day cooldown timer |
| 2 | `claim_unlock` | After cooldown, relayer explicitly claims tokens |

The relayer must actively return to confirm the withdrawal — accidental or automated releases are impossible.

---

## Contract Functions

| Function | Description |
|---|---|
| `initialize(token)` | One-time setup — stores the SAC token address |
| `deposit_stake(relayer, amount)` | Transfers tokens from relayer into escrow |
| `initiate_unlock(relayer, amount)` | Starts the 2-day cooldown for `amount` tokens |
| `claim_unlock(relayer)` | Claims tokens after cooldown has elapsed |
| `get_stake(relayer)` | Returns current staked balance |
| `get_unlock_request(relayer)` | Returns pending unlock request, or `None` |
| `get_token()` | Returns the SAC token address |

---

## Storage Layout

| Key | Storage type | Description |
|---|---|---|
| `Token` | Instance | SAC token address |
| `Stake(Address)` | Persistent | Staked balance per relayer |
| `UnlockRequest(Address)` | Persistent | Pending unlock `{ amount, unlock_time }` |

---

## Error Codes

| Code | Name | Meaning |
|---|---|---|
| 1 | `NotInitialized` | Contract not yet initialized |
| 2 | `AlreadyInitialized` | `initialize` called more than once |
| 3 | `InsufficientStake` | Not enough staked balance |
| 4 | `PendingUnlockExists` | An unlock request already exists |
| 5 | `NoUnlockRequest` | No pending unlock to claim |
| 6 | `CooldownNotElapsed` | 2-day cooldown has not passed yet |
| 7 | `InvalidAmount` | Amount must be > 0 |

---

## Security Properties

- `require_auth()` called before every state mutation
- Checks-effects-interactions: storage updated **before** token transfers
- Cooldown enforced via `env.ledger().timestamp()`
- Double-unlock prevented by checking for existing `UnlockRequest`

---

## Prerequisites

```bash
rustup target add wasm32-unknown-unknown
cargo install --locked stellar-cli --features opt
```

---

## Build

```bash
stellar contract build
```

Output: `target/wasm32v1-none/release/token_escrow_handshake.wasm`

---

## Run Tests

```bash
cargo test -p token_escrow_handshake
```

---

## Deploy to Stellar Testnet

### 1. Create and fund a testnet identity

```bash
stellar keys generate --global alice --network testnet
stellar keys fund alice --network testnet
```

### 2. Deploy

```bash
stellar contract deploy \
  --wasm target/wasm32v1-none/release/token_escrow_handshake.wasm \
  --source alice \
  --network testnet
```

Save the printed contract ID as `$CONTRACT_ID`.

### 3. Initialize

```bash
stellar contract invoke \
  --id $CONTRACT_ID \
  --source alice \
  --network testnet \
  -- initialize \
  --token <SAC_TOKEN_ADDRESS>
```

### 4. Deposit stake

```bash
stellar contract invoke \
  --id $CONTRACT_ID \
  --source alice \
  --network testnet \
  -- deposit_stake \
  --relayer <RELAYER_ADDRESS> \
  --amount 1000000
```

### 5. Initiate unlock (starts 2-day cooldown)

```bash
stellar contract invoke \
  --id $CONTRACT_ID \
  --source alice \
  --network testnet \
  -- initiate_unlock \
  --relayer <RELAYER_ADDRESS> \
  --amount 500000
```

### 6. Claim after cooldown

```bash
stellar contract invoke \
  --id $CONTRACT_ID \
  --source alice \
  --network testnet \
  -- claim_unlock \
  --relayer <RELAYER_ADDRESS>
```

---

## Events

| Topic | Data | Emitted by |
|---|---|---|
| `init` | `(token)` | `initialize` |
| `deposit` | `(relayer, amount)` | `deposit_stake` |
| `unlock` | `(relayer, amount, unlock_time)` | `initiate_unlock` |
| `claim` | `(relayer, amount)` | `claim_unlock` |

---

## Testnet RPC

```
https://soroban-testnet.stellar.org
```

Network passphrase: `Test SDF Network ; September 2015`
