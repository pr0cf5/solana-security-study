pub mod level0;

use poc_framework::{
    solana_transaction_status::EncodedConfirmedTransactionWithStatusMeta, PrintableTransaction,
};

pub fn assert_tx_success(
    tx: EncodedConfirmedTransactionWithStatusMeta,
) -> EncodedConfirmedTransactionWithStatusMeta {
    match &tx.transaction.meta {
        Some(meta) if meta.err.is_some() => {
            tx.print();
            panic!("tx failed!")
        }
        _ => tx,
    }
}
