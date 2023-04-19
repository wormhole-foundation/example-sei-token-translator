#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;

use anyhow::{ensure, Context};
use cosmwasm_std::{
    coin, to_binary, BankMsg, Binary, Coin, CosmosMsg, DepsMut, Empty, Env, MessageInfo,
    QueryRequest, Reply, Response, SubMsg, Uint128, WasmMsg, WasmQuery,
};
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};
use cw_token_bridge::msg::{
    ExecuteMsg as TokenBridgeExecuteMsg, QueryMsg as TokenBridgeQueryMsg, TransferInfoResponse,
};
use sei_cosmwasm::SeiMsg;

use cw_wormhole::msg::{GetStateResponse, QueryMsg as WormholeQueryMsg};

use cw20_wrapped_2::msg::ExecuteMsg as Cw20WrappedExecuteMsg;
use terraswap::asset::{Asset, AssetInfo};

use crate::{
    msg::{ExecuteMsg, InstantiateMsg, ReceiveAction},
    state::{CW_DENOMS, TOKEN_BRIDGE_CONTRACT, WORMHOLE_CONTRACT},
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
        .context("failed to save token bridge contract address to storage")?;

    WORMHOLE_CONTRACT
        .save(deps.storage, &msg.wormhole_contract)
        .context("failed to save wormhole contract address to storage")?;

    Ok(Response::new()
        .add_attribute("action", "instantiate")
        .add_attribute("owner", info.sender))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: Empty) -> Result<Response, anyhow::Error> {
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response<SeiMsg>, anyhow::Error> {
    match msg {
        ExecuteMsg::CompleteTransferAndConvert { vaa } => {
            complete_transfer_and_convert(deps, info, vaa)
        }
        ExecuteMsg::ConvertAndTransfer {
            recipient_chain,
            recipient,
            fee,
        } => convert_and_transfer(deps, info, env, recipient_chain, recipient, fee),
        ExecuteMsg::ConvertBankToCw20 => convert_bank_to_cw20(deps, info, env),
        ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender,
            amount,
            msg,
        }) => handle_receiver_msg(deps, info, env, sender, amount, msg),
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
    let mut wasm_event_iter = wasm_event.attributes.iter();
    let contract_addr = wasm_event_iter
        .find(|a| a.key == "contract")
        .map(|a| a.value.clone())
        .context("contract attribute not found in wasm event, we should never get here")?;
    let recipient = wasm_event_iter
        .find(|a| a.key == "recipient")
        .map(|a| a.value.clone())
        .context("recipient attribute not found in wasm event, we should never get here")?;
    let amount = wasm_event_iter
        .find(|a| a.key == "amount")
        .map(|a| a.value.clone())
        .context("amount attribute not found in wasm event, we should never get here")?
        .parse::<u128>()
        .context("could not parse amount string to u128, we should never get here")?;

    return convert_cw20_to_bank(deps, env, recipient, amount, contract_addr);
}

/// Calls into the wormhole token bridge to complete the payload3 transfer.
fn complete_transfer_and_convert(
    deps: DepsMut,
    info: MessageInfo,
    vaa: Binary,
) -> Result<Response<SeiMsg>, anyhow::Error> {
    // get the token bridge contract address from storage
    let token_bridge_contract = TOKEN_BRIDGE_CONTRACT
        .load(deps.storage)
        .context("could not load token bridge contract address")?;

    // craft the token bridge execute message
    // this will be added as a submessage to the response
    let token_bridge_execute_msg = to_binary(&TokenBridgeExecuteMsg::CompleteTransferWithPayload {
        data: vaa.clone(),
        relayer: info.sender.to_string(),
    })
    .context("could not serialize token bridge execute msg")?;

    let sub_msg = SubMsg::reply_on_success(
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: token_bridge_contract.clone(),
            msg: token_bridge_execute_msg,
            funds: vec![],
        }),
        COMPLETE_TRANSFER_REPLY_ID,
    );

    // craft the token bridge query message to parse the payload3 vaa
    let token_bridge_query_msg = to_binary(&TokenBridgeQueryMsg::TransferInfo { vaa })
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

fn convert_and_transfer(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    recipient_chain: u16,
    recipient: Binary,
    fee: Uint128,
) -> Result<Response<SeiMsg>, anyhow::Error> {
    // bank tokens sent to the contract will be in info.funds
    ensure!(
        info.funds.len() == 2,
        "info.funds should contain 2 coins: 1 for bridging and another for the wormhole fee"
    );

    // get the wormhole contract address from storage
    let wormhole_contract = WORMHOLE_CONTRACT
        .load(deps.storage)
        .context("could not load wormhole contract address")?;

    // load the token bridge contract address
    let token_bridge_contract = TOKEN_BRIDGE_CONTRACT
        .load(deps.storage)
        .context("could not load token bridge contract address")?;

    // check wormhole fee token and use the token that's not the wormhole fee token
    let wormhole_query_msg = to_binary(&WormholeQueryMsg::GetState {})
        .context("could not serialize wormhole get_state query msg")?;
    let wormhole_info: GetStateResponse = deps
        .querier
        .query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: wormhole_contract,
            msg: wormhole_query_msg,
        }))
        .context("could not query wormhole state")?;

    let wormhole_fee_coin = info
        .funds
        .iter()
        .find(|c| c.denom == wormhole_info.fee.denom)
        .context("wormhole fee token not included in info.funds")?;
    let bridging_coin = info
        .funds
        .iter()
        .find(|c| c.denom != wormhole_info.fee.denom)
        .context("coin to bridge not included in info.funds")?;

    let cw20_contract_addr = parse_bank_token_factory_contract(deps, env, bridging_coin.clone())?;

    // batch calls together
    let mut response: Response<SeiMsg> = Response::new();

    // 1. seimsg::burn for the bank tokens
    response = response.add_message(SeiMsg::BurnTokens {
        amount: bridging_coin.clone(),
    });

    // 2. cw20::increaseAllowance to the contract address for the token bridge to spend the amount of tokens
    let increase_allowance_msg = to_binary(&Cw20WrappedExecuteMsg::IncreaseAllowance {
        spender: token_bridge_contract.clone(),
        amount: bridging_coin.amount,
        expires: None,
    })
    .context("could not serialize cw20 increase_allowance msg")?;
    response = response.add_message(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: cw20_contract_addr.clone(),
        msg: increase_allowance_msg,
        funds: vec![],
    }));

    // 3. token_bridge::initiate_transfer -- the cw20 tokens will be either burned or transferred to the token_bridge
    let initiate_transfer_msg = to_binary(&TokenBridgeExecuteMsg::InitiateTransfer {
        asset: Asset {
            info: AssetInfo::Token {
                contract_addr: cw20_contract_addr,
            },
            amount: bridging_coin.amount,
        },
        recipient_chain,
        recipient,
        fee,
        nonce: 0,
    })
    .context("could not serialize token bridge initiate_transfer msg")?;
    response = response.add_message(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: token_bridge_contract,
        msg: initiate_transfer_msg,
        funds: vec![wormhole_fee_coin.clone()],
    }));

    Ok(response)
}

fn convert_bank_to_cw20(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
) -> Result<Response<SeiMsg>, anyhow::Error> {
    // bank tokens sent to the contract will be in info.funds
    ensure!(
        info.funds.len() == 1,
        "info.funds should contain only 1 coin"
    );

    let converting_coin = info.funds[0].clone();
    let cw20_contract_addr = parse_bank_token_factory_contract(deps, env, converting_coin.clone())?;

    // batch calls together
    let mut response: Response<SeiMsg> = Response::new();

    // 1. seimsg::burn for the bank tokens
    response = response.add_message(SeiMsg::BurnTokens {
        amount: converting_coin.clone(),
    });

    // 2. cw20::transfer to send back to the msg.sender
    let transfer_msg = to_binary(&Cw20ExecuteMsg::Transfer {
        recipient: info.sender.to_string(),
        amount: converting_coin.amount,
    })
    .context("could not serialize cw20::transfer msg")?;
    response = response.add_message(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: cw20_contract_addr,
        msg: transfer_msg,
        funds: vec![],
    }));

    Ok(response)
}

fn parse_bank_token_factory_contract(
    deps: DepsMut,
    env: Env,
    coin: Coin,
) -> Result<String, anyhow::Error> {
    // extract the contract address from the denom of the token that was sent to us
    // if the token is not a factory token created by this contract, return error
    let parsed_denom = coin.denom.split("/").collect::<Vec<_>>();
    ensure!(
        parsed_denom.len() == 3
            && parsed_denom[0] == "factory"
            && parsed_denom[1] == env.contract.address.to_string(),
        "coin is not from the token factory"
    );
    let cw20_contract_addr = parsed_denom[3].to_string();

    // validate that the contract does indeed match the stored denom we have for it
    let stored_denom = CW_DENOMS
        .load(deps.storage, cw20_contract_addr.clone())
        .context(
            "a corresponding denom for the extracted contract addr is not contained in storage",
        )?;
    ensure!(
        stored_denom == coin.denom,
        "the stored denom for the contract does not match the actual coin denom"
    );

    Ok(cw20_contract_addr)
}

fn handle_receiver_msg(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    sender: String,
    amount: Uint128,
    msg: Binary,
) -> Result<Response<SeiMsg>, anyhow::Error> {
    // deserialize msg and match cases:
    // (1) ConvertToBank -- call into convert_cw20_to_bank
    let receive_action: ReceiveAction = serde_json_wasm::from_slice(msg.as_slice())
        .context("could not parse receive action payload")?;
    match receive_action {
        ReceiveAction::ConvertToBank => {
            convert_cw20_to_bank(deps, env, sender, amount.u128(), info.sender.into_string())
        }
    }
}

fn convert_cw20_to_bank(
    deps: DepsMut,
    env: Env,
    recipient: String,
    amount: u128,
    contract_addr: String,
) -> Result<Response<SeiMsg>, anyhow::Error> {
    // TODO: increment the number of newly minted cw20 tokens that we have -- do we need to do this??

    let mut response: Response<SeiMsg> = Response::new();

    // check contract storage see if we've created a denom for this cw20 token yet
    // if we haven't created the denom, then create the denom
    // info.sender contains the cw20 contract address
    if !CW_DENOMS.has(deps.storage, contract_addr.clone()) {
        // call into token factory to create the denom
        response = response.add_message(SeiMsg::CreateDenom {
            subdenom: contract_addr.clone(),
        });
    }

    // format the amount using the proper token factory denom
    let tokenfactory_denom = "factory/".to_string()
        + env.contract.address.to_string().as_ref()
        + "/"
        + contract_addr.as_ref();
    let amount = coin(amount, tokenfactory_denom);

    // add calls to mint and send bank tokens
    response = response.add_message(SeiMsg::MintTokens {
        amount: amount.clone(),
    });
    response = response.add_message(BankMsg::Send {
        to_address: recipient,
        amount: vec![amount],
    });

    Ok(response)
}
