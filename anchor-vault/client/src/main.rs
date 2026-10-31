// =====================================================================================
// LEGACY CLIENT (anchor-client, legacy transaction format, 1232-byte limit)
// Kept for reference. Re-enable by uncommenting this block, commenting out the v1 block
// below, and restoring `anchor-client = "1.2.1"` in client/Cargo.toml.
// =====================================================================================
// //! Client that invokes the anchor_vault instructions against a live cluster
// //! and verifies the on-chain state after each step.
// //!
// //! Usage:
// //!   cargo run -p anchor-vault-client -- [RPC_URL] [KEYPAIR_PATH]
// //!
// //! RPC_URL defaults to the `RPC_URL` env var, falling back to the devnet Helius endpoint.
// //! KEYPAIR_PATH defaults to ~/.config/solana/id.json
//
// use std::rc::Rc;
//
// use anchor_client::{
//     anchor_lang::{prelude::Pubkey, system_program},
//     Client, ClientError, Cluster, CommitmentConfig, Program,
// };
// use anchor_vault::{constants::{COUNTER_SEED, MAX_COUNT}, state::Counter};
// use anyhow::{bail, ensure, Context, Result};
// use solana_keypair::{read_keypair_file, Keypair, Signer};
//
// const DEFAULT_RPC_URL: &str =
//     "https://devnet.helius-rpc.com/?api-key=8c79234f-3452-457b-96e3-171b70c0cfd4";
//
// fn main() -> Result<()> {
//     let mut args = std::env::args().skip(1);
//     let rpc_url = args
//         .next()
//         .or_else(|| std::env::var("RPC_URL").ok())
//         .unwrap_or_else(|| DEFAULT_RPC_URL.to_string());
//     let keypair_path = args.next().unwrap_or_else(|| {
//         format!("{}/.config/solana/id.json", std::env::var("HOME").unwrap_or_default())
//     });
//
//     let payer = Rc::new(
//         read_keypair_file(&keypair_path)
//             .map_err(|e| anyhow::anyhow!("failed to read keypair {keypair_path}: {e}"))?,
//     );
//     let ws_url = if let Some(rest) = rpc_url.strip_prefix("https://") {
//         format!("wss://{rest}")
//     } else if let Some(rest) = rpc_url.strip_prefix("http://") {
//         format!("ws://{}", rest.replace(":8899", ":8900"))
//     } else {
//         rpc_url.clone()
//     };
//     let cluster = Cluster::Custom(rpc_url.clone(), ws_url);
//     let client = Client::new_with_options(cluster, payer.clone(), CommitmentConfig::confirmed());
//     let program = client.program(anchor_vault::id())?;
//
//     let counter = Pubkey::find_program_address(&[COUNTER_SEED], &anchor_vault::id()).0;
//     println!("cluster:  {}", rpc_url.split('?').next().unwrap_or_default());
//     println!("program:  {}", anchor_vault::id());
//     println!("payer:    {}", payer.pubkey());
//     println!("counter:  {counter}\n");
//
//     // 1. initialize (only if the counter PDA does not exist yet; it is a singleton).
//     let before = match fetch_counter(&program, counter)? {
//         Some(state) => {
//             println!("[initialize] skipped, counter already exists (count = {})", state.count);
//             state
//         }
//         None => {
//             let sig = program
//                 .request()
//                 .accounts(anchor_vault::accounts::Initialize {
//                     payer: payer.pubkey(),
//                     counter,
//                     system_program: system_program::ID,
//                 })
//                 .args(anchor_vault::instruction::Initialize {})
//                 .send()
//                 .context("initialize failed")?;
//             println!("[initialize] tx: {sig}");
//
//             let state = fetch_counter(&program, counter)?.context("counter missing after init")?;
//             ensure!(state.count == 0, "expected count 0 after init, got {}", state.count);
//             ensure!(state.authority == payer.pubkey(), "authority mismatch after init");
//             println!("[initialize] ok: count = 0, authority = payer");
//             state
//         }
//     };
//
//     if before.authority != payer.pubkey() {
//         bail!(
//             "counter authority is {}, not this payer; increment would fail",
//             before.authority
//         );
//     }
//
//     // 2. increment by the authority.
//     if before.count >= MAX_COUNT {
//         println!("[increment] counter already at MAX_COUNT ({MAX_COUNT}); expecting overflow error");
//         let res = send_increment(&program, counter, &payer);
//         ensure!(res.is_err(), "increment past MAX_COUNT should have failed");
//         println!("[increment] ok: rejected with {}", short_err(&res.unwrap_err()));
//     } else {
//         let sig = send_increment(&program, counter, &payer).context("increment failed")?;
//         println!("[increment] tx: {sig}");
//
//         let after = fetch_counter(&program, counter)?.context("counter missing")?;
//         ensure!(
//             after.count == before.count + 1,
//             "expected count {}, got {}",
//             before.count + 1,
//             after.count
//         );
//         println!("[increment] ok: count {} -> {}", before.count, after.count);
//     }
//
//     // 3. increment by a non-authority must be rejected (payer still pays the fee).
//     let current = fetch_counter(&program, counter)?.context("counter missing")?;
//     let intruder = Keypair::new();
//     let res = send_increment(&program, counter, &intruder);
//     ensure!(res.is_err(), "increment by non-authority should have failed");
//     let unchanged = fetch_counter(&program, counter)?.context("counter missing")?;
//     ensure!(unchanged.count == current.count, "count changed on rejected increment");
//     println!("[unauthorized] ok: rejected with {}", short_err(&res.unwrap_err()));
//
//     println!("\nall checks passed");
//     Ok(())
// }
//
// fn fetch_counter(program: &Program<Rc<Keypair>>, counter: Pubkey) -> Result<Option<Counter>> {
//     match program.account::<Counter>(counter) {
//         Ok(state) => Ok(Some(state)),
//         Err(ClientError::AccountNotFound) => Ok(None),
//         Err(e) => Err(e.into()),
//     }
// }
//
// fn send_increment(
//     program: &Program<Rc<Keypair>>,
//     counter: Pubkey,
//     authority: &Keypair,
// ) -> Result<String, ClientError> {
//     let mut request = program
//         .request()
//         .accounts(anchor_vault::accounts::Increment {
//             counter,
//             authority: authority.pubkey(),
//         })
//         .args(anchor_vault::instruction::Increment {});
//     // The payer already signs; adding it again causes "too many signers".
//     if authority.pubkey() != program.payer() {
//         request = request.signer(authority);
//     }
//     request.send().map(|sig| sig.to_string())
// }
//
// fn short_err(e: &ClientError) -> String {
//     let s = e.to_string();
//     for code in ["Unauthorized", "CounterOverflow"] {
//         if s.contains(code) {
//             return code.to_string();
//         }
//     }
//     s.lines().next().unwrap_or_default().to_string()
// }

// =====================================================================================
// V1 CLIENT (transaction v1 / SIMD-0385, 4096-byte limit)
//
// Differences from the legacy client:
// - Messages are compiled with `v1::Message::try_compile_with_config` and wrapped in
//   `VersionedMessage::V1`.
// - Compute budget lives in the message header (`v1::TransactionConfig`), not in
//   ComputeBudget instructions. Unset limits are ZERO in v1 (not runtime defaults), so the
//   CU limit and loaded-accounts data size must always be set.
// - Priority fee is a TOTAL in lamports, not micro-lamports per CU.
// - Reading a v1 transaction needs `max_supported_transaction_version: Some(1)`.
// - No address lookup tables, no duplicate account keys.
//
// Usage:
//   cargo run -p anchor-vault-client -- [RPC_URL] [KEYPAIR_PATH]
// RPC_URL defaults to the `RPC_URL` env var, falling back to the devnet Helius endpoint.
// KEYPAIR_PATH defaults to ~/.config/solana/id.json
// =====================================================================================

use anchor_vault::{
    constants::{COUNTER_SEED, MAX_COUNT},
    state::Counter,
};
use anchor_lang::{
    prelude::Pubkey, solana_program::instruction::Instruction, system_program,
    AccountDeserialize, InstructionData, ToAccountMetas,
};
use anyhow::{bail, ensure, Context, Result};
use solana_keypair::{read_keypair_file, Keypair, Signer};
use solana_message::{v1, VersionedMessage};
use solana_rpc_client::rpc_client::RpcClient;
use solana_rpc_client_api::config::{CommitmentConfig, RpcTransactionConfig, UiTransactionEncoding};
use solana_transaction::versioned::VersionedTransaction;

const DEFAULT_RPC_URL: &str =
    "https://devnet.helius-rpc.com/?api-key=8c79234f-3452-457b-96e3-171b70c0cfd4";

/// Resource limits carried in the v1 message header.
/// - CU limit: initialize (create account + transfer CPI) uses well under 50k.
/// - Loaded accounts data size: must cover the program's ProgramData account (~137 KB)
///   plus the payer/counter/system program, so 256 KiB.
/// - Heap size: left unset (v1 defaults heap to 32 KiB, which is enough).
/// - Priority fee: total lamports for the whole transaction.
const CONFIG: v1::TransactionConfig = v1::TransactionConfig::empty()
    .with_compute_unit_limit(50_000)
    .with_loaded_accounts_data_size_limit(256 * 1024)
    .with_priority_fee(1_000);

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let rpc_url = args
        .next()
        .or_else(|| std::env::var("RPC_URL").ok())
        .unwrap_or_else(|| DEFAULT_RPC_URL.to_string());
    let keypair_path = args.next().unwrap_or_else(|| {
        format!("{}/.config/solana/id.json", std::env::var("HOME").unwrap_or_default())
    });

    let payer = read_keypair_file(&keypair_path)
        .map_err(|e| anyhow::anyhow!("failed to read keypair {keypair_path}: {e}"))?;
    let rpc = RpcClient::new_with_commitment(rpc_url.clone(), CommitmentConfig::confirmed());

    let program_id = anchor_vault::id();
    let counter = Pubkey::find_program_address(&[COUNTER_SEED], &program_id).0;
    println!("cluster:  {}", rpc_url.split('?').next().unwrap_or_default());
    println!("program:  {program_id}");
    println!("payer:    {}", payer.pubkey());
    println!("counter:  {counter}");
    println!("tx:       v1 (4096-byte limit)\n");

    // 1. initialize (only if the counter PDA does not exist yet; it is a singleton).
    let before = match fetch_counter(&rpc, &counter)? {
        Some(state) => {
            println!("[initialize] skipped, counter already exists (count = {})", state.count);
            state
        }
        None => {
            let ix = Instruction::new_with_bytes(
                program_id,
                &anchor_vault::instruction::Initialize {}.data(),
                anchor_vault::accounts::Initialize {
                    payer: payer.pubkey(),
                    counter,
                    system_program: system_program::ID,
                }
                .to_account_metas(None),
            );
            let sig = send_v1(&rpc, ix, &payer, &[&payer]).context("initialize failed")?;
            println!("[initialize] tx: {sig}");
            verify_v1_on_chain(&rpc, &sig)?;

            let state = fetch_counter(&rpc, &counter)?.context("counter missing after init")?;
            ensure!(state.count == 0, "expected count 0 after init, got {}", state.count);
            ensure!(state.authority == payer.pubkey(), "authority mismatch after init");
            println!("[initialize] ok: count = 0, authority = payer");
            state
        }
    };

    if before.authority != payer.pubkey() {
        bail!(
            "counter authority is {}, not this payer; increment would fail",
            before.authority
        );
    }

    // 2. increment by the authority.
    if before.count >= MAX_COUNT {
        println!("[increment] counter already at MAX_COUNT ({MAX_COUNT}); expecting overflow error");
        let res = send_v1(&rpc, increment_ix(counter, payer.pubkey()), &payer, &[&payer]);
        ensure!(res.is_err(), "increment past MAX_COUNT should have failed");
        println!("[increment] ok: rejected with {}", short_err(&res.unwrap_err()));
    } else {
        let sig = send_v1(&rpc, increment_ix(counter, payer.pubkey()), &payer, &[&payer])
            .context("increment failed")?;
        println!("[increment] tx: {sig}");
        verify_v1_on_chain(&rpc, &sig)?;

        let after = fetch_counter(&rpc, &counter)?.context("counter missing")?;
        ensure!(
            after.count == before.count + 1,
            "expected count {}, got {}",
            before.count + 1,
            after.count
        );
        println!("[increment] ok: count {} -> {}", before.count, after.count);
    }

    // 3. increment by a non-authority must be rejected (payer still pays the fee and signs
    //    as fee payer; the intruder signs as `authority`).
    let current = fetch_counter(&rpc, &counter)?.context("counter missing")?;
    let intruder = Keypair::new();
    let res = send_v1(&rpc, increment_ix(counter, intruder.pubkey()), &payer, &[&payer, &intruder]);
    ensure!(res.is_err(), "increment by non-authority should have failed");
    let unchanged = fetch_counter(&rpc, &counter)?.context("counter missing")?;
    ensure!(unchanged.count == current.count, "count changed on rejected increment");
    println!("[unauthorized] ok: rejected with {}", short_err(&res.unwrap_err()));

    println!("\nall checks passed");
    Ok(())
}

fn increment_ix(counter: Pubkey, authority: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        anchor_vault::id(),
        &anchor_vault::instruction::Increment {}.data(),
        anchor_vault::accounts::Increment { counter, authority }.to_account_metas(None),
    )
}

/// Compile a v1 message, sign it, send it and wait for confirmation.
/// `signers` must contain exactly the required signers (the fee payer once).
fn send_v1(
    rpc: &RpcClient,
    ix: Instruction,
    payer: &Keypair,
    signers: &[&Keypair],
) -> Result<String> {
    let blockhash = rpc.get_latest_blockhash()?;
    let message = v1::Message::try_compile_with_config(&payer.pubkey(), &[ix], blockhash, CONFIG)?;
    let tx = VersionedTransaction::try_new(VersionedMessage::V1(message), signers)?;
    // The RPC client sends base64, which is required for anything over 1232 bytes.
    let sig = rpc.send_and_confirm_transaction(&tx)?;
    Ok(sig.to_string())
}

/// Fetch the transaction back with `max_supported_transaction_version: Some(1)` and check
/// that it really landed as a v1 transaction with our config in the message header.
fn verify_v1_on_chain(rpc: &RpcClient, sig: &str) -> Result<()> {
    let tx = rpc.get_transaction_with_config(
        &sig.parse()?,
        RpcTransactionConfig {
            encoding: Some(UiTransactionEncoding::Base64),
            commitment: Some(CommitmentConfig::confirmed()),
            max_supported_transaction_version: Some(1), // without this, reading a v1 tx fails
        },
    )?;
    let decoded = tx
        .transaction
        .transaction
        .decode()
        .context("could not decode fetched transaction")?;
    match decoded.message {
        VersionedMessage::V1(m) => {
            ensure!(m.config == CONFIG, "on-chain v1 config differs from what was sent");
            println!("             fetched back as v1, config = {:?}", m.config);
            Ok(())
        }
        other => bail!("expected a v1 transaction on-chain, got {other:?}"),
    }
}

fn fetch_counter(rpc: &RpcClient, counter: &Pubkey) -> Result<Option<Counter>> {
    match rpc.get_account_with_commitment(counter, CommitmentConfig::confirmed())?.value {
        Some(account) => Ok(Some(Counter::try_deserialize(&mut account.data.as_slice())?)),
        None => Ok(None),
    }
}

/// Map raw custom program error codes back to the program's ErrorCode names.
fn short_err(e: &anyhow::Error) -> String {
    let s = format!("{e:#}");
    for (hex, name) in [("0x1770", "Unauthorized"), ("0x1771", "CounterOverflow")] {
        if s.contains(hex) || s.contains(name) {
            return name.to_string();
        }
    }
    s.lines().next().unwrap_or_default().to_string()
}
