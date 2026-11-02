# New Anchor project from scratch: SBPF v3 + LiteSVM tests (+ v1 transactions)

Every command and output below was run on 2026-11-02 (macOS arm64). Start here for a new Anchor
project. For Pinocchio use `../pinocchio.md`. For background and the full issue history see
`../anchor-vault/steps.md`.

## 0. Versions that were verified together

| Tool | Version | Installed with |
|---|---|---|
| anchor-cli | **1.2.2** (latest) | `avm` |
| Agave (solana CLI, `cargo build-sbf`) | 4.1.2 **or** 4.4.0, both verified | `agave-install` |
| platform-tools | v1.57 (what `anchor build` passes by default) | automatic |
| host rustc | 1.89.0 (path A) / 1.99.1 (path B) | `rustup` (automatic via `rust-toolchain.toml`) |
| litesvm | 0.15.0 (path A, scaffold default) / 0.18.1 (path B, latest) | Cargo |

Pick a path:
- **Path A, the default scaffold:** no edits. `anchor test` passes on SBPF v3. LiteSVM 0.15.0
  also runs v1 transactions. Rust 1.89.
- **Path B, latest LiteSVM (0.18.1):** this is what `real-vault` uses. It needs rustc 1.99.1
  and two pins. Choose it if you want the newest test crates or will add the RPC client from
  `anchor-vault` (that client needs rustc ≥ 1.98).

## 1. Install the tools (once per machine)

```sh
# Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Agave / Solana CLI (pick one version; both work for everything below)
sh -c "$(curl -sSfL https://release.anza.xyz/v4.4.0/install)"   # fresh machine
agave-install init 4.4.0                                          # machine with agave-install
export PATH="$HOME/.local/share/solana/install/active_release/bin:$PATH"   # if the installer says so

# Anchor version manager + Anchor (from the official Anchor docs; avm comes from the otter-sec repo,
# NOT the unrelated `avm` crate on crates.io)
cargo install --git https://github.com/otter-sec/anchor avm --force
avm install 1.2.2
avm use 1.2.2

# Wallet (skip if ~/.config/solana/id.json exists)
solana-keygen new

# Check
anchor --version          # anchor-cli 1.2.2
solana --version          # solana-cli 4.4.0 (or 4.1.2)
```

## 2. Path A: scaffold and test, no changes

```sh
anchor init demo-vault --no-install     # --no-install: skip JS package install (none needed)
cd demo-vault
anchor test
xxd -s 0x30 -l 4 target/deploy/demo_vault.so    # 0300 0000 = SBPF v3
```

Observed (first run ≈ 5 min, mostly compiling):

```
running 1 test
test test_initialize ... ok
test result: ok. 1 passed; 0 failed; ...
00000030: 0300 0000
```

What `anchor init` 1.2.2 generates:

| File | Content |
|---|---|
| `Anchor.toml` | `skip_local_validator = true`, `cluster = "localnet"`, `[scripts] test = "cargo test"`, program ID |
| `rust-toolchain.toml` | `channel = "1.89.0"` |
| `Cargo.toml` | workspace `programs/*`, `rust-version = "1.89.0"`, release profile `lto = "fat"` |
| `programs/demo-vault/Cargo.toml` | `anchor-lang = "1.2.2"`. Dev: `litesvm = "0.15.0"`, `solana-message = "4"`, `solana-transaction = "4"`, `solana-signer = "3"`, `solana-keypair = "3"` |
| `Cargo.lock` | Pre-resolved for Rust 1.89 (Agave crates 4.1.2, solana-message 4.3.0). **Commit it, and don't run a bare `cargo update`** |
| `programs/demo-vault/src/` | Counter program: `initialize`, `increment`, `constants.rs` (`COUNTER_SEED`), `error.rs`, `state.rs` |
| `programs/demo-vault/tests/test_initialize.rs` | LiteSVM test: legacy-message transactions for initialize + increment |
| `security.json` | Security metadata template (`--no-security-metadata` skips it). Upload with `anchor deploy --security-metadata` once it's filled in |
| `target/deploy/demo_vault-keypair.json` | **The program ID keypair.** Back it up: `cargo clean` deletes it |

Under Anchor 1.2.1 the same `anchor init` generated `litesvm = "0.10.0"`, which can't load
v3, so `anchor test` failed with `InvalidAccountData`. That's why `real-vault` needed path B.

### Optional: v1 transactions in path A tests

Verified with litesvm 0.15.0 in the scaffold (1 passed). Save as
`programs/demo-vault/tests/test_v1.rs` and replace `demo_vault` with your crate name:

```rust
use {
    anchor_lang::{
        prelude::Pubkey,
        solana_program::{instruction::Instruction, system_program},
        AccountDeserialize, InstructionData, ToAccountMetas,
    },
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_message::{v1, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};

#[test]
fn v1_initialize() {
    let program_id = demo_vault::id();
    let payer = Keypair::new();
    let counter =
        Pubkey::find_program_address(&[demo_vault::constants::COUNTER_SEED], &program_id).0;
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!(concat!(env!("CARGO_TARGET_TMPDIR"), "/../deploy/demo_vault.so"));
    svm.add_program(program_id, bytes).unwrap();
    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
    let ix = Instruction::new_with_bytes(
        program_id,
        &demo_vault::instruction::Initialize {}.data(),
        demo_vault::accounts::Initialize { payer: payer.pubkey(), counter, system_program: system_program::ID }
            .to_account_metas(None),
    );
    // v1: unset limits are ZERO, so always set the CU limit and the loaded-accounts size.
    let cfg = v1::TransactionConfig::empty()
        .with_compute_unit_limit(50_000)
        .with_loaded_accounts_data_size_limit(256 * 1024);
    let msg = v1::Message::try_compile_with_config(&payer.pubkey(), &[ix], svm.latest_blockhash(), cfg).unwrap();
    let tx = VersionedTransaction::try_new(VersionedMessage::V1(msg), &[&payer]).unwrap();
    let res = svm.send_transaction(tx);
    println!("{res:?}");
    assert!(res.is_ok());
    let acc = svm.get_account(&counter).unwrap();
    let state = demo_vault::state::Counter::try_deserialize(&mut acc.data.as_slice()).unwrap();
    assert_eq!(state.count, 0);
}
```

```sh
anchor test      # or, after one anchor build: cargo test -p demo-vault --test test_v1
```

## 3. Path B: LiteSVM 0.18.1 + rustc 1.99.1 (what real-vault uses)

Start from the path A scaffold, then make three edits and one lockfile pin.

### 3.1 `rust-toolchain.toml`

```toml
[toolchain]
channel = "1.99.1"
components = ["rustfmt","clippy"]
profile = "minimal"
```

litesvm 0.18.1 has `rust-version = 1.97.1`, and the Agave 4.x crates it pulls need ≥ 1.98.
**Leave `rust-version = "1.89.0"` in the root `Cargo.toml`.** It also applies to the program,
which platform-tools compiles with rustc 1.95.0-dev. Raising it breaks `anchor build` with
`rustc 1.95.0-dev is not supported`.

### 3.2 `programs/<name>/Cargo.toml` → replace the whole `[dev-dependencies]` block

```toml
[dev-dependencies]
litesvm = "0.18.1"
solana-program-runtime = "~4.3"
solana-message = "4.2.4"
solana-transaction = { version = "4.1.5", features = ["verify"] }
solana-signer = "3.0.1"
solana-keypair = "3.1.2"
```

- `solana-program-runtime = "~4.3"` isn't used in code. litesvm 0.18.1 doesn't compile against
  Agave 4.4, and this line holds the whole Agave family at 4.3.x.
- The other versions are the ones litesvm 0.18.1 uses, so the test types match.

### 3.3 Re-resolve the lockfile and pin `solana-fee`

```sh
cargo generate-lockfile
cargo update -p solana-fee --precise 4.3.2
```

Without the pin: `SanitizedMessage: SVMStaticMessage is not satisfied` (solana-fee 4.4.0 is
outside the `~4.3` family). Re-checked 2026-11-02.

### 3.4 Test

```sh
anchor test
xxd -s 0x30 -l 4 target/deploy/<name>.so    # 0300 0000
```

Observed on the Anchor 1.2.2 scaffold: `test test_initialize ... ok`, v3. Resolved versions:
`anchor-lang 1.2.2`, `litesvm 0.18.1`, `solana-program-runtime 4.3.2`, `solana-fee 4.3.2`.

The generated `test_initialize.rs` needs no change for either path.

## 4. Deploy to devnet

```sh
solana config set --url devnet
solana airdrop 2                                     # or https://faucet.solana.com
solana feature status -u devnet 5cC3foj77CWun58pC51ebHFUWavHWKarWyR5UUik7dnC   # SBPF v3: "active since epoch 1069"

anchor keys list                                     # program ID from target/deploy/*-keypair.json
anchor keys sync                                     # only if declare_id!/Anchor.toml differ from it

anchor build                                         # deploy uploads whatever .so is in target/deploy
xxd -s 0x30 -l 4 target/deploy/<name>.so             # 0300 0000
anchor deploy --provider.cluster devnet              # also uploads the IDL (extra SOL); --no-idl to skip
solana program show <PROGRAM_ID> -u devnet
solana program dump <PROGRAM_ID> /tmp/p.so -u devnet && xxd -s 0x30 -l 4 /tmp/p.so   # deployed version
```

- **Don't set `[provider] cluster = "devnet"` in `Anchor.toml`** unless you want every
  `anchor test` to deploy to devnet first. Pass `--provider.cluster devnet` per command, or use
  `anchor test --skip-deploy`.
- `anchor deploy` never compiles. If you ran `cargo build-sbf --arch v2` (or a bare
  `cargo build-sbf` on CLI 4.1.2, which defaults to **v0**), that is what gets uploaded.

## 5. Add a v1-transaction RPC client

Use path B (rustc 1.99.1). Copy the working client from `anchor-vault`, which has run on devnet.
Verified 2026-11-02 on a fresh Anchor 1.2.2 scaffold named `demo-vault` with path B applied:
builds, and against a 4.4.0 local validator prints initialize → increment → `rejected with
Unauthorized` → `all checks passed`, each transaction read back as v1. `anchor test` still
passed afterwards.

The scaffold template is the same counter (`COUNTER_SEED`, `MAX_COUNT`, `Unauthorized`,
`CounterOverflow`), so the client compiles after a rename. Run from the project root, replacing
`demo-vault` / `demo_vault` with your names:

```sh
cp -R ../anchor-vault/client ./client && rm -rf client/target   # project created inside contract-practice/
sed -i '' 's/members = \[/members = [\n    "client",/' Cargo.toml          # workspace member
sed -i '' -e 's/anchor-vault-client/demo-vault-client/' \
  -e 's|^anchor-vault = { path = "../programs/anchor-vault"|demo-vault = { path = "../programs/demo-vault"|' \
  client/Cargo.toml
sed -i '' 's/anchor_vault/demo_vault/g' client/src/main.rs
cargo build -q -p demo-vault-client
```

(`sed -i ''` is the macOS form; on Linux use `sed -i`.)

Then:
1. `client/src/main.rs`: **delete the hard-coded Helius URL/API key in `DEFAULT_RPC_URL`**.
   Use `RPC_URL` or the first argument instead.
2. Keep `solana-rpc-client(-api) = "~4.3"`. The workspace has one lockfile, and litesvm 0.18.1
   holds Agave at 4.3.x. `solana-fee` stays at 4.3.2.
3. Devnet run: `RPC_URL=https://api.devnet.solana.com cargo run -q -p demo-vault-client`.
   This sends real transactions from `~/.config/solana/id.json`.

For a local run you need a **4.4.0** validator. The 4.1.2 `solana-test-validator` rejects v1
(`Transaction did not pass signature verification`):

```sh
A=~/.local/share/solana/install/releases/4.4.0/solana-release/bin   # set in BOTH terminals
$A/solana-test-validator --reset                                     # terminal 1
export PATH=$A:$PATH                                                 # terminal 2
solana airdrop 100 -u localhost && anchor deploy --provider.cluster localnet
cargo run -q -p <client-package> -- http://127.0.0.1:8899
```

v1 rules (SIMD-0385, details in `../simd.md`): set `with_compute_unit_limit` and
`with_loaded_accounts_data_size_limit` (unset = 0). The loaded-accounts limit must cover the
ProgramData account (~128 KB for this Anchor program, so 256 KiB). `with_priority_fee` is
**total lamports**. Read transactions back with `max_supported_transaction_version: Some(1)`.
No address lookup tables.

## 6. Quick checks when something fails

| Symptom | Fix |
|---|---|
| `InvalidAccountData` at `svm.add_program` | The LiteSVM version can't load that SBPF version: use path A (0.15.0) or B (0.18.1). Check with `xxd -s 0x30 -l 4` |
| litesvm 0.18.1 fails to compile (`new_builtin`, `Arc<SlotHashes>`) | Missing `solana-program-runtime = "~4.3"` |
| `SanitizedMessage: SVMStaticMessage` | Missing `cargo update -p solana-fee --precise 4.3.2` |
| `requires rustc 1.98` / `1.97.1` | Path B needs `rust-toolchain.toml` 1.99.1 |
| `rustc 1.95.0-dev is not supported` | Root `rust-version` raised above 1.95. Put it back to `"1.89.0"` |
| `expected X, found X` in tests | Dev-dependency versions don't match litesvm's (section 3.2) |
| v1 tx fails locally, works on devnet | Local validator is 4.1.2; use 4.4.0 |
| `solana confirm -v` can't show a v1 tx | CLI 4.1.2; use 4.4.0 |

Deeper method: `../deps.md`. Version switching: `../commands.md`.

## 7. What was done for real-vault itself (history)

`real-vault` was scaffolded with **Anchor 1.2.1** (template `litesvm = "0.10.0"`,
`solana-message = "3.0.1"`, `solana-transaction = "3.0.2"`, `solana-signer = "3.0.0"`,
`solana-keypair = "3.0.1"`, rustc 1.89.0). Changes applied:

```diff
 # rust-toolchain.toml
-channel = "1.89.0"
+channel = "1.99.1"

 # programs/real-vault/Cargo.toml [dev-dependencies]
-litesvm = "0.10.0"
-solana-message = "3.0.1"
-solana-transaction = "3.0.2"
+litesvm = "0.18.1"
+solana-program-runtime = "~4.3"
+solana-message = "4.2.4"
+solana-transaction = { version = "4.1.5", features = ["verify"] }
 solana-signer = "3.0.0"
 solana-keypair = "3.0.1"
```

```sh
cargo generate-lockfile
cargo update -p solana-fee --precise 4.3.2
anchor test        # test test_initialize ... ok, v3
```

State on 2026-11-02:
- `Cargo.toml` has `anchor-lang = "1.2.1"` and `Cargo.lock` resolves 1.2.2 (`^1.2.1`), the same
  as the installed CLI.
- Re-verified on Agave 4.4.0: build, 1/1 test, local deploy, and a `.so` byte-identical to the
  4.1.2 build.
- Program ID: `B5DuAWy4xkAUiniLkXbiey14tBbSn9C7CKq93JXiKVWA`, `cluster = "localnet"`.
