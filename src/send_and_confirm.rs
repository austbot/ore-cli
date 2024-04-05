use std::{
    io::{stdout, Write},
    time::Duration,
};

use solana_client::{
    client_error::{ClientError, ClientErrorKind, Result as ClientResult},
    nonblocking::rpc_client::RpcClient,
    rpc_config::RpcSendTransactionConfig,
};
use solana_program::instruction::Instruction;
use solana_sdk::{
    commitment_config::{CommitmentConfig, CommitmentLevel},
    signature::{Signature, Signer},
    transaction::Transaction,
};
use solana_transaction_status::{TransactionConfirmationStatus, UiTransactionEncoding};

use crate::Miner;

const RPC_RETRIES: usize = 0;
const GATEWAY_RETRIES: usize = 10;
const CONFIRM_RETRIES: usize = 10;

impl Miner {
    pub async fn send_and_confirm(
        &self,
        ixs: &[Instruction],
        skip_confirm: bool,
    ) -> ClientResult<Signature> {
        let mut stdout = stdout();
        let signer = self.signer();
        let client =
            RpcClient::new_with_commitment(self.cluster.clone(), CommitmentConfig::confirmed());

        // Return error if balance is zero
        // let balance = client
        //     .get_balance_with_commitment(&signer.pubkey(), CommitmentConfig::confirmed())
        //     .await
        //     .unwrap();
        // if balance.value <= 0 {
        //     return Err(ClientError {
        //         request: None,
        //         kind: ClientErrorKind::Custom("Insufficient SOL balance".into()),
        //     });
        // }

        let mut attempts = 0;
        loop {
            let (hash, slot) = client
                .get_latest_blockhash_with_commitment(CommitmentConfig::confirmed())
                .await
                .unwrap();
            let send_cfg = RpcSendTransactionConfig {
                skip_preflight: true,
                preflight_commitment: Some(CommitmentLevel::Confirmed),
                encoding: Some(UiTransactionEncoding::Base64),
                max_retries: Some(RPC_RETRIES),
                min_context_slot: Some(slot),
            };
            let mut tx = Transaction::new_with_payer(ixs, Some(&signer.pubkey()));
            tx.sign(&[&signer], hash);
            println!("Attempt: {:?}", attempts);

            loop {
                std::thread::sleep(Duration::from_millis(100));
                let bh = client.get_block_height().await.unwrap();
                eprintln!("Block height: {:?}", bh);
                eprintln!("Slot: {:?}", slot);
                if bh > slot - 300 {
                    break;
                }

                match client.send_transaction_with_config(&tx, send_cfg).await {
                    Ok(sig) => {
                        println!("{:?}", sig);

                        // Confirm tx
                        if skip_confirm {
                            return Ok(sig);
                        }
                        for _ in 0..CONFIRM_RETRIES {
                            std::thread::sleep(Duration::from_millis(100));
                            match client
                                .get_signature_status_with_commitment(
                                    &sig,
                                    CommitmentConfig::confirmed(),
                                )
                                .await
                            {
                                Ok(signature_statuses) => {
                                    if let Some(Ok(_)) = signature_statuses {
                                        return Ok(sig);
                                    } else {
                                        println!("No status");
                                    }
                                }

                                // Handle confirmation errors
                                Err(err) => {
                                    println!("Error: {:?}", err);
                                }
                            }
                        }
                    }

                    // Handle submit errors
                    Err(err) => {
                        println!("Error {:?}", err);
                    }
                }
            }
            stdout.flush().ok();
            attempts += 1;
            if attempts > GATEWAY_RETRIES {
                return Err(ClientError {
                    request: None,
                    kind: ClientErrorKind::Custom("Max retries".into()),
                });
            }
        }
    }
}
