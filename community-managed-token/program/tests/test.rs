use solana_program::program_pack::Pack;
use solana_program_test::*;
use solana_sdk::{
    commitment_config::CommitmentLevel,
    instruction::Instruction,
    native_token::LAMPORTS_PER_SOL,
    pubkey::Pubkey,
    signature::Signature,
    signature::{Keypair, Signer},
    system_instruction,
    transaction::Transaction,
};

use community_managed_token::instruction::*;
use spl_associated_token_account::{
    get_associated_token_address, instruction::create_associated_token_account,
};

pub fn sol(amount: f64) -> u64 {
    (amount * LAMPORTS_PER_SOL as f64) as u64
}

async fn process_transaction(
    client: &mut BanksClient,
    instructions: Vec<Instruction>,
    signers: Vec<&Keypair>,
) -> anyhow::Result<Signature> {
    let mut tx = Transaction::new_with_payer(&instructions, Some(&signers[0].pubkey()));
    tx.partial_sign(&signers, client.get_latest_blockhash().await?);
    let sig = tx.signatures[0];
    client
        .process_transaction_with_commitment(tx, CommitmentLevel::Confirmed)
        .await?;
    Ok(sig)
}

async fn transfer(
    context: &mut BanksClient,
    payer: &Keypair,
    receiver: &Pubkey,
    amount: u64,
) -> anyhow::Result<Signature> {
    let ixs = vec![system_instruction::transfer(
        &payer.pubkey(),
        receiver,
        amount,
    )];
    process_transaction(context, ixs, vec![payer]).await
}

async fn initialize_mint2(
    context: &mut BanksClient,
    payer: &Keypair,
    mint: &Keypair,
    mint_authority: &Pubkey,
    freeze_authority: &Pubkey,
    decimals: u8,
) -> anyhow::Result<Signature> {
    let rent = context.get_rent().await.unwrap();
    let mint_rent = rent.minimum_balance(spl_token::state::Mint::LEN);

    let ixs = vec![
        system_instruction::create_account(
            &payer.pubkey(),
            &mint.pubkey(),
            mint_rent,
            spl_token::state::Mint::LEN as u64,
            &spl_token::id(),
        ),
        spl_token::instruction::initialize_mint2(
            &spl_token::id(),
            &mint.pubkey(),
            mint_authority,
            Some(freeze_authority),
            decimals,
        )?,
    ];
    process_transaction(context, ixs, vec![payer, mint]).await
}

fn community_managed_token_test() -> ProgramTest {
    ProgramTest::new(
        "community_managed_token",
        community_managed_token::id(),
        processor!(community_managed_token::process_instruction),
    )
}

#[tokio::test]
async fn test_community_managed_token_basic() {
    let mut context = community_managed_token_test().start_with_context().await;
    let lwc = &mut context.banks_client;
    let authority = Keypair::new();
    transfer(lwc, &context.payer, &authority.pubkey(), sol(10.0))
        .await
        .unwrap();
    let mint = Keypair::new();
    let mint_key = mint.pubkey();
    let create_ix =
        create_initialize_mint_instruction(&mint_key, &authority.pubkey(), &authority.pubkey(), 0)
            .unwrap();
    process_transaction(lwc, vec![create_ix], vec![&authority, &mint])
        .await
        .unwrap();

    let alice = Keypair::new();
    let alice_key = alice.pubkey();
    let bob = Keypair::new();
    let bob_key = bob.pubkey();
    let eve = Keypair::new();
    let eve_key = eve.pubkey();

    for k in [&alice_key, &bob_key] {
        transfer(lwc, &context.payer, k, sol(1.0)).await.unwrap();
        let create_ata = create_initialize_account_instruction(
            &mint_key,
            k,
            &authority.pubkey(),
            &authority.pubkey(),
        )
        .unwrap();
        let mint_to_ix =
            create_mint_to_instruction(&mint_key, k, &authority.pubkey(), 1000).unwrap();
        process_transaction(lwc, vec![create_ata, mint_to_ix], vec![&authority])
            .await
            .unwrap();
    }

    let create_eve = create_associated_token_account(&authority.pubkey(), &eve_key, &mint_key);
    process_transaction(lwc, vec![create_eve], vec![&authority])
        .await
        .unwrap();

    // Try transfer the normal way
    let failed_transfer_ix = spl_token::instruction::transfer(
        &spl_token::id(),
        &get_associated_token_address(&alice_key, &mint_key),
        &get_associated_token_address(&bob_key, &mint_key),
        &alice_key,
        &[],
        100,
    )
    .unwrap();

    assert!(
        process_transaction(lwc, vec![failed_transfer_ix], vec![&alice])
            .await
            .is_err()
    );

    let eve_ix =
        create_transfer_instruction(&alice_key, &eve_key, &mint_key, &authority.pubkey(), 100)
            .unwrap();

    assert!(
        process_transaction(lwc, vec![eve_ix], vec![&alice, &authority])
            .await
            .is_err()
    );

    let successful_transfer_ix =
        create_transfer_instruction(&alice_key, &bob_key, &mint_key, &authority.pubkey(), 100)
            .unwrap();
    let burn_ix = create_burn_instruction(&mint_key, &alice_key, &authority.pubkey(), 900).unwrap();

    process_transaction(
        lwc,
        vec![successful_transfer_ix, burn_ix],
        vec![&alice, &authority],
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn test_community_managed_token_with_delegate() {
    let mut context = community_managed_token_test().start_with_context().await;
    let lwc = &mut context.banks_client;
    let authority = Keypair::new();
    transfer(lwc, &context.payer, &authority.pubkey(), sol(10.0))
        .await
        .unwrap();
    let mint = Keypair::new();
    let mint_key = mint.pubkey();
    let create_ix =
        create_initialize_mint_instruction(&mint_key, &authority.pubkey(), &authority.pubkey(), 0)
            .unwrap();
    process_transaction(lwc, vec![create_ix], vec![&authority, &mint])
        .await
        .unwrap();

    let alice = Keypair::new();
    let alice_key = alice.pubkey();
    let bob = Keypair::new();
    let bob_key = bob.pubkey();
    let eve = Keypair::new();
    let eve_key = eve.pubkey();

    transfer(lwc, &context.payer, &alice_key, sol(1.0))
        .await
        .unwrap();
    transfer(lwc, &context.payer, &bob_key, sol(1.0))
        .await
        .unwrap();

    let create_alice_ata_ix = create_initialize_account_instruction(
        &mint_key,
        &alice_key,
        &authority.pubkey(),
        &authority.pubkey(),
    )
    .unwrap();
    let mint_to_ix =
        create_mint_to_instruction(&mint_key, &alice_key, &authority.pubkey(), 1).unwrap();
    let delegate_ix =
        create_approve_instruction(&mint_key, &alice_key, &bob_key, &authority.pubkey(), 1)
            .unwrap();
    process_transaction(
        lwc,
        vec![create_alice_ata_ix, mint_to_ix, delegate_ix],
        vec![&alice, &authority],
    )
    .await
    .unwrap();

    let create_eve_ata_ix = create_initialize_account_instruction(
        &mint_key,
        &eve_key,
        &authority.pubkey(),
        &authority.pubkey(),
    )
    .unwrap();
    let successful_transfer_ix = create_transfer_with_delegate_instruction(
        &alice_key,
        &eve_key,
        &bob_key,
        &mint_key,
        &authority.pubkey(),
        1,
    )
    .unwrap();
    process_transaction(
        lwc,
        vec![create_eve_ata_ix, successful_transfer_ix],
        vec![&bob, &authority],
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn test_community_managed_token_wrap() {
    let mut context = community_managed_token_test().start_with_context().await;
    let lwc = &mut context.banks_client;

    let old_authority = Keypair::new();
    let new_authority = Keypair::new();
    transfer(lwc, &context.payer, &old_authority.pubkey(), sol(10.0))
        .await
        .unwrap();

    let mint = Keypair::new();
    let mint_key = mint.pubkey();

    initialize_mint2(
        lwc,
        &old_authority,
        &mint,
        &old_authority.pubkey(),
        &old_authority.pubkey(),
        0u8,
    )
    .await
    .unwrap();

    let wrap_ix = create_wrap_instruction(
        &mint_key,
        &old_authority.pubkey(),
        &old_authority.pubkey(),
        &new_authority.pubkey(),
    )
    .unwrap();
    process_transaction(lwc, vec![wrap_ix], vec![&old_authority])
        .await
        .unwrap();
}
