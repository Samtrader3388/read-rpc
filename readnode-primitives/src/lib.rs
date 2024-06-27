use num_traits::ToPrimitive;
use std::convert::TryFrom;
use std::str::FromStr;

use near_indexer_primitives::{views, CryptoHash, IndexerTransactionWithOutcome};

#[derive(
    Clone,
    Debug,
    serde::Serialize,
    serde::Deserialize,
    borsh::BorshSerialize,
    borsh::BorshDeserialize,
)]
pub struct ExecutionOutcomeWithReceipt {
    pub execution_outcome: views::ExecutionOutcomeWithIdView,
    pub receipt: views::ReceiptView,
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Debug)]
pub struct TransactionKey {
    pub transaction_hash: CryptoHash,
    pub block_height: u64,
}

impl TransactionKey {
    pub fn new(transaction_hash: CryptoHash, block_height: u64) -> Self {
        Self {
            transaction_hash,
            block_height,
        }
    }
}

#[derive(
    borsh::BorshSerialize,
    borsh::BorshDeserialize,
    serde::Serialize,
    serde::Deserialize,
    Debug,
    Clone,
)]
pub struct CollectingTransactionDetails {
    pub transaction: views::SignedTransactionView,
    pub receipts: Vec<views::ReceiptView>,
    pub execution_outcomes: Vec<views::ExecutionOutcomeWithIdView>,
    // block_height using to handle transaction hash collisions
    pub block_height: u64,
}

impl CollectingTransactionDetails {
    pub fn from_indexer_tx(transaction: IndexerTransactionWithOutcome, block_height: u64) -> Self {
        Self {
            transaction: transaction.transaction.clone(),
            receipts: vec![],
            execution_outcomes: vec![transaction.outcome.execution_outcome],
            block_height,
        }
    }

    /// Build unique transaction key based on transaction_hash and block_height
    /// Help to handle transaction hash collisions
    pub fn transaction_key(&self) -> TransactionKey {
        TransactionKey::new(self.transaction.hash.clone(), self.block_height)
    }

    pub fn final_status(&self) -> Option<views::FinalExecutionStatus> {
        let mut looking_for_id = self.transaction.hash;
        let num_outcomes = self.execution_outcomes.len();
        self.execution_outcomes.iter().find_map(|outcome_with_id| {
            if outcome_with_id.id == looking_for_id {
                match &outcome_with_id.outcome.status {
                    views::ExecutionStatusView::Unknown if num_outcomes == 1 => {
                        Some(views::FinalExecutionStatus::NotStarted)
                    }
                    views::ExecutionStatusView::Unknown => {
                        Some(views::FinalExecutionStatus::Started)
                    }
                    views::ExecutionStatusView::Failure(e) => {
                        Some(views::FinalExecutionStatus::Failure(e.clone()))
                    }
                    views::ExecutionStatusView::SuccessValue(v) => {
                        Some(views::FinalExecutionStatus::SuccessValue(v.clone()))
                    }
                    views::ExecutionStatusView::SuccessReceiptId(id) => {
                        looking_for_id = *id;
                        None
                    }
                }
            } else {
                None
            }
        })
    }

    pub fn to_final_transaction_result(&self) -> anyhow::Result<TransactionDetails> {
        let mut outcomes = self.execution_outcomes.clone();
        match self.final_status() {
            Some(status) => {
                let receipts_outcome = outcomes.split_off(1);
                let transaction_outcome = outcomes.pop().unwrap();
                Ok(TransactionDetails {
                    receipts: self.receipts.clone(),
                    receipts_outcome,
                    status,
                    transaction: self.transaction.clone(),
                    transaction_outcome,
                })
            }
            None => anyhow::bail!("Results should resolve to a final outcome"),
        }
    }
}

impl From<CollectingTransactionDetails> for TransactionDetails {
    fn from(tx: CollectingTransactionDetails) -> Self {
        let mut outcomes = tx.execution_outcomes.clone();
        let receipts_outcome = outcomes.split_off(1);
        let transaction_outcome = outcomes.pop().unwrap();
        // Execution status defined by nearcore/chain.rs:get_final_transaction_result
        // FinalExecutionStatus::NotStarted - the tx is not converted to the receipt yet
        // FinalExecutionStatus::Started - we have at least 1 receipt, but the first leaf receipt_id (using dfs) hasn't finished the execution
        // FinalExecutionStatus::Failure - the result of the first leaf receipt_id
        // FinalExecutionStatus::SuccessValue - the result of the first leaf receipt_id
        let status = tx
            .final_status()
            .unwrap_or(views::FinalExecutionStatus::NotStarted);
        Self {
            receipts: tx.receipts,
            receipts_outcome,
            status,
            transaction: tx.transaction,
            transaction_outcome,
        }
    }
}

#[derive(
    borsh::BorshSerialize,
    borsh::BorshDeserialize,
    serde::Serialize,
    serde::Deserialize,
    Debug,
    Clone,
)]
pub struct TransactionDetails {
    pub receipts: Vec<views::ReceiptView>,
    pub receipts_outcome: Vec<views::ExecutionOutcomeWithIdView>,
    pub status: views::FinalExecutionStatus,
    pub transaction: views::SignedTransactionView,
    pub transaction_outcome: views::ExecutionOutcomeWithIdView,
}

#[derive(borsh::BorshDeserialize, serde::Serialize, Debug, Clone)]
pub struct TransactionDetailsV0201 {
    pub receipts: Vec<near_indexer_primitives_0_20_1::views::ReceiptView>,
    pub receipts_outcome: Vec<near_indexer_primitives_0_20_1::views::ExecutionOutcomeWithIdView>,
    pub status: near_indexer_primitives_0_20_1::views::FinalExecutionStatus,
    pub transaction: near_indexer_primitives_0_20_1::views::SignedTransactionView,
    pub transaction_outcome: near_indexer_primitives_0_20_1::views::ExecutionOutcomeWithIdView,
}

#[derive(borsh::BorshDeserialize, serde::Serialize, Debug, Clone)]
pub struct TransactionDetailsV0212 {
    pub receipts: Vec<near_indexer_primitives_0_21_2::views::ReceiptView>,
    pub receipts_outcome: Vec<near_indexer_primitives_0_21_2::views::ExecutionOutcomeWithIdView>,
    pub status: near_indexer_primitives_0_21_2::views::FinalExecutionStatus,
    pub transaction: near_indexer_primitives_0_21_2::views::SignedTransactionView,
    pub transaction_outcome: near_indexer_primitives_0_21_2::views::ExecutionOutcomeWithIdView,
}

#[derive(borsh::BorshDeserialize, serde::Serialize, Debug, Clone)]
pub struct TransactionDetailsV0220 {
    pub receipts: Vec<near_indexer_primitives_0_22_0::views::ReceiptView>,
    pub receipts_outcome: Vec<near_indexer_primitives_0_22_0::views::ExecutionOutcomeWithIdView>,
    pub status: near_indexer_primitives_0_22_0::views::FinalExecutionStatus,
    pub transaction: near_indexer_primitives_0_22_0::views::SignedTransactionView,
    pub transaction_outcome: near_indexer_primitives_0_22_0::views::ExecutionOutcomeWithIdView,
}

#[derive(borsh::BorshDeserialize, serde::Serialize, Debug, Clone)]
pub struct TransactionDetailsV0230 {
    pub receipts: Vec<near_indexer_primitives_0_23_0::views::ReceiptView>,
    pub receipts_outcome: Vec<near_indexer_primitives_0_23_0::views::ExecutionOutcomeWithIdView>,
    pub status: near_indexer_primitives_0_23_0::views::FinalExecutionStatus,
    pub transaction: near_indexer_primitives_0_23_0::views::SignedTransactionView,
    pub transaction_outcome: near_indexer_primitives_0_23_0::views::ExecutionOutcomeWithIdView,
}

// Deserialize old versions of the TransactionDetails
// This is needed to handle the backward incompatible changes in the TransactionDetails
enum TransactionDetailsOldVersion {
    V0201(TransactionDetailsV0201),
    V0212(TransactionDetailsV0212),
    V0220(TransactionDetailsV0220),
    V0230(TransactionDetailsV0230),
}

impl TransactionDetailsOldVersion {
    fn borsh_deserialize(data: &[u8]) -> anyhow::Result<Self> {
        match borsh::from_slice::<TransactionDetailsV0201>(data) {
            Ok(tx_details) => Ok(TransactionDetailsOldVersion::V0201(tx_details)),
            Err(_) => match borsh::from_slice::<TransactionDetailsV0212>(data) {
                Ok(tx_details) => Ok(TransactionDetailsOldVersion::V0212(tx_details)),
                Err(_) => match borsh::from_slice::<TransactionDetailsV0220>(data) {
                    Ok(tx_details) => Ok(TransactionDetailsOldVersion::V0220(tx_details)),
                    Err(_) => match borsh::from_slice::<TransactionDetailsV0230>(data) {
                        Ok(tx_details) => Ok(TransactionDetailsOldVersion::V0230(tx_details)),
                        Err(err) => Err(anyhow::anyhow!(
                            "Failed to deserialize TransactionDetails: {}",
                            err
                        )),
                    },
                },
            },
        }
    }

    fn to_latest(&self) -> anyhow::Result<TransactionDetails> {
        let value = match self {
            TransactionDetailsOldVersion::V0201(tx_details) => serde_json::to_value(tx_details)?,
            TransactionDetailsOldVersion::V0212(tx_details) => serde_json::to_value(tx_details)?,
            TransactionDetailsOldVersion::V0220(tx_details) => serde_json::to_value(tx_details)?,
            TransactionDetailsOldVersion::V0230(tx_details) => serde_json::to_value(tx_details)?,
        };
        Ok(serde_json::from_value(value)?)
    }
}

impl TransactionDetails {
    pub fn to_final_execution_outcome(&self) -> views::FinalExecutionOutcomeView {
        views::FinalExecutionOutcomeView {
            status: self.status.clone(),
            transaction: self.transaction.clone(),
            transaction_outcome: self.transaction_outcome.clone(),
            receipts_outcome: self.receipts_outcome.clone(),
        }
    }

    pub fn to_final_execution_outcome_with_receipts(
        &self,
    ) -> views::FinalExecutionOutcomeWithReceiptView {
        views::FinalExecutionOutcomeWithReceiptView {
            final_outcome: self.to_final_execution_outcome(),
            receipts: self
                .receipts
                .iter()
                // We need to filter out the local receipts
                // (which is the receipt transaction was converted into if transaction's signer and receiver are the same)
                // because NEAR JSON RPC doesn't return them. We need to filter them out because they are not
                // expected to be present in the final response from the JSON RPC.
                .filter(|receipt|
                    if self.transaction.signer_id == self.transaction.receiver_id {
                        receipt.receipt_id != *self
                    .transaction_outcome
                    .outcome
                    .receipt_ids
                    .first()
                    .expect("Transaction ExecutionOutcome must have exactly one receipt id in `receipt_ids`")
                    } else {
                        true
                    }
                )
                .cloned()
                .collect(),
        }
    }

    // Deserialize TransactionDetails from bytes
    // If the deserialization fails, try to deserialize the old version of the TransactionDetails
    // and convert it to the new version
    // This is needed to handle the backward incompatible changes in the TransactionDetails
    // https://github.com/near/nearcore/pull/10676/files#diff-1e4fc99d32e48420a9bd37050fa1412758cba37825851edea40cbdfcab406944R1927
    pub fn borsh_deserialize(data: &[u8]) -> anyhow::Result<Self> {
        match borsh::from_slice::<Self>(data) {
            Ok(tx_details) => Ok(tx_details),
            Err(_) => TransactionDetailsOldVersion::borsh_deserialize(data)?.to_latest(),
        }
    }
}

pub type StateKey = Vec<u8>;
pub type StateValue = Vec<u8>;
pub struct BlockHeightShardId(pub u64, pub u64);
pub struct QueryData<T: borsh::BorshDeserialize> {
    pub data: T,
    // block_height and block_hash we return here represents the moment
    // when the data was last updated in the database
    // We used to return it in the `QueryResponse` but it was replaced with
    // the logic that corresponds the logic of the `nearcore` RPC API
    pub block_height: near_indexer_primitives::types::BlockHeight,
    pub block_hash: CryptoHash,
}

pub struct ReceiptRecord {
    pub receipt_id: CryptoHash,
    pub parent_transaction_hash: CryptoHash,
    pub receiver_id: near_indexer_primitives::types::AccountId,
    pub block_height: near_indexer_primitives::types::BlockHeight,
    pub block_hash: CryptoHash,
    pub shard_id: near_indexer_primitives::types::ShardId,
}

pub struct BlockRecord {
    pub height: u64,
    pub hash: CryptoHash,
}

#[derive(Debug)]
pub struct EpochValidatorsInfo {
    pub epoch_id: CryptoHash,
    pub epoch_height: u64,
    pub epoch_start_height: u64,
    pub validators_info: views::EpochValidatorInfo,
}

#[derive(Debug)]
pub struct IndexedEpochInfo {
    pub epoch_id: CryptoHash,
    pub epoch_height: u64,
    pub epoch_start_height: u64,
    pub validators_info: views::EpochValidatorInfo,
}

#[derive(Debug)]
pub struct IndexedEpochInfoWithPreviousAndNextEpochId {
    pub previous_epoch_id: Option<CryptoHash>,
    pub epoch_info: IndexedEpochInfo,
    pub next_epoch_id: CryptoHash,
}

// TryFrom impls for defined types

impl<T> TryFrom<(T, T)> for BlockHeightShardId
where
    T: ToPrimitive,
{
    type Error = anyhow::Error;

    fn try_from(value: (T, T)) -> Result<Self, Self::Error> {
        let stored_at_block_height = value
            .0
            .to_u64()
            .ok_or_else(|| anyhow::anyhow!("Failed to parse `stored_at_block_height` to u64"))?;

        let parsed_shard_id = value
            .1
            .to_u64()
            .ok_or_else(|| anyhow::anyhow!("Failed to parse `shard_id` to u64"))?;

        Ok(BlockHeightShardId(stored_at_block_height, parsed_shard_id))
    }
}

impl<T>
    TryFrom<(
        Vec<u8>,
        near_indexer_primitives::types::BlockHeight,
        CryptoHash,
    )> for QueryData<T>
where
    T: borsh::BorshDeserialize,
{
    type Error = anyhow::Error;

    fn try_from(
        value: (
            Vec<u8>,
            near_indexer_primitives::types::BlockHeight,
            CryptoHash,
        ),
    ) -> Result<Self, Self::Error> {
        let data = T::try_from_slice(&value.0)?;

        Ok(Self {
            data,
            block_height: value.1,
            block_hash: value.2,
        })
    }
}

impl<T> TryFrom<(String, String, String, T, String, T)> for ReceiptRecord
where
    T: ToPrimitive,
{
    type Error = anyhow::Error;

    fn try_from(value: (String, String, String, T, String, T)) -> Result<Self, Self::Error> {
        let receipt_id = CryptoHash::from_str(&value.0).map_err(|err| {
            anyhow::anyhow!("Failed to parse `receipt_id` to CryptoHash: {}", err)
        })?;
        let parent_transaction_hash = CryptoHash::from_str(&value.1).map_err(|err| {
            anyhow::anyhow!(
                "Failed to parse `parent_transaction_hash` to CryptoHash: {}",
                err
            )
        })?;
        let receiver_id =
            near_indexer_primitives::types::AccountId::from_str(&value.2).map_err(|err| {
                anyhow::anyhow!("Failed to parse `receiver_id` to AccountId: {}", err)
            })?;
        let block_height = value
            .3
            .to_u64()
            .ok_or_else(|| anyhow::anyhow!("Failed to parse `block_height` to u64"))?;
        let block_hash = CryptoHash::from_str(&value.4).map_err(|err| {
            anyhow::anyhow!("Failed to parse `block_hash` to CryptoHash: {}", err)
        })?;
        let shard_id = value
            .5
            .to_u64()
            .ok_or_else(|| anyhow::anyhow!("Failed to parse `shard_id` to u64"))?;

        Ok(ReceiptRecord {
            receipt_id,
            parent_transaction_hash,
            receiver_id,
            block_height,
            block_hash,
            shard_id,
        })
    }
}

impl<T> TryFrom<(String, T)> for BlockRecord
where
    T: ToPrimitive,
{
    type Error = anyhow::Error;

    fn try_from(value: (String, T)) -> Result<Self, Self::Error> {
        let height = value
            .1
            .to_u64()
            .ok_or_else(|| anyhow::anyhow!("Failed to parse `block_height` to u64"))?;
        let hash = CryptoHash::from_str(&value.0).map_err(|err| {
            anyhow::anyhow!("Failed to parse `block_hash` to CryptoHash: {}", err)
        })?;

        Ok(BlockRecord { height, hash })
    }
}
