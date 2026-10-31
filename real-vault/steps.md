# real-vault: SBPF v3 setup after `anchor init`

Toolchain: anchor-cli 1.2.1, solana-cli 4.1.2 (Agave), host rustc 1.99.1.
Based on the findings in `../anchor-vault/steps.md` (sections 7 and 10).

## 1. Create the project

```sh
cd contract-practice
anchor init real-vault
cd real-vault
```

## 2. `rust-toolchain.toml`: raise the host rustc

```diff
 [toolchain]
-channel = "1.89.0"
+channel = "1.99.1"
```

litesvm 0.18 needs Agave 4.x crates, which need rustc >= 1.98.

Do NOT change `rust-version = "1.89.0"` in the root `Cargo.toml`. The program is compiled by
platform-tools rustc (1.95.0-dev), and a higher value breaks `cargo build-sbf`.

## 3. `programs/real-vault/Cargo.toml`: update `[dev-dependencies]`

```diff
 [dev-dependencies]
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

- `litesvm 0.18.1` can load SBPF v3. `0.10.0` fails with `InvalidAccountData`.
- `solana-program-runtime = "~4.3"` keeps the Agave crates at 4.3.x. litesvm 0.18.1 does not
  compile with 4.4.
- The message and transaction versions must match litesvm's, or the test types don't line up.

## 4. Pin `solana-fee` in the lockfile

```sh
cargo generate-lockfile
cargo update -p solana-fee --precise 4.3.2
```

Without this, `solana-fee` stays at 4.4.0 and you get
`SanitizedMessage: SVMStaticMessage` not satisfied.

## 5. No change needed

- `Anchor.toml`: `[scripts] test = "cargo test"` is already correct. Don't add the v2 rebuild.
- `Cargo.toml` (root): unchanged.
- `programs/real-vault/src/**` and `tests/test_initialize.rs`: unchanged.

## 6. Run and verify

```sh
anchor test
xxd -s 0x30 -l 4 target/deploy/real_vault.so   # 0300 0000 = v3
```

Result: `test test_initialize ... ok` against a v3 binary.

## 7. Before deploying to devnet

```sh
solana feature status -u devnet | grep -i sbpf   # v3 feature must be active
anchor build                                       # deploy uploads whatever .so is in target/deploy
anchor deploy --provider.cluster devnet
```
