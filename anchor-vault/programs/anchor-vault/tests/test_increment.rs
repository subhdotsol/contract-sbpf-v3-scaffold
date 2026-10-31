// v2 tests

use {
    anchor_lang::{
        prelude::Pubkey,
        solana_program::{instruction::Instruction, system_program},
        AccountDeserialize, InstructionData, ToAccountMetas,
    },
    anchor_vault::{constants::MAX_COUNT, error::ErrorCode, state::Counter},
    litesvm::{types::TransactionResult, LiteSVM},
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};

fn counter_pda() -> Pubkey {
    Pubkey::find_program_address(
        &[anchor_vault::constants::COUNTER_SEED],
        &anchor_vault::id(),
    )
    .0
}

fn send(svm: &mut LiteSVM, ix: Instruction, signer: &Keypair) -> TransactionResult {
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[ix], Some(&signer.pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[signer]).unwrap();
    let res = svm.send_transaction(tx);
    svm.expire_blockhash();
    res
}

fn increment_ix(authority: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        anchor_vault::id(),
        &anchor_vault::instruction::Increment {}.data(),
        anchor_vault::accounts::Increment {
            counter: counter_pda(),
            authority,
        }
        .to_account_metas(None),
    )
}

/// Deploys the program and initializes the counter with `payer` as authority.
fn setup() -> (LiteSVM, Keypair) {
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!(concat!(
        env!("CARGO_TARGET_TMPDIR"),
        "/../deploy/anchor_vault.so"
    ));
    svm.add_program(anchor_vault::id(), bytes).unwrap();

    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();

    let ix = Instruction::new_with_bytes(
        anchor_vault::id(),
        &anchor_vault::instruction::Initialize {}.data(),
        anchor_vault::accounts::Initialize {
            payer: payer.pubkey(),
            counter: counter_pda(),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send(&mut svm, ix, &payer).unwrap();
    (svm, payer)
}

fn read_counter(svm: &LiteSVM) -> Counter {
    let account = svm.get_account(&counter_pda()).unwrap();
    Counter::try_deserialize(&mut account.data.as_slice()).unwrap()
}

fn assert_custom_error(res: TransactionResult, code: ErrorCode) {
    let err = res.expect_err("transaction should have failed");
    let expected = format!("Custom({})", u32::from(code));
    assert!(
        format!("{:?}", err.err).contains(&expected),
        "expected {expected}, got {:?}\nlogs: {:#?}",
        err.err,
        err.meta.logs
    );
}

#[test]
fn test_increment_by_authority() {
    let (mut svm, payer) = setup();

    send(&mut svm, increment_ix(payer.pubkey()), &payer).unwrap();
    assert_eq!(read_counter(&svm).count, 1);

    send(&mut svm, increment_ix(payer.pubkey()), &payer).unwrap();
    let counter = read_counter(&svm);
    assert_eq!(counter.count, 2);
    assert_eq!(counter.authority, payer.pubkey());
}

#[test]
fn test_increment_unauthorized() {
    let (mut svm, _payer) = setup();

    let attacker = Keypair::new();
    svm.airdrop(&attacker.pubkey(), 1_000_000_000).unwrap();

    let res = send(&mut svm, increment_ix(attacker.pubkey()), &attacker);
    assert_custom_error(res, ErrorCode::Unauthorized);
    assert_eq!(read_counter(&svm).count, 0);
}

#[test]
fn test_increment_overflow() {
    let (mut svm, payer) = setup();

    for _ in 0..MAX_COUNT {
        send(&mut svm, increment_ix(payer.pubkey()), &payer).unwrap();
    }
    assert_eq!(read_counter(&svm).count, MAX_COUNT);

    let res = send(&mut svm, increment_ix(payer.pubkey()), &payer);
    assert_custom_error(res, ErrorCode::CounterOverflow);
    assert_eq!(read_counter(&svm).count, MAX_COUNT);
}

// upgrading to v3 tests
