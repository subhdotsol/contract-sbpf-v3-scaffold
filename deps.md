# Solving Solana dependency problems: method + what works with what

How the dependency problems in `real.md`, `pinocchio.md` and `commands.md` were actually
solved. Every case below happened in this repo; the commands are the ones that were run.

## 1. Four kinds of "doesn't support this version"

Most confusion comes from mixing these up. Each has its own check.

| Kind | Typical error | What is incompatible | Check with |
|---|---|---|---|
| **A. Version range** | `failed to select a version for X` | Two crates ask for non-overlapping versions of the same crate | The dependency's own `Cargo.toml` in the registry |
| **B. API changed inside an allowed range** | `no function ... found`, `missing field`, `takes 1 argument but 2 were supplied` | Cargo picked a version the range allows, but its API differs | `cargo tree` (what was picked) + reading the crate source |
| **C. Compiler too old (MSRV)** | `rustc X is not supported ... requires rustc Y` | The crate's `rust-version` is higher than the compiler | `cargo info crate@ver`, `rustc --version`, platform-tools rustc |
| **D. Runtime / format** | `InvalidAccountData` on load, `signature verification` failure | The binary or transaction format is newer than the program loading it (LiteSVM, validator) | SBPF byte (`xxd`), validator version, feature gates (`simd.md`) |

A and B are about crates in `Cargo.lock`. C is about compilers. D happens after everything
compiles.

## 2. The toolbox: one question → one command

| Question | Command |
|---|---|
| Which versions exist? | `curl -s -A x https://crates.io/api/v1/crates/<crate>/versions \| python3 -c "import json,sys;print([v['num'] for v in json.load(sys.stdin)['versions'] if not v['yanked']][:15])"` |
| What is the latest version, and its MSRV? | `cargo info <crate>` / `cargo info <crate>@<ver>` |
| What exactly does crate X require? | `grep -A2 '^\[dependencies.<dep>\]' ~/.cargo/registry/src/*/<crate>-<ver>/Cargo.toml` |
| Which versions did my project get? | `cargo tree --depth 1` (add `-e dev` for dev-dependencies) |
| Which versions does library X use? | `cargo tree --depth 1 -p <crate>` |
| Who pulls in this version? | `cargo tree -i <crate>@<ver> --target all` |
| Are there duplicates? | `cargo tree -d` |
| Does a function exist / behind which feature? | `grep -rn "pub fn <name>" ~/.cargo/registry/src/*/<crate>-<ver>/src` and read the `#[cfg(...)]` above it |
| Try a change safely | `cp Cargo.lock /tmp/bak` → `cargo update -p <c> --precise <v> --dry-run` → apply → `cargo build --tests` → restore if worse |

The registry source (`~/.cargo/registry/src/index.crates.io-*/<crate>-<ver>/`) appears after
any build, `cargo fetch` or `cargo generate-lockfile` downloads the crate. Its `Cargo.toml` is
the normalized manifest. Every dependency there is written as
`[dependencies.<name>]` / `version = "..."`, which makes it easy to grep.

### The probe-crate trick

To learn which versions work together **before** touching your project, make a throwaway crate
and let Cargo resolve:

```sh
cargo new --lib /tmp/probe && cd /tmp/probe
cargo add solana-rpc-client@4.4.0 solana-rpc-client-api@4.4.0
cargo generate-lockfile
cargo tree --depth 1 -p solana-rpc-client    # → solana-message 5.1.0, solana-transaction 5.1.0,
                                             #   solana-instruction 4.0.0, solana-pubkey 4.4.0 ...
```

The pinocchio client's `Cargo.toml` versions (`solana-message = "5.1"`, etc.) came from exactly
this. If two crates can't coexist, `cargo add` / `generate-lockfile` fails in the probe, not in
your project.

## 3. Case studies from this repo

### 3.1 pinocchio 0.12 + litesvm 0.18.1 → kind A

Symptom (`cargo add pinocchio@0.12.0 ... ; cargo add --dev litesvm@0.18.1` in a probe):

```
failed to select a version for `solana-address`.
    ... required by package `solana-account-view v3.0.0`
    ... which satisfies dependency `solana-account-view = "^3.0"` of package `pinocchio v0.12.0`
versions that meet the requirements `^2.8.0` are: 2.9.0, 2.8.0
  previously selected package `solana-address v2.7.0`
    ... which satisfies dependency `solana-address = "~2.7.0"` of package `litesvm v0.18.1`
```

How to read it: the chain shows both sides. pinocchio → solana-account-view 3.0 wants
`solana-address ^2.8`, and litesvm wants `~2.7` (2.7.x only). Confirmed in the registry:

```sh
grep -A1 '^\[dependencies.solana-address\]' ~/.cargo/registry/src/*/solana-account-view-3.0.0/Cargo.toml  # 2.8.0
grep -A1 '^\[dependencies.solana-address\]' ~/.cargo/registry/src/*/litesvm-0.18.1/Cargo.toml             # ~2.7.0
```

Why it can't be forced: Cargo allows several versions of a crate in one lockfile only if they
are **semver-incompatible** (e.g. 1.x and 2.x). 2.7 and 2.9 are "compatible", so Cargo must pick
one, and no single version satisfies both requirements.

Options considered:
1. pinocchio 0.11.2 (older) + litesvm in one workspace.
2. pinocchio 0.12 + mollusk-svm 0.16.4 (doesn't pin `solana-address`, but can't send real
   transactions, so no v1 testing).
3. **Chosen:** separate workspace for the tests. The tests only need the `.so` bytes, not the
   program's Rust types, so nothing has to share a lockfile.

### 3.2 litesvm 0.18.1 + Agave 4.4 → kind B

Symptom: litesvm itself fails to compile (`new_builtin takes 1 argument`, `Arc<SlotHashes>`
mismatch). litesvm asks for `solana-program-runtime = "4.3.0"`, which means `^4.3.0`. Cargo
picked 4.4.0, whose API changed. Agave crates pin each other with `=` (e.g.
`solana-svm = "=4.4.0"`), so they move as a family.

Fix: `solana-program-runtime = "~4.3"` in your own `Cargo.toml`. That holds the whole family at
4.3.x.

### 3.3 `solana-fee` outside the family → kind B

Symptom: `SanitizedMessage: SVMStaticMessage is not satisfied`. `solana-fee` isn't exact-pinned
by the 4.3 family, so it stayed at 4.4.0 and pulled `solana-svm-transaction` 6.0 next to 5.0.

Fix: `cargo update -p solana-fee --precise 4.3.2`. Re-verified for the pinocchio tests by
removing the pin: the same error came back.

### 3.4 Type mismatch between your code and a library → kind A/B

Symptom: `no function try_new found for VersionedTransaction` or `expected X, found X`. Your
dev-dependency `solana-transaction 3.x` and litesvm's `4.x` are **two different types** with
the same name.

Fix: read which versions the library uses and match them:

```sh
grep -E -A1 '^\[dependencies\.solana-(message|transaction|keypair|signer|address|instruction)\]' \
  ~/.cargo/registry/src/*/litesvm-0.18.1/Cargo.toml
```

### 3.5 rpc-client-api 4.4 struct change → kind B

Symptom: `missing field min_context_slot in initializer of RpcTransactionConfig`. The newer
version added a field. Fix: add `min_context_slot: None`. Rule: a compile error **in your code**
after a bump means read the new struct or function signature. A compile error **inside a
dependency** means a version-range problem (3.2).

### 3.6 Function exists on-chain but not on the host → feature/cfg

Symptom: `no associated function find_program_address for Address` on `cargo check`.
Found by grepping the source:

```sh
grep -rn -B3 "pub fn find_program_address" ~/.cargo/registry/src/*/solana-address-2.9.0/src/
#   #[cfg(any(target_os = "solana", target_arch = "bpf", feature = "curve25519"))]
```

Fix: enable `curve25519` only for host builds
(`[target.'cfg(not(target_os = "solana"))'.dependencies]`).

### 3.7 MSRV: two compilers → kind C

- litesvm 0.18.1: `cargo info litesvm@0.18.1` → `rust-version: 1.97.1`. Agave 4.x crates need
  1.98 → host rustc 1.99.1 via `rust-toolchain.toml`.
- pinocchio 0.12: `rust-version: 1.89.0`. The program is compiled by platform-tools rustc
  `1.95.0-dev`.
- Workspace `rust-version = "1.99.1"` broke `cargo build-sbf` (`rustc 1.95.0-dev is not
  supported`), because `rust-version` applies to the program too. Keep it ≤ 1.95.

Find the newest version that fits a compiler:

```sh
for v in 0.15.0 0.16.0 0.17.0 0.18.1; do echo "$v $(cargo info litesvm@$v | grep rust-version)"; done
```

### 3.8 Runtime / format → kind D

| Symptom | Cause | How found |
|---|---|---|
| `InvalidAccountData` from `svm.add_program` | litesvm 0.10 can load only up to SBPF v2; Anchor builds v3 | `xxd -s 0x30 -l 4 x.so` → `0300 0000` |
| v1 tx → `Transaction did not pass signature verification` on local validator | `solana-test-validator` 4.1.2 predates the final v1 format | Same client passed on 4.4.0 |
| `solana confirm` → `Transaction version (1) is not supported` | CLI 4.1.2 can't read v1 transactions | 4.4.0 CLI read the same signature |
| `.so` is v0 | `cargo build-sbf` 4.1.0 defaults to `--arch v0` | `cargo build-sbf --help` → `[default: v0]` |

These never show up in `Cargo.lock`. Check the binary, the program that loads it, and the cluster.

## 4. What works with what (verified 2026-11-02)

### Program side (compiled by platform-tools)

| Crate | Version | Needs | rust-version |
|---|---|---|---|
| pinocchio | 0.12.0 | solana-account-view 3.0 → **solana-address ≥ 2.8** | 1.89.0 |
| pinocchio | 0.11.2 | solana-account-view 2.0 → solana-address ≥ 2.2 (so it can share a lockfile with litesvm's `~2.7`) | – |
| pinocchio-system | 0.7.0 | pinocchio 0.12 (`cpi`) | – |
| pinocchio-system | 0.6.1 | pinocchio 0.11 | – |
| solana-program-log | 1.2.0 | no_std, works with 0.12 | – |
| anchor-lang | 1.2.1 / 1.2.2 | anchor-cli of the same version | – |
| platform-tools | v1.57 / v1.58 | rustc 1.95.0-dev → keep `rust-version` ≤ 1.95 | – |

### Test side (host rustc)

| Crate | Version | Needs |
|---|---|---|
| litesvm | 0.18.1 | Agave **4.3.x** (`solana-program-runtime ~4.3`, `solana-fee =4.3.2`), solana-address **~2.7**, solana-message 4.2.4+, solana-transaction 4.1.5+, solana-keypair 3.1.2, solana-signer 3.0.1, solana-instruction ~3.5; rustc ≥ 1.97.1 (Agave crates ≥ 1.98) |
| litesvm | 0.15.0 | Default in the Anchor 1.2.2 scaffold. Agave 4.1.2 crates, solana-message 4.3, rustc 1.89 (scaffold `Cargo.lock`). Loads SBPF v3 and runs v1 transactions (verified) |
| litesvm | 0.10.0 | Default in the Anchor 1.2.1 scaffold. Solana 3.x crates; loads SBPF ≤ v2 only |
| mollusk-svm | 0.16.4 | requires Agave `^4.3` and doesn't pin solana-address; not compile-tested here |

### Client side (host rustc)

| Crate | Version | Matching crates |
|---|---|---|
| solana-rpc-client(-api) | 4.4.0 | solana-message 5.1, solana-transaction 5.1, solana-instruction 4.0, solana-pubkey 4.4, solana-keypair 4.0, solana-signer 4.0, solana-commitment-config 3.1 |
| solana-rpc-client(-api) | 4.3.x | solana-message / solana-transaction 4.x (anchor-vault client) |
| solana-rpc-client | 4.2.x | requires solana-transaction < 4.2 |

### Tools

| Tool | Version | Behavior |
|---|---|---|
| Agave CLI | 4.1.2 | Builds v3 only with `--arch v3`. Local validator and `solana confirm` can't handle final v1 |
| Agave CLI | 4.4.0 | `cargo build-sbf` 4.5.0 (default v3, platform-tools v1.57). v1 works end to end |
| anchor-cli | 1.2.1 / 1.2.2 | avm maps both to Solana 4.1.2 → platform-tools v1.57 |

These go stale. Re-derive them with the toolbox (section 2) instead of trusting old tables,
this one included.

## 5. Procedure for a new dependency error

1. Find the **first** `error` line, not the last.
2. Classify it: A, B, C or D (section 1).
3. Name the crate and the version in the message.
4. A: grep both crates' registry `Cargo.toml` for the shared dependency. If the ranges don't
   overlap, change one side, or split workspaces if the types don't need to cross.
5. B: `cargo tree -i crate@ver` shows who wanted it. Pin with `~x.y` on the family or
   `--precise` on the straggler.
6. C: `cargo info crate@ver | grep rust-version` vs the compiler that builds that crate.
7. D: check the SBPF byte, the loader/validator version and the cluster feature gates.
8. Change **one** thing, run `cargo build --tests`, keep or revert (`cp /tmp/Cargo.lock.bak Cargo.lock`).
9. Avoid a bare `cargo update` while debugging. It moves everything, including your pins.
