use anchor_client::solana_sdk::{compute_budget::ComputeBudgetInstruction, pubkey::Pubkey};
use anyhow::Result;
use mpl_token_metadata::{
    instructions::{CreateMasterEditionV3Builder, CreateMetadataAccountV3Builder},
    types::{CollectionDetails, Creator, DataV2},
};
use spl_associated_token_account::{
    get_associated_token_address, instruction::create_associated_token_account,
};
use spl_token::{
    instruction::{initialize_mint, mint_to},
    ID as TOKEN_PROGRAM_ID,
};

use crate::{
    candy_machine::CANDY_MACHINE_ID,
    common::*,
    config::ConfigData,
    deploy::DeployArgs,
    pdas::{find_master_edition_pda, find_metadata_pda},
    setup::SugarClient,
};

pub fn create_collection(
    client: &SugarClient,
    _candy_machine: Pubkey,
    cache: &mut Cache,
    config_data: &ConfigData,
    args: &DeployArgs,
) -> Result<(Signature, Pubkey)> {
    let program = client.program(CANDY_MACHINE_ID)?;
    let payer = program.payer();

    let collection_mint = Keypair::new();
    let collection_item: &mut CacheItem = match cache.items.get_mut("-1") {
        Some(item) => item,
        None => {
            return Err(anyhow!("Trying to create and set collection when collection item info isn't in cache! This shouldn't happen!"));
        }
    };

    // Allocate memory for the account
    let min_rent = program
        .rpc()
        .get_minimum_balance_for_rent_exemption(MINT_LAYOUT as usize)?;

    // Create mint account
    let create_mint_account_ix = system_instruction::create_account(
        &payer,
        &collection_mint.pubkey(),
        min_rent,
        MINT_LAYOUT,
        &TOKEN_PROGRAM_ID,
    );

    // Initialize mint ix
    let init_mint_ix = initialize_mint(
        &TOKEN_PROGRAM_ID,
        &collection_mint.pubkey(),
        &payer,
        Some(&payer),
        0,
    )?;

    let ata_pubkey = get_associated_token_address(&payer, &collection_mint.pubkey());

    // Create associated account instruction
    let create_assoc_account_ix =
        create_associated_token_account(&payer, &payer, &collection_mint.pubkey(), &spl_token::ID);

    // Mint to instruction
    let mint_to_ix = mint_to(
        &TOKEN_PROGRAM_ID,
        &collection_mint.pubkey(),
        &ata_pubkey,
        &payer,
        &[],
        1,
    )?;

    let creator = Creator {
        address: payer,
        verified: true,
        share: 100,
    };
    let collection_metadata_pubkey = find_metadata_pda(&collection_mint.pubkey());

    let create_metadata_account_ix = CreateMetadataAccountV3Builder::new()
        .metadata(collection_metadata_pubkey)
        .mint(collection_mint.pubkey())
        .mint_authority(payer)
        .payer(payer)
        .update_authority(payer, true)
        .data(DataV2 {
            name: collection_item.name.clone(),
            symbol: config_data.symbol.clone(),
            uri: collection_item.metadata_link.clone(),
            creators: Some(vec![creator]),
            seller_fee_basis_points: 0,
            collection: None,
            uses: None,
        })
        .collection_details(CollectionDetails::V1 { size: 0 })
        .is_mutable(true)
        .instruction();

    let collection_edition_pubkey = find_master_edition_pda(&collection_mint.pubkey());

    let create_master_edition_ix = CreateMasterEditionV3Builder::new()
        .edition(collection_edition_pubkey)
        .mint(collection_mint.pubkey())
        .update_authority(payer)
        .mint_authority(payer)
        .metadata(collection_metadata_pubkey)
        .payer(payer)
        .max_supply(0)
        .instruction();
    let priority_fee = ComputeBudgetInstruction::set_compute_unit_price(args.priority_fee);

    let builder = program
        .request()
        .instruction(priority_fee)
        .instruction(create_mint_account_ix)
        .instruction(init_mint_ix)
        .instruction(create_assoc_account_ix)
        .instruction(mint_to_ix)
        .signer(&collection_mint)
        .instruction(create_metadata_account_ix)
        .instruction(create_master_edition_ix);

    let sig = builder.send()?;

    collection_item.on_chain = true;
    cache.program.collection_mint = collection_mint.pubkey().to_string();
    cache.sync_file()?;

    Ok((sig, collection_mint.pubkey()))
}
