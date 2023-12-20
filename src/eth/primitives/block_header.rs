use chrono::DateTime;
use chrono::Utc;
use ethereum_types::Bloom;
use ethereum_types::H64;
use ethereum_types::U256;
use ethers_core::types::Block as EthersBlock;
use hex_literal::hex;

use crate::eth::primitives::Address;
use crate::eth::primitives::BlockNumber;
use crate::eth::primitives::Gas;
use crate::eth::primitives::Hash;

/// Special hash used in block mining to indicate no uncle blocks.
const HASH_EMPTY_UNCLES: Hash = Hash::new_const(hex!("1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347"));

/// Special hash used in block mining to indicate no transaction root and no receipts root.
const HASH_EMPTY_TRANSACTIONS_ROOT: Hash = Hash::new_const(hex!("56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421"));

#[derive(Debug, Clone)]
pub struct BlockHeader {
    pub number: BlockNumber,
    pub hash: Hash,
    pub transactions_root: Hash,
    pub gas: Gas,
    pub bloom: Bloom,
    pub created_at: DateTime<Utc>,
}

impl BlockHeader {
    /// Creates a new block header with the given number.
    pub fn new(number: BlockNumber) -> Self {
        Self {
            number,
            hash: Hash::default(),
            transactions_root: HASH_EMPTY_TRANSACTIONS_ROOT,
            gas: Gas::ZERO,
            bloom: Bloom::default(),
            created_at: Utc::now(),
        }
    }
}

// -----------------------------------------------------------------------------
// Conversions: Self -> Other
// -----------------------------------------------------------------------------
impl<T> From<BlockHeader> for EthersBlock<T>
where
    T: Default,
{
    fn from(header: BlockHeader) -> Self {
        Self {
            // block: identifiers
            hash: Some(header.hash.into()),
            number: Some(header.number.into()),

            // block: relation with other blocks
            uncles_hash: HASH_EMPTY_UNCLES.into(),
            uncles: Vec::new(),
            parent_beacon_block_root: None,

            // mining: identifiers
            timestamp: header.created_at.timestamp().into(),
            author: Some(Address::COINBASE.into()),

            // minining: difficulty
            difficulty: U256::zero(),
            total_difficulty: Some(U256::zero()),
            nonce: Some(H64::zero()),

            // mining: gas
            gas_limit: Gas::from(100_000_000).into(),
            gas_used: header.gas.into(),
            base_fee_per_gas: Some(U256::zero()),
            blob_gas_used: None,
            excess_blob_gas: None,

            // transactions
            transactions_root: header.transactions_root.into(),
            receipts_root: HASH_EMPTY_TRANSACTIONS_ROOT.into(),

            // data
            logs_bloom: Some(header.bloom),
            extra_data: Default::default(),

            // TODO
            ..Default::default() // parent_hash: todo!(),
                                 // state_root: todo!(),

                                 // seal_fields: todo!(),
                                 // transactions: todo!(),
                                 // size: todo!(),
                                 // mix_hash: todo!(),
                                 // withdrawals_root: todo!(),
                                 // withdrawals: todo!(),
                                 // other: todo!(),
        }
    }
}