//! RPC server for HTTP and WS.

use std::ops::Deref;
use std::sync::Arc;

use ethereum_types::U256;
use jsonrpsee::server::RpcModule;
use jsonrpsee::server::RpcServiceBuilder;
use jsonrpsee::server::Server;
use jsonrpsee::types::ErrorObjectOwned;
use jsonrpsee::types::Params;
use jsonrpsee::IntoSubscriptionCloseResponse;
use jsonrpsee::PendingSubscriptionSink;
use serde_json::Value as JsonValue;

use crate::eth::primitives::Address;
use crate::eth::primitives::BlockSelection;
use crate::eth::primitives::Bytes;
use crate::eth::primitives::CallInput;
use crate::eth::primitives::Hash;
use crate::eth::primitives::StoragerPointInTime;
use crate::eth::primitives::TransactionInput;
use crate::eth::rpc::next_rpc_param;
use crate::eth::rpc::parse_rpc_rlp;
use crate::eth::rpc::rpc_internal_error;
use crate::eth::rpc::rpc_parser::next_rpc_param_default;
use crate::eth::rpc::rpc_parser::rpc_parsing_error;
use crate::eth::rpc::RpcContext;
use crate::eth::rpc::RpcMiddleware;
use crate::eth::rpc::RpcSubscriptions;
use crate::eth::storage::EthStorage;
use crate::eth::EthExecutor;

// -----------------------------------------------------------------------------
// Server
// -----------------------------------------------------------------------------

pub async fn serve_rpc(mut executor: EthExecutor, eth_storage: Arc<dyn EthStorage>) -> eyre::Result<()> {
    // configure subscriptions
    let subs = Arc::new(RpcSubscriptions::default());
    Arc::clone(&subs).spawn_subscriptions_cleaner();
    executor.set_rpc_block_notifier(Arc::clone(&subs).spawn_new_heads_notifier());
    executor.set_rpc_log_notifier(Arc::clone(&subs).spawn_logs_notifier());

    // configure context
    let ctx = RpcContext {
        chain_id: 2008,
        client_version: "ledger",
        gas_price: 0,

        // services
        executor,
        storage: eth_storage,

        // subscriptions
        subs,
    };
    tracing::info!(?ctx, "starting rpc server");

    // configure module
    let mut module = RpcModule::<RpcContext>::new(ctx);
    module = register_methods(module)?;

    // configure middleware
    let rpc_middleware = RpcServiceBuilder::new().layer_fn(RpcMiddleware::new);

    // serve module
    let server = Server::builder().set_rpc_middleware(rpc_middleware).build("0.0.0.0:3000").await?;
    let handle = server.start(module);
    handle.stopped().await;

    Ok(())
}

fn register_methods(mut module: RpcModule<RpcContext>) -> eyre::Result<RpcModule<RpcContext>> {
    // blockchain
    module.register_method("net_version", net_version)?;
    module.register_method("eth_chainId", eth_chain_id)?;
    module.register_method("web3_clientVersion", web3_client_version)?;

    // gas
    module.register_method("eth_gasPrice", eth_gas_price)?;

    // block
    module.register_method("eth_blockNumber", eth_block_number)?;
    module.register_method("eth_getBlockByNumber", eth_get_block_by_selector)?;
    module.register_method("eth_getBlockByHash", eth_get_block_by_selector)?;

    // transactions
    module.register_method("eth_getTransactionCount", eth_get_transaction_count)?;
    module.register_method("eth_getTransactionByHash", eth_get_transaction_by_hash)?;
    module.register_method("eth_getTransactionReceipt", eth_get_transaction_receipt)?;
    module.register_method("eth_estimateGas", eth_estimate_gas)?;
    module.register_method("eth_call", eth_call)?;
    module.register_method("eth_sendRawTransaction", eth_send_raw_transaction)?;

    // account
    module.register_method("eth_getBalance", eth_get_balance)?;
    module.register_method("eth_getCode", eth_get_code)?;

    // subscriptions
    module.register_subscription("eth_subscribe", "eth_subscription", "eth_unsubscribe", eth_subscribe)?;

    Ok(module)
}

// -----------------------------------------------------------------------------
// Handlers
// -----------------------------------------------------------------------------

// Blockchain

/// OK
fn net_version(_: Params, ctx: &RpcContext) -> String {
    ctx.chain_id.to_string()
}

/// OK
fn eth_chain_id(_: Params, ctx: &RpcContext) -> String {
    hex_num(ctx.chain_id)
}

/// OK
fn web3_client_version(_: Params, ctx: &RpcContext) -> String {
    ctx.client_version.to_owned()
}

// Gas

/// OK
fn eth_gas_price(_: Params, _: &RpcContext) -> String {
    hex_zero()
}

// Block

fn eth_block_number(_: Params, ctx: &RpcContext) -> Result<JsonValue, ErrorObjectOwned> {
    let number = ctx.storage.read_current_block_number()?;
    Ok(serde_json::to_value(number).unwrap())
}
fn eth_get_block_by_selector(params: Params, ctx: &RpcContext) -> Result<JsonValue, ErrorObjectOwned> {
    let (params, block_selection) = next_rpc_param::<BlockSelection>(params.sequence())?;
    let (_, full_transactions) = next_rpc_param::<bool>(params)?;

    let block = ctx.storage.read_block(&block_selection)?;

    match (block, full_transactions) {
        (Some(block), true) => Ok(block.to_json_rpc_with_full_transactions()),
        (Some(block), false) => Ok(block.to_json_rpc_with_transactions_hashes()),
        (None, _) => Ok(JsonValue::Null),
    }
}

// Transaction

fn eth_get_transaction_count(params: Params, ctx: &RpcContext) -> Result<String, ErrorObjectOwned> {
    let (params, address) = next_rpc_param::<Address>(params.sequence())?;
    let (_, block_selection) = next_rpc_param_default::<BlockSelection>(params)?;

    let point_in_time = ctx.storage.translate_to_point_in_time(&block_selection)?;
    let account = ctx.storage.read_account(&address, &point_in_time)?;
    Ok(hex_num(account.nonce))
}

fn eth_get_transaction_by_hash(params: Params, ctx: &RpcContext) -> Result<JsonValue, ErrorObjectOwned> {
    let (_, hash) = next_rpc_param::<Hash>(params.sequence())?;
    let mined = ctx.storage.read_mined_transaction(&hash)?;

    match mined {
        Some(mined) => Ok(mined.to_json_rpc_transaction()),
        None => Ok(JsonValue::Null),
    }
}

fn eth_get_transaction_receipt(params: Params, ctx: &RpcContext) -> Result<JsonValue, ErrorObjectOwned> {
    let (_, hash) = next_rpc_param::<Hash>(params.sequence())?;
    match ctx.storage.read_mined_transaction(&hash)? {
        Some(mined_transaction) => Ok(mined_transaction.to_json_rpc_receipt()),
        None => Ok(JsonValue::Null),
    }
}

/// OK
fn eth_estimate_gas(params: Params, ctx: &RpcContext) -> Result<String, ErrorObjectOwned> {
    let (_, call) = next_rpc_param::<CallInput>(params.sequence())?;

    match ctx.executor.call(call, StoragerPointInTime::Present) {
        // result is success
        Ok(result) if result.is_success() => Ok(hex_num(result.gas)),

        // result is failure
        Ok(result) => Err(rpc_internal_error(hex_data(result.output))),

        // internal error
        Err(e) => {
            tracing::error!(reason = ?e, "failed to execute eth_estimateGas");
            Err(e.into())
        }
    }
}

fn eth_call(params: Params, ctx: &RpcContext) -> Result<String, ErrorObjectOwned> {
    let (params, call) = next_rpc_param::<CallInput>(params.sequence())?;
    let (_, block_selection) = next_rpc_param_default::<BlockSelection>(params)?;

    let point_in_time = ctx.storage.translate_to_point_in_time(&block_selection)?;
    match ctx.executor.call(call, point_in_time) {
        // success or failure, does not matter
        Ok(result) => Ok(hex_data(result.output)),

        // internal error
        Err(e) => {
            tracing::error!(reason = ?e, "failed to execute eth_call");
            Err(e.into())
        }
    }
}

fn eth_send_raw_transaction(params: Params, ctx: &RpcContext) -> Result<String, ErrorObjectOwned> {
    let (_, data) = next_rpc_param::<Bytes>(params.sequence())?;
    let transaction = parse_rpc_rlp::<TransactionInput>(&data)?;

    let hash = transaction.hash.clone();
    match ctx.executor.transact(transaction) {
        // result is success
        Ok(result) if result.is_success() => Ok(hex_data(hash)),

        // result is failure
        Ok(result) => Err(rpc_internal_error(hex_data(result.output))),

        // internal error
        Err(e) => {
            tracing::error!(reason = ?e, "failed to execute eth_sendRawTransaction");
            Err(e.into())
        }
    }
}

// Account

fn eth_get_balance(params: Params, ctx: &RpcContext) -> Result<String, ErrorObjectOwned> {
    let (params, address) = next_rpc_param::<Address>(params.sequence())?;
    let (_, block_selection) = next_rpc_param_default::<BlockSelection>(params)?;

    let point_in_time = ctx.storage.translate_to_point_in_time(&block_selection)?;
    let account = ctx.storage.read_account(&address, &point_in_time)?;

    Ok(hex_num(account.balance))
}

fn eth_get_code(params: Params, ctx: &RpcContext) -> Result<String, ErrorObjectOwned> {
    let (params, address) = next_rpc_param::<Address>(params.sequence())?;
    let (_, block_selection) = next_rpc_param_default::<BlockSelection>(params)?;

    let point_in_time = ctx.storage.translate_to_point_in_time(&block_selection)?;
    let account = ctx.storage.read_account(&address, &point_in_time)?;

    Ok(account.bytecode.map(hex_data).unwrap_or_else(hex_zero))
}

// Subscription

async fn eth_subscribe(params: Params<'_>, pending: PendingSubscriptionSink, ctx: Arc<RpcContext>) -> impl IntoSubscriptionCloseResponse {
    let (_, kind) = next_rpc_param::<String>(params.sequence())?;
    match kind.deref() {
        "newHeads" => ctx.subs.add_new_heads(pending.accept().await?),
        "logs" => ctx.subs.add_logs(pending.accept().await?),

        // unknown subscription kind
        kind => {
            tracing::warn!(%kind, "unsupported subscription kind");
            pending.reject(rpc_parsing_error(format!("unsupported subscription kind: {}", kind))).await;
        }
    }
    Ok(())
}

// -----------------------------------------------------------------------------
// Helpers
// -----------------------------------------------------------------------------
#[inline(always)]
fn hex_data<T: AsRef<[u8]>>(value: T) -> String {
    const_hex::encode_prefixed(value)
}

#[inline(always)]
fn hex_num(value: impl Into<U256>) -> String {
    format!("{:#x}", value.into())
}

fn hex_zero() -> String {
    "0x0".to_owned()
}
