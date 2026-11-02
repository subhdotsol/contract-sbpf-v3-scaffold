# anchor-vault: steps after `anchor init`

Built with: anchor-cli 1.2.1, solana-cli 4.1.2 (Agave), platform-tools v1.57.
Host rustc: started at 1.89.0, now **1.99.1** (pinned in `rust-toolchain.toml`, see section 7).

> **Status on 2026-11-02**
> - Machine now: anchor-cli **1.2.2** (avm), Agave **4.1.2** active, 4.4.0 also cached in
>   `~/.local/share/solana/install/releases/`. `Cargo.lock` already resolves
>   `anchor-lang 1.2.2`, because `"1.2.1"` means `^1.2.1`.
> - Re-verified with Agave 4.4.0 (section 14). Build, tests, deploy and the v1 client all work,
>   and the `.so` is byte-identical to the 4.1.2 build.
> - The v1 client has run on devnet. Signature `5dataz3H...` reads back as `Version: 1`,
>   `Compute Unit Limit: 50000`, `Priority Fee: 1000 lamports`.
> - **Starting a new project?** Use `../real-vault/steps.md`. With Anchor 1.2.2, `anchor init`
>   passes `anchor test` on SBPF v3 with no changes. Most of sections 2-3 and 7 below is history.

Quick map: sections 1-8 are the steps in order. Section 9 is the v1-transaction client,
section 10 lists every issue and its fix, section 11 is lessons learned, and section 12
explains every crate in the `Cargo.toml` files. Section 13 is a debugging guide, section 14
covers the Agave 4.4.0 upgrade, and section 15 lists related docs.

## 1. Scaffold

```sh
anchor init anchor-vault
cd anchor-vault
```

This generates the counter program (`initialize` / `increment`), a LiteSVM Rust test
(`programs/anchor-vault/tests/test_initialize.rs`) and `Anchor.toml` with
`[scripts] test = "cargo test"`.

## 2. Run `anchor test` and diagnose the failure

```sh
anchor test
```

Failure:

```
test_initialize.rs:29:40: called `Result::unwrap()` on an `Err` value: Instruction(InvalidAccountData)
```

Line 29 is `svm.add_program(program_id, bytes).unwrap()`. LiteSVM could not load the `.so` at all.

Cause:
- Anchor 1.2.1 builds programs as **SBPF v3** by default (`anchor build --arch`, default `v3`).
  You can confirm with `xxd -s 0x30 -l 4 target/deploy/anchor_vault.so` → `0300 0000` means v3.
- The generated dev-dependency `litesvm = "0.10.0"` (solana-sbpf 0.13) cannot load that v3 ELF.
- Upgrading LiteSVM (0.17+) pulls Agave 4.x crates that need rustc ≥ 1.98, but the project pins 1.89.
- `anchor test` has no `--arch` flag and `Anchor.toml` has no arch setting.

> Fixed upstream in Anchor 1.2.2: `anchor init` now generates `litesvm = "0.15.0"` plus a
> `Cargo.lock` that works with rustc 1.89. litesvm 0.15.0 loads the v3 `.so`, so `anchor test`
> passes on a fresh scaffold (verified 2026-11-02). The problem above only affects projects
> scaffolded with Anchor 1.2.1 (`litesvm = "0.10.0"`).

## 3. Fix: rebuild as SBPF v2 inside the test script

`Anchor.toml`:

```toml
[scripts]
test = "cargo build-sbf --tools-version v1.57 --arch v2 && cargo test"
```

`anchor test` still does its own v3 build. The script then overwrites
`target/deploy/anchor_vault.so` with a v2 build, which LiteSVM can load, and runs the tests.
`--tools-version v1.57` matches the platform-tools version Anchor uses, so nothing new is downloaded.

## 4. Add increment tests

New file `programs/anchor-vault/tests/test_increment.rs`:

| test | checks |
|---|---|
| `test_increment_by_authority` | count goes 0 → 1 → 2; authority unchanged |
| `test_increment_unauthorized` | another signer gets `Unauthorized` (6000); count unchanged |
| `test_increment_overflow` | after `MAX_COUNT` (10) increments, the next one gets `CounterOverflow` (6001) |

```sh
anchor test   # 4 tests pass (1 initialize + 3 increment)
```

## 5. Deploy to devnet

```sh
solana config set --url devnet
solana airdrop 2              # if the wallet needs SOL
anchor deploy --provider.cluster devnet
solana program show 8Tf2yD9hUxbuv2Dx4jaBNrpz2Aej15ajetzVxqayZzQY -u devnet
```

The `[provider] cluster` in `Anchor.toml` was then set to `"devnet"`.

Deploy history of program `8Tf2yD9hUxbuv2Dx4jaBNrpz2Aej15ajetzVxqayZzQY` on devnet:

| Slot | What was uploaded | Why |
|---|---|---|
| 516701354 | SBPF **v2** build | `anchor program deploy` uploaded the `.so` left by the v2 test script |
| 516860415 | SBPF **v3** build | `anchor build` (v3 default) then deploy |

Check which version is deployed:
```sh
solana program dump <PROGRAM_ID> /tmp/p.so -u devnet && xxd -s 0x30 -l 4 /tmp/p.so
# 0200 0000 = v2, 0300 0000 = v3
```
The dumped file is zero-padded to the program account's allocated size, so compare hashes only
over the first `stat -f%z target/deploy/anchor_vault.so` bytes.

> Note: with `cluster = "devnet"`, `anchor test` deploys to devnet before running the tests.
> Use `anchor test --skip-deploy` (or set `cluster` back to `"localnet"`) for local-only testing.

## 6. Rust client that calls the deployed program (first version, anchor-client)

> This first version is now commented out at the top of `client/src/main.rs`; section 9 describes
> the current v1-transaction client.

Add a workspace member, `client/`.

`Cargo.toml` (root):

```toml
[workspace]
members = ["programs/*", "client"]
```

`client/Cargo.toml`:

```toml
[dependencies]
anchor-client = "1.2.1"
anchor-vault = { path = "../programs/anchor-vault", features = ["no-entrypoint"] }
solana-keypair = "3.0.1"
anyhow = "1"
```

`client/src/main.rs` does the following:
1. Loads the wallet (`~/.config/solana/id.json` by default).
2. Derives the counter PDA (`seeds = ["counter"]`).
3. Calls `initialize` if the counter does not exist yet, then checks that `count == 0` and `authority == payer`.
4. Calls `increment` and checks that the count went up by 1. If the count is already at `MAX_COUNT`, it instead checks for a `CounterOverflow` rejection.
5. Calls `increment` signed by a random keypair and checks that it is rejected (`Unauthorized`) and the count did not change.

Fix made along the way: the client got `too many signers` because the authority was also the
payer, so the same keypair was added twice. It now adds `.signer(authority)` only when
`authority != payer`.

RPC selection, first match wins: CLI argument → `RPC_URL` env var → the devnet Helius URL hard-coded in `main.rs`.

```sh
cargo run -q -p anchor-vault-client
# or
RPC_URL="https://api.devnet.solana.com" cargo run -q -p anchor-vault-client
```

Expected output:

```
[initialize] skipped, counter already exists (count = 0)
[increment] tx: ...
[increment] ok: count 0 -> 1
[unauthorized] ok: rejected with Unauthorized

all checks passed
```

> The Helius API key is hard-coded as the default in `client/src/main.rs`. Remove it and rely on
> `RPC_URL` before committing or sharing the repo.

## 7. Can we test with SBPF v3 instead of v2?

**Yes, and this repo now does.** The steps below are applied and verified: `anchor test --skip-deploy`
passes all 4 tests against a v3 `.so` (version byte `0300 0000`). The old v2 lines are kept as
comments in each file. See "Applied changes" at the end of this section for the exact versions.

The v2 workaround exists only because the test harness (`litesvm 0.10.0`, which uses solana-sbpf
0.13) cannot load the v3 ELF that platform-tools v1.57 produces. The program code itself does not
need to change. To test the v3 build directly:

1. **Raise the host Rust toolchain.** Newer LiteSVM pulls Agave 4.x crates that need rustc ≥ 1.98.
   In `rust-toolchain.toml`:
   ```toml
   channel = "1.99.1"   # any version >= 1.98
   ```
   Keep `rust-version = "1.89.0"` in the root `Cargo.toml`. That value also applies to the program
   crate, and the program is compiled by the Solana platform-tools rustc (1.95.0-dev for v1.57).
   Setting it to 1.99 makes `cargo build-sbf` fail with
   `rustc 1.95.0-dev is not supported ... anchor-vault requires rustc 1.99.1`.

2. **Upgrade LiteSVM** in `programs/anchor-vault/Cargo.toml`:
   ```toml
   [dev-dependencies]
   litesvm = "0.18.1"
   ```
   If the tests fail to compile with type mismatches (`Message`, `VersionedTransaction`, `Keypair`),
   bump `solana-message`, `solana-transaction`, `solana-signer` and `solana-keypair` to the versions
   LiteSVM uses (check with `cargo tree -p litesvm`).

3. **Restore the plain test script** in `Anchor.toml`, so the v3 `.so` Anchor builds is the one tested:
   ```toml
   [scripts]
   test = "cargo test"
   ```

4. Run:
   ```sh
   cargo update
   anchor test --skip-deploy   # or set [provider] cluster = "localnet"
   ```
   Confirm the tested binary is v3 with `xxd -s 0x30 -l 4 target/deploy/anchor_vault.so`
   (`0300 0000` = v3, `0200 0000` = v2).

**Applied changes** (v2 lines left commented in each file):

| File | v2 (old, commented) | v3 (current) |
|---|---|---|
| `rust-toolchain.toml` | `channel = "1.89.0"` | `channel = "1.99.1"` |
| `Anchor.toml` | `test = "cargo build-sbf --tools-version v1.57 --arch v2 && cargo test"` | `test = "cargo test"` |
| `programs/anchor-vault/Cargo.toml` | `litesvm = "0.10.0"`, `solana-message = "3.0.1"`, `solana-transaction = "3.0.2"` | `litesvm = "0.18.1"`, `solana-program-runtime = "~4.3"`, `solana-message = "4.2.4"`, `solana-transaction = { version = "4.1.5", features = ["verify"] }` |
| `client/Cargo.toml` | `solana-rpc-client(-api) = "~4.2"` | `solana-rpc-client(-api) = "~4.3"` |

Two gotchas hit while applying it:
- **Agave 4.4 breaks litesvm 0.18.1.** litesvm asks for `^4.3.0`, but Agave's internal crates
  changed APIs in 4.4.0 (e.g. `ProgramCacheEntry::new_builtin`), so a plain `cargo update` picks
  4.4 and litesvm fails to compile. Fix: `solana-program-runtime = "~4.3"` (the Agave crates pin
  each other exactly, so this holds the whole family at 4.3.x), plus
  `cargo update -p solana-fee --precise 4.3.2` because `solana-fee` is not part of that exact pin.
- **The test dev-dependencies must match litesvm's types.** With `solana-message`/`solana-transaction`
  3.x, `VersionedTransaction::try_new` doesn't exist on the type litesvm expects. Bumped to the
  versions litesvm 0.18.1 uses (message 4.2.4, transaction 4.1.5 with the `verify` feature).

Revert to v2: uncomment the v2 lines, comment the v3 lines, delete the `solana-program-runtime`
line, and run `cargo update`.

**Deploying v3 on-chain** also needs the cluster's v3 feature gate
(`enable_sbpf_v3_deployment_and_execution`, ID `5cC3foj77CWun58pC51ebHFUWavHWKarWyR5UUik7dnC`).
On 2026-11-02 it is active on devnet (slot 461,808,000), testnet and mainnet. Check it by ID.
`solana feature status -u devnet | grep -i sbpf` does **not** show this gate on CLI 4.1.2.
```sh
solana feature status -u devnet 5cC3foj77CWun58pC51ebHFUWavHWKarWyR5UUik7dnC
# 4.1.2 and 4.4.0: "active since epoch 1069"
```
If a cluster doesn't have v3, deploy a v2 build instead:
```sh
cargo build-sbf --tools-version v1.57 --arch v2
anchor deploy --provider.cluster devnet
```

## 8. SBPF v2 vs v3

SBPF ("Solana BPF") is the bytecode format Solana programs compile to. Each version is a set of
rule changes defined in SIMDs and enabled on a cluster by a feature gate
(`enable_sbpf_vN_deployment_and_execution`). Versions build on each other: v3 includes everything
in v2. Gates: v2 `F6UVKh1ujTEFK3en2SyAL3cdVnqko1FVEXWhmdLRu6WP`, v3
`5cC3foj77CWun58pC51ebHFUWavHWKarWyR5UUik7dnC`. Both are active on devnet, testnet and mainnet
(2026-11-02). How to read SIMDs and check gates: `../simd.md`.

| | SBPF v2 | SBPF v3 |
|---|---|---|
| Main SIMDs (as listed in Agave's gate description) | SIMD-0173 (instruction encoding improvements), SIMD-0174 (wide multiplication, signed division, explicit sign extension) | SIMD-0178 (static syscalls), SIMD-0189 (stricter ELF headers), SIMD-0377 (eBPF ISA compatibility) |
| Focus | Simpler, more regular instruction set | No runtime relocations, strict ELF layout, ISA aligned with upstream eBPF/LLVM |
| Syscalls | Resolved at load time through ELF relocations | Static syscalls (SIMD-0178): encoded in the `call` instruction by the murmur32 hash of the name, resolved at link time, no runtime relocation |
| ELF | Relocations and dynamic sections (`.dynamic`, `.dynsym`, `.rel.dyn`) still processed by the loader | Fixed program headers, `e_entry` must lie in the second program header, section headers ignored (SIMD-0189) |
| Instruction set | SBPF-specific encodings | Matches the eBPF ISA that LLVM emits (SIMD-0377): reverts some earlier SBPF changes, re-encodes instructions (e.g. `callx` takes its target from the destination register), adds new ones |
| Tooling | `cargo build-sbf --arch v2` | `cargo build-sbf --arch v3`. Default in `anchor build` 1.2.x and `cargo build-sbf` 4.5.0; **`cargo build-sbf` 4.1.0 defaults to v0** |
| On-chain requirement | v2 gate active | v3 gate active |

(An earlier version of this table listed "SIMD-0179, stricter control-flow checks". Agave's v3
gate doesn't mention 0179, and the SIMD repo has no 0179 proposal file.)

In practice for this project:
- Program source code is identical. Only the compiled `.so` differs (the ELF header `e_flags` byte
  at offset `0x30` holds the version).
- Every tool that loads the binary (validator, LiteSVM, Mollusk, Surfpool) must implement the same
  version of the rules as the compiler. That mismatch is why the old `litesvm 0.10` rejected
  Anchor's v3 build with `InvalidAccountData`.
- v3 is the newer format; v2 is the safer choice when tools or clusters lag behind.


## 9. Client moved to transaction v1 (4096-byte transactions)

The `anchor-client` code is kept, commented out, at the top of `client/src/main.rs`. Below a
separator banner is the new client, which builds **v1 transactions** (SIMD-0385) by hand:

```rust
const CONFIG: v1::TransactionConfig = v1::TransactionConfig::empty()
    .with_compute_unit_limit(50_000)
    .with_loaded_accounts_data_size_limit(256 * 1024)
    .with_priority_fee(1_000);            // TOTAL lamports, not micro-lamports per CU

let message = v1::Message::try_compile_with_config(&payer.pubkey(), &[ix], blockhash, CONFIG)?;
let tx = VersionedTransaction::try_new(VersionedMessage::V1(message), signers)?;
rpc.send_and_confirm_transaction(&tx)?;
```

After each send it reads the transaction back with `max_supported_transaction_version: Some(1)` and
checks it landed as `VersionedMessage::V1` with the same config.

What differs from legacy/v0:

| | legacy / v0 | v1 |
|---|---|---|
| Max size | 1232 bytes | 4096 bytes |
| Compute budget | ComputeBudget instructions | `TransactionConfig` in the message header |
| Unset CU / data-size limit | runtime default | **zero**, so always set both |
| Priority fee | micro-lamports per CU | total lamports |
| Address lookup tables | v0 only | not supported |
| Reading it back | default works | needs `max_supported_transaction_version: Some(1)` |

Checks done before writing it:
- v1 is active on devnet: the feature account `txv1aq4pp281K9um3tnPgkfX8UqtFT6wcVW3hNezGLL` holds
  an activation slot (492,480,000). `solana feature status` doesn't help on CLI 4.1.2:
  querying this ID gives `Error: Unknown feature`, and the full list shows an older ID,
  `txv1hPU7...`, as inactive. CLI 4.4.0 shows `active since epoch 1140`. Or query
  `getAccountInfo` directly (section 13, step 8).
- The loaded-accounts limit is 256 KiB because the transaction loads the program's ProgramData
  account (~137 KB). A smaller limit fails with `MaxLoadedAccountsDataSizeExceeded`.

Status: **works on devnet** (v1 transaction `5dataz3H...` confirmed as `Version: 1`). Also verified
end to end on a local Agave **4.4.0** validator: initialize → increment → unauthorized rejected,
each read back as v1.

Reading a v1 transaction with the CLI needs CLI **4.4.0**. CLI 4.1.2 fails with `Transaction
version (1) is not supported by the requesting client`. `solana-test-validator` **4.1.2**
rejects final-format v1 transactions (`Transaction did not pass signature verification`), so
test the client locally only with a 4.4.0 validator:
```sh
A=~/.local/share/solana/install/releases/4.4.0/solana-release/bin   # set this in BOTH terminals
$A/solana-test-validator --reset                                    # terminal 1
export PATH=$A:$PATH                                                # terminal 2: 4.4.0 CLI (verified)
solana airdrop 100 -u localhost && anchor deploy --provider.cluster localnet
cargo run -q -p anchor-vault-client -- http://127.0.0.1:8899
```

Against devnet (default RPC, or `RPC_URL` / the first argument):

```sh
cargo run -q -p anchor-vault-client
```

## 10. Every issue hit, and the fix

| # | Symptom | Cause | Fix |
|---|---|---|---|
| 1 | `anchor test`: `Instruction(InvalidAccountData)` at `svm.add_program` | Anchor 1.2.1 builds SBPF v3; `litesvm 0.10` can only load up to v2 | First: test script rebuilds as v2 (section 3). Later: upgrade LiteSVM (section 7) |
| 2 | `cargo build-sbf --arch v2`: platform-tools v1.54 download failed | `cargo build-sbf` 4.1.0 defaults to platform-tools v1.54, which wasn't installed; Anchor uses v1.57 | Pass `--tools-version v1.57` (already in `~/.cache/solana/`) |
| 3 | `litesvm 0.17`: `solana-* 4.4.0 requires rustc 1.98.1` | Newer LiteSVM pulls Agave 4.x crates with a higher MSRV than the pinned 1.89 | Kept v2 at first; later raised `rust-toolchain.toml` to 1.99.1 |
| 4 | `anchor test` has no `--arch` flag | Not supported by anchor-cli 1.2.1 | Run `cargo build-sbf` from `[scripts] test` |
| 5 | Client: `too many signers` | The authority was also the payer, so the same keypair was added as a signer twice | Add `.signer(authority)` only when `authority != payer` |
| 6 | Deployed program was v2 although `anchor build` defaults to v3 | Deploy uploads whatever `.so` is in `target/deploy`; the last build there was the v2 test script's | Run `anchor build` right before deploying; verify with `xxd` |
| 7 | v1 client: `solana-rpc-client 4.4 requires rustc 1.98.1` (while still on 1.89) | RPC client 4.4 MSRV | Pinned `solana-rpc-client(-api) = "~4.2"` |
| 8 | `~4.2` RPC pin vs `solana-transaction = "4.2"`: no version satisfies both | `solana-rpc-client 4.2` requires `solana-transaction <4.2` | Used `solana-message`/`solana-transaction = "4.1"` (4.1 already has `VersionedMessage::V1`) |
| 9 | `rustc 1.95.0-dev is not supported ... anchor-vault requires rustc 1.99.1` | Workspace `rust-version` also applies to the program, which platform-tools compiles with rustc 1.95.0-dev | Keep `rust-version = "1.89.0"`; raise only `rust-toolchain.toml` |
| 10 | litesvm 0.18 vs the client's `~4.2` pin: `solana-epoch-schedule` conflict | One lockfile for the whole workspace, so both crates must agree on shared deps | Moved client RPC crates to `~4.3` |
| 11 | litesvm 0.18.1 fails to compile (`new_builtin takes 1 argument`, `Arc<SlotHashes>` mismatch) | litesvm requires `^4.3.0`, so cargo picked Agave 4.4.0, which changed those APIs | `solana-program-runtime = "~4.3"` (holds the exact-pinned Agave family at 4.3.x) |
| 12 | `SanitizedMessage: SVMStaticMessage` not satisfied | `solana-fee` stayed at 4.4.0 (outside that exact pin) and pulled `solana-svm-transaction` 6.0 next to 5.0 | `cargo update -p solana-fee --precise 4.3.2` |
| 13 | Tests: `no function try_new found for VersionedTransaction` | Tests used `solana-message`/`solana-transaction` 3.x; LiteSVM uses 4.x types | Bumped to `solana-message 4.2.4`, `solana-transaction 4.1.5` + `verify` |
| 14 | `solana-test-validator` 4.1.2: v1 client gets `Transaction did not pass signature verification` | The 4.1.2 validator predates the final v1 format | Test locally with the 4.4.0 validator (section 9). devnet/mainnet nodes are already ≥ 4.3.2 |
| 15 | `solana confirm -v <sig>`: `Transaction version (1) is not supported by the requesting client` | CLI 4.1.2 can't read v1 transactions | CLI 4.4.0, or RPC `getTransaction` with `maxSupportedTransactionVersion: 1` |
| 16 | `solana feature status \| grep -i sbpf` doesn't show the v3 gate, and SIMD-0385 shows as inactive | CLI 4.1.2's full list omits/mislabels these. Its v1 entry is an old ID | Query by ID: `solana feature status -u devnet <ID>`, or `getAccountInfo` (section 13, step 8) |
| 17 | A plain `cargo build-sbf` produces a v0 `.so` (`0000 0000`) | `cargo build-sbf` 4.1.0 defaults to `--arch v0` (4.5.0 defaults to v3) | Always pass `--arch v3`. `anchor build` already does |

Commands used to diagnose version problems:

```sh
xxd -s 0x30 -l 4 target/deploy/anchor_vault.so   # SBPF version of a build
cargo info <crate>@<version>                     # rust-version (MSRV) of a crate version
cargo tree -i <crate>@<version> --target all     # who pulls in a given version
grep -A1 'dependencies.<dep>]' ~/.cargo/registry/src/*/<crate>-<ver>/Cargo.toml  # exact requirement
cargo update -p <crate> --precise <version>      # pin one crate in Cargo.lock
```

## 11. Lessons learned

1. **The compiler and every loader must agree on the SBPF version.** The program code doesn't
   change between v2 and v3; the `.so` does. LiteSVM, the validator, Surfpool and Mollusk each
   support a maximum version. A mismatch shows up as a vague `InvalidAccountData`, not as a
   "wrong version" message.
2. **Build and deploy are separate steps.** `anchor deploy` / `anchor program deploy` never
   compile. Whatever ran last (`anchor build`, `anchor test` and its script, `cargo build-sbf`)
   decides what gets uploaded. Check the version byte before deploying.
3. **There are two Rust compilers.** The host rustc (`rust-toolchain.toml`) builds tests and the
   client. The platform-tools rustc (1.95.0-dev for v1.57) builds the program. The workspace
   `rust-version` applies to both, so it must stay at or below the platform-tools version.
4. **Agave crates don't follow semver between each other.** They pin each other with `=x.y.z` and
   change internal APIs in minor releases. A library that says `^4.3.0` can break on 4.4.0. Hold
   the family with `~4.3` and pin stragglers (`solana-fee`) with `cargo update --precise`.
5. **One workspace = one `Cargo.lock`.** The tests' LiteSVM and the client's RPC crates had to
   agree on shared dependencies even though they never run together.
6. **Types must come from the same crate version.** Two versions of `solana-transaction` in the
   graph means two different `VersionedTransaction` types; pass one where the other is expected
   and it won't compile. Match your dev-dependencies to the library you hand values to.
7. **Read the dependency's own `Cargo.toml`** in `~/.cargo/registry/src/` to see exact
   requirements. It's faster than guessing from error messages.
8. **Check feature gates before using a new format on a live cluster** (SBPF v3, transaction v1).
   Devnet can be ahead of or behind mainnet, and the CLI's feature list may not know new features.
9. **v1 transactions budget zero by default.** Always set the CU limit and the loaded-accounts
   data size (which must cover large accounts such as ProgramData).
10. **Don't hard-code RPC API keys.** The Helius key is still a default in `client/src/main.rs`;
    move it to `RPC_URL` before committing.

## 12. Crate reference

Every crate in the `Cargo.toml` files also has a comment next to it explaining its use.

**`programs/anchor-vault/Cargo.toml`**

| Crate | Section | Used for |
|---|---|---|
| `anchor-lang 1.2.1` (lockfile: 1.2.2) | dependencies | The program framework: `#[program]`, `#[derive(Accounts)]`, `#[account]`, constraints, errors, system-program CPI. `"1.2.1"` means `^1.2.1`, so `Cargo.lock` resolved 1.2.2, the same as the installed CLI. Write `"1.2.2"` to make that explicit |
| `litesvm 0.18.1` | dev | In-process Solana VM that loads the `.so` and runs the test transactions |
| `solana-program-runtime ~4.3` | dev | Not used directly; pins the Agave family at 4.3.x so litesvm 0.18.1 compiles |
| `solana-message 4.2.4` | dev | `Message` / `VersionedMessage` for test transactions |
| `solana-transaction 4.1.5` (`verify`) | dev | `VersionedTransaction`, the signed transaction passed to `svm.send_transaction` |
| `solana-signer 3.0.0` | dev | `Signer` trait (`.pubkey()`, signing) |
| `solana-keypair 3.0.1` | dev | `Keypair` for test wallets |

**`client/Cargo.toml`**

| Crate | Used for |
|---|---|
| `anchor-client 1.2.1` (commented out) | Old high-level client; only builds legacy/v0 transactions |
| `anchor-lang 1.2.1` | `InstructionData`, `ToAccountMetas`, `AccountDeserialize`, `Pubkey`, `Instruction` |
| `anchor-vault` (path, `no-entrypoint`) | Program ID, instruction/accounts structs, `COUNTER_SEED`, `MAX_COUNT`, `Counter` |
| `solana-message 4.1` | `v1::Message`, `v1::TransactionConfig`, `VersionedMessage::V1` |
| `solana-transaction 4.1` | `VersionedTransaction` (signing) |
| `solana-rpc-client ~4.3` | Blocking `RpcClient`: blockhash, send, read accounts and transactions |
| `solana-rpc-client-api ~4.3` | `RpcTransactionConfig`, `CommitmentConfig`, `UiTransactionEncoding` |
| `solana-keypair 3.0.1` | `read_keypair_file`, `Keypair`, `Signer` |
| `anyhow 1` | Error handling in `main` |

**Other config files**

| File | Key setting |
|---|---|
| `rust-toolchain.toml` | Host rustc `1.99.1` (needed by litesvm 0.18) |
| `Cargo.toml` (root) | Workspace members `programs/*` and `client`; `rust-version = "1.89.0"` (must not exceed platform-tools rustc) |
| `Anchor.toml` | `test = "cargo test"`; `cluster = "devnet"` (use `--skip-deploy` for local testing) |
| `Cargo.lock` | `solana-fee = 4.3.2` pinned by hand (see issue 12) |

## 13. Debug version problems yourself

Run these from the project root. Each one was checked on this repo.

### Step 1: Which compilers are in play?

```sh
rustup show active-toolchain        # host rustc (tests + client), set by rust-toolchain.toml
~/.cache/solana/v1.57/platform-tools/rust/bin/rustc --version   # program rustc (1.95.0-dev)
ls ~/.cache/solana/                 # platform-tools versions already installed
anchor --version; solana --version; cargo build-sbf --version
```

If you see `rustc X is not supported ... requires rustc Y`, compare Y with both compilers above:
- The host rustc is too old: raise `channel` in `rust-toolchain.toml`.
- The message says `1.95.0-dev`: the program compiler is too old for the workspace `rust-version`.
  Lower `rust-version` in the root `Cargo.toml`. Don't raise it above the platform-tools rustc.

### Step 2: Which SBPF version is a binary?

```sh
xxd -s 0x30 -l 4 target/deploy/anchor_vault.so          # local build
solana program dump <PROGRAM_ID> /tmp/p.so -u devnet    # deployed build
xxd -s 0x30 -l 4 /tmp/p.so
# 0000 0000 = v0, 0100 0000 = v1, 0200 0000 = v2, 0300 0000 = v3
```

Build a specific version: `cargo build-sbf --tools-version v1.57 --arch v2` (or `v3`).
If LiteSVM fails with `InvalidAccountData` on `add_program`, it can't load that version: build
an older version or upgrade LiteSVM.

### Step 3: What is the minimum rustc (MSRV) of a crate version?

```sh
cargo info litesvm@0.18.1 | grep rust-version
cargo info solana-rpc-client@4.2.0 | grep rust-version
# List all published versions (crates.io API), e.g. to find the last 4.3.x:
curl -s -A "debug" https://crates.io/api/v1/crates/solana-program-runtime/versions \
  | python3 -c "import json,sys;print([v['num'] for v in json.load(sys.stdin)['versions']][:15])"
```

Loop over versions to find the newest one that fits your rustc:
```sh
for v in 0.14.0 0.15.0 0.16.0 0.17.0 0.18.1; do echo "$v $(cargo info litesvm@$v | grep rust-version)"; done
```

### Step 4: What does a crate require exactly?

`cargo info` doesn't always show dependency ranges. The crate's own `Cargo.toml` does (after
the crate has been downloaded once by any build or `cargo update`):

```sh
R=~/.cargo/registry/src/index.crates.io-*
grep -A1 '^\[dependencies.solana-program-runtime\]' $R/litesvm-0.18.1/Cargo.toml
grep -A1 '^\[dependencies.solana-message\]'         $R/litesvm-0.18.1/Cargo.toml
```

Note the shape of the requirement:
- `"4.3.0"` means `^4.3.0`, i.e. any 4.x ≥ 4.3.0. This is where Agave 4.4 slipped in (issue 11).
- `"~5.0.0"` means 5.0.x only.
- `"=4.4.0"` means exactly that version. Agave crates pin each other like this, which is why they
  must all move together.

### Step 5: Which versions did cargo actually pick, and why?

```sh
cargo tree -p anchor-vault -e dev --depth 1     # direct dev-dependencies and their resolved versions
cargo tree -p anchor-vault-client --depth 1     # same for the client
cargo tree -i solana-svm-transaction@6.0.0 --target all   # who pulls in this version
cargo tree -d -e normal,dev                     # every crate present in more than one version
grep -A1 'name = "solana-fee"$' Cargo.lock      # versions recorded in the lockfile
```

Two versions of a crate are fine *unless* values cross between them. Errors like
`expected X, found X` or `note: there are multiple different versions of crate ...` mean a type
from one version is being passed where the other is expected. Use `cargo tree -i` to find who
pulls in each version, then align them.

### Step 6: Try a version change safely

```sh
cp Cargo.lock /tmp/Cargo.lock.bak                       # backup first
cargo update -p solana-fee --precise 4.3.2 --dry-run    # preview what would change
cargo update -p solana-fee --precise 4.3.2              # pin one crate in Cargo.lock
cargo update -p litesvm                                 # re-resolve one crate after editing Cargo.toml
cargo build --tests --workspace                         # compile tests + client without running anything
cp /tmp/Cargo.lock.bak Cargo.lock                       # undo if it got worse
```

Avoid a bare `cargo update` while debugging. It moves every crate at once, including the
`solana-fee 4.3.2` pin, so you can't tell which change broke the build.

If `--precise` fails with `failed to select a version for the requirement X = "=4.4.0"`, the crate
is exact-pinned by a sibling. Constrain the family instead, e.g. `solana-program-runtime = "~4.3"`
in `Cargo.toml`, then `cargo update -p solana-program-runtime`.

### Step 7: Read the error carefully

| Error text | Meaning | Where to look |
|---|---|---|
| `requires rustc X` | MSRV too high for a compiler | Step 1, Step 3 |
| `failed to select a version ... versions that meet the requirements` | Two requirements on the same crate don't overlap | Step 4 on both crates named in the message |
| `no function ... found`, `takes 1 argument but 2 were supplied` inside a dependency | A dependency resolved to an API-incompatible version | Step 4 (what it asked for) and Step 5 (what it got) |
| `multiple different versions of crate` | Types from two versions mixed | Step 5 `cargo tree -i` / `-d` |
| `InvalidAccountData` from `add_program` / deploy | SBPF version not supported by the loader or cluster | Step 2, Step 8 |
| `Failed to install platform-tools` | That platform-tools version isn't downloaded | `ls ~/.cache/solana/`, pass `--tools-version` |

### Step 8: Is a feature active on the cluster?

```sh
solana feature status -u devnet 5cC3foj77CWun58pC51ebHFUWavHWKarWyR5UUik7dnC   # query by ID; the grep'd list misses gates
# Features unknown to your CLI version: read the feature account directly.
# "value": null (account missing) = inactive; data present = activated.
curl -s https://api.devnet.solana.com -X POST -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"getAccountInfo","params":["txv1aq4pp281K9um3tnPgkfX8UqtFT6wcVW3hNezGLL",{"encoding":"base64"}]}'
curl -s https://api.devnet.solana.com -X POST -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"getVersion"}'           # cluster software version
```

### Checklist

1. Find the first `error` line, not the last.
2. Name the crate and the version in the error.
3. `cargo info` (MSRV) → registry `Cargo.toml` (requirements) → `cargo tree -i` (who pulls it in).
4. Change one thing: one `Cargo.toml` line or one `--precise` pin.
5. `cargo build --tests --workspace`, then `anchor test --skip-deploy`.
6. Check the SBPF byte of the `.so` you test and the one you deploy.

## 14. Upgrading Agave 4.1.2 → 4.4.0 (verified 2026-11-02)

How it was tested: a copy of this project and of `real-vault`, with the official Agave 4.4.0
release first on `PATH` (`solana`, `cargo build-sbf` 4.5.0, `solana-test-validator`), using
anchor-cli 1.2.1. A fresh Anchor 1.2.2 scaffold was also verified on 4.4.0
(`../real-vault/steps.md`).

| Step | anchor-vault | real-vault |
|---|---|---|
| `anchor build` | ✅ v3, IDL generated | ✅ v3 |
| `anchor test --skip-deploy` | ✅ 4/4 | ✅ 1/1 |
| `anchor deploy --provider.cluster localnet` (4.4.0 validator) | ✅ | ✅ |
| v1 client against the 4.4.0 validator | ✅ "all checks passed" | – |
| `.so` SHA-256 vs the 4.1.2 build | identical | identical |

The `.so` doesn't change because `anchor build` passes `--tools-version v1.57 --arch v3`
itself. The devnet deployment doesn't need redeploying.

```sh
agave-install init 4.4.0     # upgrade (`agave-install update` won't cross minor versions)
agave-install init 4.1.2     # roll back
```

After upgrading:
- `avm solana install` with **no version** switches back to 4.1.2, because avm maps Anchor
  1.2.x → Solana 4.1.2. Pass `4.4.0` explicitly.
- A bare `cargo build-sbf` now builds v3 instead of v0.
- New: the local validator accepts v1, `solana confirm -v` reads v1, and `solana feature status`
  recognizes `txv1aq4pp...`.

## 15. Related docs in this repo

| File | Use it for |
|---|---|
| `../real-vault/steps.md` | **Start a new Anchor project** from scratch (exact commands, verified) |
| `../pinocchio.md` | Start a Pinocchio project (SBPF v3 + v1, three-workspace layout) |
| `../commands.md` | Upgrade, downgrade, pin or delete Anchor / Agave / platform-tools / Rust / crates |
| `../deps.md` | Method for solving dependency errors + what works with what |
| `../simd.md` | Reading SIMDs, finding feature-gate IDs, checking activation |
