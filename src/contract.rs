#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;

use anyhow::{ensure, Context};
use cosmwasm_std::{
    coin, to_binary, BankMsg, Binary, Coin, CosmosMsg, DepsMut, Empty, Env, MessageInfo,
    QueryRequest, Reply, Response, SubMsg, Uint128, WasmQuery,
};
use cw20::Cw20ReceiveMsg;
use sei_cosmwasm::SeiMsg;
use token_bridge_terra_2::msg::{
    ExecuteMsg as TokenBridgeExecuteMsg, QueryMsg as TokenBridgeQueryMsg, TransferInfoResponse,
};

use crate::{
    msg::{ExecuteMsg, InstantiateMsg, ReceiveAction},
    state::{CW_DENOMS, TOKEN_BRIDGE_CONTRACT},
};

const COMPLETE_TRANSFER_REPLY_ID: u64 = 1;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, anyhow::Error> {
    TOKEN_BRIDGE_CONTRACT
        .save(deps.storage, &msg.token_bridge_contract)
        .context("failed to save token_bridge_contract to storage")?;

    Ok(Response::new()
        .add_attribute("action", "instantiate")
        .add_attribute("owner", info.sender))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: Empty) -> Result<Response, anyhow::Error> {
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, anyhow::Error> {
    match msg {
        ExecuteMsg::CompleteTransferAndConvert { vaa } => {
            complete_transfer_and_convert(deps, info, vaa)
        }
        ExecuteMsg::ConvertAndTransfer { coins } => convert_and_transfer(coins),
        ExecuteMsg::ConvertBankToCw20 { coins } => convert_bank_to_cw20(coins),
        ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender,
            amount,
            msg,
        }) => handle_receiver_msg(deps, info, sender, amount, msg),
    }
}

/// Reply handler for various kinds of replies
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, env: Env, msg: Reply) -> Result<Response<SeiMsg>, anyhow::Error> {
    // handle submessage cases based on the reply id
    if msg.id == COMPLETE_TRANSFER_REPLY_ID {
        return handle_complete_transfer_reply(deps, env, msg);
    }

    // other cases probably from calling into the sei burn/mint messages and token factory methods

    Ok(Response::default())
}

fn handle_complete_transfer_reply(
    deps: DepsMut,
    env: Env,
    msg: Reply,
) -> Result<Response<SeiMsg>, anyhow::Error> {
    // we should only be replying on success
    ensure!(
        msg.result.is_ok(),
        "msg result is not okay, we should never get here"
    );

    let res = msg.result.unwrap();

    // find the wasm event and get the attributes
    // we need the contract address, recipient, and amount
    let wasm_event =
        res.events.iter().find(|e| e.ty == "wasm").context(
            "wasm event not included in token bridge response, we should never get here",
        )?;
    let wasm_event_iter = wasm_event.attributes.iter();
    let contract_addr = wasm_event_iter
        .find(|a| a.key == "contract")
        .map(|a| a.value)
        .context("contract attribute not found in wasm event, we should never get here")?;
    let recipient = wasm_event_iter
        .find(|a| a.key == "recipient")
        .map(|a| a.value)
        .context("recipient attribute not found in wasm event, we should never get here")?;
    let amount = wasm_event_iter
        .find(|a| a.key == "amount")
        .map(|a| a.value)
        .context("amount attribute not found in wasm event, we should never get here")?
        .parse::<u128>()
        .context("could not parse amount string to u128, we should never get here")?;

    // TODO: increment the number of newly minted cw20 tokens that we have -- do we need to do this??

    let response: Response<SeiMsg> = Response::new();

    // check CW_DENOMS to see if the denom exists
    // add call into token factory create denom if it doesn't exist
    if !CW_DENOMS.has(deps.storage, contract_addr) {
        response.add_message(SeiMsg::CreateDenom {
            subdenom: contract_addr,
        });
    }

    // format the amount using the proper token factory denom
    let tokenfactory_denom = "factory/".to_string()
        + env.contract.address.to_string().as_ref()
        + "/"
        + contract_addr.as_ref();
    let amount = coin(amount, tokenfactory_denom);

    // add calls to mint and send bank tokens
    response.add_message(SeiMsg::MintTokens { amount });
    response.add_message(BankMsg::Send {
        to_address: recipient,
        amount: vec![amount],
    });

    Ok(response)
}

/// Calls into the wormhole token bridge to complete the payload3 transfer.
fn complete_transfer_and_convert(
    deps: DepsMut,
    info: MessageInfo,
    vaa: Binary,
) -> Result<Response, anyhow::Error> {
    // get the token bridge contract address from storage
    let token_bridge_contract = TOKEN_BRIDGE_CONTRACT
        .load(deps.storage)
        .context("could not load token_bridge_contract")?;

    // craft the token bridge execute message
    // this will be added as a submessage to the response
    let token_bridge_execute_msg = to_binary(&TokenBridgeExecuteMsg::CompleteTransferWithPayload {
        data: vaa,
        relayer: info.sender.to_string(),
    })
    .context("could not serialize token bridge execute msg")?;

    let sub_msg = SubMsg::reply_on_success(
        CosmosMsg::Wasm(cosmwasm_std::WasmMsg::Execute {
            contract_addr: token_bridge_contract,
            msg: token_bridge_execute_msg,
            funds: vec![],
        }),
        COMPLETE_TRANSFER_REPLY_ID,
    );

    // craft the token bridge query message to parse the payload3 vaa
    let token_bridge_query_msg = to_binary(&TokenBridgeQueryMsg::TransferInfo { vaa: vaa })
        .context("could not serialize token bridge transfer_info query msg")?;

    let transfer_info: TransferInfoResponse = deps
        .querier
        .query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: token_bridge_contract,
            msg: token_bridge_query_msg,
        }))
        .context("could not parse token bridge payload3 vaa")?;

    // return the response which will callback to the reply handler on success
    Ok(Response::new()
        .add_submessage(sub_msg)
        .add_attribute("action", "complete_transfer_with_payload")
        .add_attribute(
            "transfer_payload",
            Binary::from(transfer_info.payload).to_base64(),
        ))
}

fn convert_and_transfer(coins: Coin) -> Result<Response, anyhow::Error> {
    // receive bank token -- how to do this??
    // check contract storage to see if this denom has corresponding locked cw20 tokens and valid amount of these tokens
    // call into seimsg::burn for the bank tokens
    // unlock cw20 tokens, send to the token bridge from this contract -- do we need approval to do this?? Can batch these together if necessary.
}

fn convert_bank_to_cw20(coins: Coin) -> Result<Response, anyhow::Error> {
    // receive bank token -- how to do this??
    // check contract storage to see if this denom has corresponding locked cw20 tokens and valid amount of these tokens
    // call into seimsg::burn for the bank tokens
    // unlock cw20 tokens, use cw20::transfer to send back to the msg.sender
}

fn handle_receiver_msg(
    deps: DepsMut,
    info: MessageInfo,
    sender: String,
    amount: Uint128,
    msg: Binary,
) -> Result<Response, anyhow::Error> {
    // deserialize msg and match cases:
    // (1) ConvertToBank -- call into convert_cw20_to_bank
    let receive_action: ReceiveAction =
        serde_json::from_slice(msg.as_slice()).context("could not parse receive action payload")?;
    match receive_action {
        ReceiveAction::ConvertToBank => convert_cw20_to_bank(deps, info, sender, amount),
    }
}

fn convert_cw20_to_bank(
    deps: DepsMut,
    info: MessageInfo,
    sender: String,
    amount: Uint128,
) -> Result<Response, anyhow::Error> {
    // check contract storage see if we've created a denom for this cw20 token yet
    // if we haven't created the denom, then create the denom
    // info.sender contains the cw20 contract address
    let has_created_denom = CW_DENOMS
        .load(&deps.storage, info.sender.to_string())
        .is_ok();
    if !has_created_denom {
        // call into token factory to create the denom
    }

    // otherwise we get the right denom for this cw20 token from contract storage.
    // then we can lock the cw20 token and then call seimsg::mint for the amount
}
