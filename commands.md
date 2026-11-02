# Version management commands: Anchor, Agave/Solana, platform-tools, Rust, Cargo, Pinocchio

Checked on 2026-11-02 against the installed tools: avm 1.2.2, anchor-cli 1.2.1, agave-install
4.1.2, cargo-build-sbf 4.1.0, rustup 1.29.2, cargo 1.99.1. Read-only commands were run. Commands
that install or remove things were checked against each tool's `--help`, not run.

## 0. What am I on right now?

```sh
anchor --version                     # anchor-cli 1.2.1
avm list                             # all Anchor versions; marks (installed) / (current) / (latest)
solana --version                     # solana-cli 4.1.2 (Agave)
agave-install info                   # active Solana release + download URL
cargo build-sbf --version            # cargo-build-sbf 4.1.0 + default platform-tools
ls ~/.cache/solana/                  # platform-tools versions downloaded (v1.57, v1.58, ...)
rustup show active-toolchain         # host rustc used in THIS directory, and why
rustc --version; cargo --version
rustup toolchain list                # every installed Rust toolchain (incl. *-sbpf-solana-*)
rustup override list                 # directories with a pinned toolchain
cargo tree --depth 1                 # direct dependency versions of this project
```

## 1. Which tool manages what

| Thing | Manager | Global switch | Per-project pin |
|---|---|---|---|
| Anchor CLI | `avm` | `avm use <ver>` | `Anchor.toml` → `[toolchain] anchor_version` |
| `anchor-lang` crate | Cargo | – | `programs/*/Cargo.toml` (keep = CLI version) |
| Solana / Agave CLI, `solana-test-validator`, `cargo build-sbf` | `agave-install` (or `avm solana`) | `agave-install init <ver>` | `Anchor.toml` → `[toolchain] solana_version` |
| platform-tools (rustc that compiles the program) | `cargo build-sbf` (or `avm platform-tools`) | the default comes with the Solana CLI | `--tools-version vX.YY` in build commands/scripts |
| SBPF version of the `.so` | `cargo build-sbf` | the default comes with the Solana CLI (4.1.0: v0, 4.5.0: v3) | `--arch v3` in build commands/scripts |
| Host Rust (tests, clients, scripts) | `rustup` | `rustup default <ver>` | `rust-toolchain.toml` (or `rustup override set`) |
| Minimum Rust a crate claims | Cargo | – | `rust-version` in `Cargo.toml` |
| Library crates (pinocchio, litesvm, solana-*) | Cargo | – | `Cargo.toml` requirement + `Cargo.lock` |

## 2. Anchor (avm)

```sh
avm list                         # available versions (add --pre-release for rc versions)
avm install 1.2.2                # install a version (prebuilt binary)
avm install latest
avm install 1.2.2 --from-source  # build from source instead of downloading
avm install <git-commit>         # a specific commit
avm use 1.2.2                    # switch the global `anchor`
avm use latest
avm update                       # install + use the newest release
avm uninstall 1.0.1              # remove an old version
avm self-update                  # update avm itself
avm nightly / avm nightly --disable
```

Installed binaries live in `~/.avm/bin/anchor-<ver>`. The active one is in `~/.avm/.version`.

**Downgrade:** `avm install 0.32.1 && avm use 0.32.1`.

**Per project** (`Anchor.toml`). With these keys set, `anchor` runs the pinned version for this
project, whatever is globally active:

```toml
[toolchain]
anchor_version = "1.2.1"
solana_version = "4.1.2"
package_manager = "yarn"   # optional
```

Keep the CLI and the crate in step, in `programs/<name>/Cargo.toml`:

```toml
anchor-lang = "1.2.1"
```

```sh
cargo update -p anchor-lang --precise 1.2.1    # move the lockfile to exactly that version
```

**Solana / platform-tools that Anchor recommends:**

```sh
avm solana resolve                              # Solana version this project should use
avm solana install                              # install + activate that version
avm solana install 4.4.0                        # or a specific one
avm platform-tools resolve                      # platform-tools for this project
avm platform-tools resolve --anchor-version 1.2.2   # → Solana 4.1.2 → v1.57 (checked)
avm platform-tools resolve --solana-version 4.4.0   # → v1.57 (checked)
avm platform-tools install [v1.58]
avm platform-tools list / uninstall <ver>
```

Anchor 1.2.2 (latest) still maps to Solana 4.1.2. In anchor-cli 1.2.1, `anchor build` passes
these flags through to `cargo build-sbf`: `--tools-version` (default `v1.57`) and `--arch`
(default `v3`). Example: `anchor build --tools-version v1.58 --arch v3`. `anchor test` has no
`--arch`. To control the build `anchor test` uses, run `cargo build-sbf` from `[scripts]`
(anchor-vault section 3).

## 3. Agave / Solana CLI (agave-install)

```sh
agave-install info               # active version
agave-install list               # versions installed locally
agave-install init 4.4.0         # install AND switch to 4.4.0 (upgrade or downgrade)
agave-install init 4.1.2         # switch back (re-uses the cached release if gc hasn't removed it)
agave-install init stable        # channels: stable | beta | edge
agave-install update             # updates only within the "compatible" line
agave-install gc                 # keep the 5 most recently modified releases, delete the rest
```

- **`gc` keeps 5 releases, not just the current one.** From `agave-install` 4.1.2 source:
  `MAX_CACHE_LEN = 5`, sorted by directory **modification time** (not version number), and the
  active release gets no special treatment. `init` and `update` run `gc` automatically after
  installing, so the 6th install removes the least recently modified release.
- Keep only the active release (manual; check what `active_release` points to first):
  ```sh
  readlink ~/.local/share/solana/install/active_release   # e.g. .../releases/4.4.0/solana-release
  ls ~/.local/share/solana/install/releases/
  rm -rf ~/.local/share/solana/install/releases/{2.3.0,3.1.9,3.1.10}   # name the ones to drop
  ```

- **`agave-install update` does not move to a new minor version.** On 4.1.2 it reports
  `Install is up to date. 4.1.2 is the latest compatible release` even though 4.4.0 exists. Use
  `agave-install init 4.4.0`.
- Fresh machine: `sh -c "$(curl -sSfL https://release.anza.xyz/v4.4.0/install)"`.
- List published releases:
  ```sh
  curl -s "https://api.github.com/repos/anza-xyz/agave/releases?per_page=10" \
    | python3 -c "import json,sys;[print(r['tag_name'],r['published_at'][:10],'pre' if r['prerelease'] else '') for r in json.load(sys.stdin)]"
  ```

**Use one version for one command without switching globally:**

```sh
~/.local/share/solana/install/releases/4.1.2/solana-release/bin/solana --version
# or a downloaded tarball, unpacked anywhere:
curl -sSfL -o /tmp/agave.tar.bz2 \
  https://github.com/anza-xyz/agave/releases/download/v4.4.0/solana-release-aarch64-apple-darwin.tar.bz2
tar xjf /tmp/agave.tar.bz2 -C /tmp && /tmp/solana-release/bin/solana-test-validator --reset
PATH=/tmp/solana-release/bin:$PATH cargo build-sbf      # use its cargo-build-sbf for one build
```

**Per project:** `Anchor.toml` `[toolchain] solana_version = "..."` (Anchor projects). Without
Anchor, write the version in the README or a script, and pin what matters for builds with
`--tools-version` and `--arch` (section 4).

**What changes when you move the Solana CLI:** `solana`, `solana-test-validator`,
`cargo build-sbf` and its defaults (platform-tools version, `--arch`). For 4.1.2 vs 4.4.0, see
`pinocchio.md` section 1.

## 4. platform-tools (the compiler for on-chain programs)

```sh
cargo build-sbf --tools-version v1.58 --arch v3        # build with exactly these tools
cargo build-sbf --install-only --tools-version v1.58   # just download them
cargo build-sbf --install-only --tools-version v1.58 --force-tools-install   # re-download
ls ~/.cache/solana/                                     # installed versions
~/.cache/solana/v1.58/platform-tools/rust/bin/rustc --version   # rustc 1.95.0-dev
rustup toolchain list | grep sbpf                       # build-sbf registers e.g. 1.95.0-sbpf-solana-v1.57
```

- Downgrade: pass the older version, e.g. `--tools-version v1.57`.
- Remove one: `rm -rf ~/.cache/solana/v1.52` and
  `rustup toolchain uninstall <name-shown-by-list>`.
- List releases:
  ```sh
  curl -s "https://api.github.com/repos/anza-xyz/platform-tools/releases?per_page=5" \
    | python3 -c "import json,sys;[print(r['tag_name'],r['published_at'][:10]) for r in json.load(sys.stdin)]"
  ```
- Check the SBPF version of a build: `xxd -s 0x30 -l 4 target/deploy/<name>.so`
  (`0000 0000` = v0, `0200 0000` = v2, `0300 0000` = v3).
- After switching `--arch`, delete `target/deploy/<name>.so` (not the `-keypair.json`) before
  rebuilding. Otherwise the old binary can stay there.

## 5. Rust toolchain (rustup / rustc / cargo)

```sh
rustup show                          # installed toolchains + what is active here
rustup check                         # which toolchains have updates
rustup update                        # update all channel toolchains (stable, nightly, 1.88, ...)
rustup update stable
rustup self update                   # update rustup itself

rustup toolchain install 1.99.1      # exact version
rustup toolchain install stable      # moving channel
rustup toolchain install nightly-2026-10-01
rustup toolchain uninstall 1.79.0    # remove old ones
rustup default 1.99.1                # global default (currently 1.99.1 here, not `stable`)
rustup default stable                # follow the stable channel instead

cargo +1.89.0 build                  # one command with another toolchain
rustup run 1.89.0 cargo test         # same thing
rustup which rustc                   # which binary will run here

rustup component add clippy rustfmt rust-src
rustup target add wasm32-unknown-unknown
```

**Per project**, two options. `rust-toolchain.toml` (committed, applies to that directory and
everything below it):

```toml
[toolchain]
channel = "1.99.1"                 # exact version, a channel ("stable"), or "nightly-2026-10-01"
components = ["rustfmt", "clippy"]
profile = "minimal"
```

Or a local override (not committed; stored in rustup's settings):

```sh
rustup override set 1.89.0         # run inside the project directory
rustup override unset
rustup override list
rustup override unset --nonexistent    # clean overrides for deleted directories
```

Precedence, highest first: `cargo +ver` → `RUSTUP_TOOLCHAIN` env → `rustup override` →
`rust-toolchain.toml` → `rustup default`.

**Solana projects use two rustc versions.** `rust-toolchain.toml` sets the host rustc (tests,
clients). The program is compiled by the platform-tools rustc (1.95.0-dev for v1.57/v1.58).
`rust-version` in `Cargo.toml` applies to both, so keep it ≤ 1.95 (e.g. `"1.89.0"`).

## 6. Cargo dependencies (any crate)

### Look up versions

```sh
cargo info pinocchio                 # latest version, rust-version (MSRV), features
cargo info pinocchio@0.11.2          # a specific version
cargo search pinocchio --limit 5
cargo tree --depth 1                 # what this project resolved
cargo tree -i solana-address         # who pulls a crate in (add @2.7.0 if several versions)
cargo tree -d                        # crates present in more than one version
# every published version of a crate:
curl -s -A "versions" https://crates.io/api/v1/crates/pinocchio/versions \
  | python3 -c "import json,sys;print([v['num'] for v in json.load(sys.stdin)['versions'] if not v['yanked']][:15])"
```

### Change versions

```sh
cargo add pinocchio@0.12                         # add or change the requirement in Cargo.toml
cargo add pinocchio@0.12 --no-default-features -F cpi
cargo add --dev litesvm@0.18.1                   # dev-dependency
cargo remove pinocchio

cargo update                                     # move every crate to the newest allowed by Cargo.toml
cargo update -p solana-fee                       # move one crate
cargo update -p solana-fee --precise 4.3.2       # pin one crate to an exact version in Cargo.lock
cargo update -p solana-fee --precise 4.3.2 --dry-run   # preview first
cargo generate-lockfile                          # create Cargo.lock from scratch
```

| Requirement in `Cargo.toml` | Allows |
|---|---|
| `"0.12"` / `"^0.12"` | `>=0.12.0, <0.13.0` (for 0.x the minor is the breaking part) |
| `"4.3.0"` / `"^4.3.0"` | `>=4.3.0, <5.0.0`. **This is how Agave 4.4 slipped into litesvm builds** |
| `"~4.3"` | `>=4.3.0, <4.4.0` |
| `"=4.3.2"` | exactly 4.3.2 |

**Downgrade a crate:** lower the requirement (`cargo add crate@<old>`), then
`cargo update -p crate --precise <old>`. If `--precise` fails with
`failed to select a version ... "=x.y.z"`, a sibling crate pins it exactly. Constrain the
family instead (e.g. `solana-program-runtime = "~4.3"`).

**Make Cargo respect rust-version:** `resolver = "3"` in the workspace `Cargo.toml` (or
`edition = "2024"`) with `rust-version` set. Cargo then prefers dependency versions that build
on that Rust version.

### Binaries installed with cargo

```sh
cargo install --list
cargo install <crate> --version 1.2.3 --locked   # exact version; --locked uses its Cargo.lock
cargo install <crate> --locked --force           # reinstall / upgrade
cargo uninstall <crate>
```

## 7. Pinocchio

Pinocchio is a library crate, not a CLI, so it is versioned per project in `Cargo.toml`.

```sh
cargo info pinocchio                                  # latest: 0.12.0 (rust-version 1.89.0)
cargo add pinocchio@0.12 --no-default-features -F cpi
cargo add pinocchio-system@0.7 solana-program-log@1.2
```

Keep companion crates on the matching line:

| `pinocchio` | `pinocchio-system` | Notes |
|---|---|---|
| 0.12.x | 0.7.x | `cpi` feature is opt-in; needs `solana-address ^2.8` (clashes with litesvm 0.18.1, see `pinocchio.md`) |
| 0.11.x | 0.6.x | `&mut [AccountView]` entrypoint |

Downgrade 0.12 → 0.11:

```sh
cargo add pinocchio@0.11 pinocchio-system@0.6
cargo update -p pinocchio --precise 0.11.2
```

Then rebuild with `cargo build-sbf --tools-version v1.58 --arch v3` (see `pinocchio.md`).

## 8. Pin everything for one project

| File | Line | Pins |
|---|---|---|
| `rust-toolchain.toml` | `channel = "1.99.1"` | host rustc for tests/clients |
| `Cargo.toml` | `rust-version = "1.89.0"`, `resolver = "3"` | MSRV (≤ platform-tools rustc) and MSRV-aware resolution |
| `Cargo.toml` | `litesvm = "0.18.1"`, `solana-program-runtime = "~4.3"` | library versions / families |
| `Cargo.lock` | committed; `cargo update --precise` for exceptions | exact versions of everything |
| `Anchor.toml` | `[toolchain] anchor_version`, `solana_version` | Anchor CLI + Solana CLI |
| `Anchor.toml` `[scripts]`, Makefile or README | `cargo build-sbf --tools-version v1.58 --arch v3` | platform-tools + SBPF version |

## 8a. Delete one specific version

| Tool | Command | Notes |
|---|---|---|
| Agave / Solana CLI | `rm -rf ~/.local/share/solana/install/releases/<version>` | `agave-install` has no uninstall subcommand (only deploy, gc, info, init, list, run, update) |
| Anchor CLI | `avm uninstall <version>` | Removes `~/.avm/bin/anchor-<version>` |
| platform-tools | `rm -rf ~/.cache/solana/<vX.YY>` + `rustup toolchain uninstall <name>` | The name is the `*-sbpf-solana-<vX.YY>` entry in `rustup toolchain list` |
| Rust toolchain | `rustup toolchain uninstall <toolchain>` | e.g. `1.79.0`, `nightly-2026-05-22` |
| cargo-installed binary | `cargo uninstall <crate>` | |

Agave, step by step:

```sh
agave-install list                                        # see installed versions
readlink ~/.local/share/solana/install/active_release     # NEVER delete this one
rm -rf ~/.local/share/solana/install/releases/3.1.9       # one version
agave-install list                                        # confirm it is gone
```

Deleting the active release breaks `solana`, `cargo build-sbf` and the other tools (the
`active_release` link points into it). Switch first with `agave-install init <other>`, then
delete.

## 9. Recipes

**Upgrade the global tools to latest:**

```sh
avm update                         # Anchor (1.2.2)
agave-install init 4.4.0           # Solana/Agave; `update` won't cross minors
cargo build-sbf --install-only --tools-version v1.58
rustup update && rustup self update
avm self-update
```

**Bring an old project back to its versions without changing anything global:**

```sh
cd old-project
cat rust-toolchain.toml Anchor.toml 2>/dev/null   # see what it pins
avm install <anchor_version>                       # anchor auto-uses it via Anchor.toml
rustup toolchain install <channel from rust-toolchain.toml>
cargo build-sbf --tools-version <vX.YY> --arch <v2|v3>
# Solana CLI pinned by Anchor.toml? `avm solana install` (this switches the global CLI)
```

**Dependencies after a bump fail to compile:**

```sh
cp Cargo.lock /tmp/Cargo.lock.bak
cargo tree -d                                 # duplicates?
cargo tree -i <crate>@<ver>                   # who wants the bad version
cargo update -p <crate> --precise <ok-ver>    # change one thing
cargo build --tests
cp /tmp/Cargo.lock.bak Cargo.lock             # undo if worse
```
