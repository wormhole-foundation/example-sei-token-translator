#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;

use anyhow::Context;
use cosmwasm_std::{DepsMut, Empty, Env, MessageInfo, Response, StdError, Binary, Coin};
use cw2::{get_contract_version, set_contract_version};
use semver::Version;

use crate::msg::ExecuteMsg;

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:sei-token-translator";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    _msg: Empty,
) -> Result<Response, anyhow::Error> {
    // save the contract name and version
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)
        .context("failed to set contract version")?;

    Ok(Response::new()
        .add_attribute("action", "instantiate")
        .add_attribute("owner", info.sender)
        .add_attribute("version", CONTRACT_VERSION))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: Empty) -> Result<Response, anyhow::Error> {
    let ver = get_contract_version(deps.storage)?;
    // ensure we are migrating from an allowed contract
    if ver.contract != CONTRACT_NAME {
        return Err(StdError::generic_err("Can only upgrade from same type").into());
    }

    // ensure we are migrating to a newer version
    let saved_version =
        Version::parse(&ver.version).context("could not parse saved contract version")?;
    let new_version =
        Version::parse(CONTRACT_VERSION).context("could not parse new contract version")?;
    if saved_version >= new_version {
        return Err(StdError::generic_err("Cannot upgrade from a newer or equal version").into());
    }

    // set the new version
    cw2::set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

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
        ExecuteMsg::CompleteTransferAndConvert { vaa } => complete_transfer_and_convert(vaa),
        ExecuteMsg::ConvertAndTransfer { coins } => convert_and_transfer(coins),
        ExecuteMsg::ConvertBankToCw20 { coins } => convert_bank_to_cw20(coins),
        ExecuteMsg::ConvertCw20ToBank { coins } => convert_cw20_to_bank(coins)
    }
}

fn complete_transfer_and_convert(vaa: Binary) -> Result<Response, anyhow::Error> {

}

fn convert_and_transfer(coins: Coin) -> Result<Response, anyhow::Error> {
    
}

fn convert_bank_to_cw20(coins: Coin) -> Result<Response, anyhow::Error> {

}

fn convert_cw20_to_bank(coins: Coin) -> Result<Response, anyhow::Error> {

}