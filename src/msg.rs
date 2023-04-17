// 1. completeTransferAndConvert(VAA)
// 2. convertAndTransfer(bankTokens)
// 3. convertToBank(cw20Tokens)
// 4. convertToCW20(bankTokens)

use cosmwasm_std::{Binary, Coin};
use cw20::Cw20ReceiveMsg;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct InstantiateMsg {
    pub token_bridge_contract: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    /// Submit a VAA to complete a wormhole payload3 token bridge transfer.
    /// This function will:
    /// 1. complete the wormhole token bridge transfer.
    /// 2. Lock the newly minted cw20 tokens.
    /// 3. CreateDenom (if it doesn't already exist)
    /// 4. Mint an equivalent amount of bank tokens using the token factory.
    /// 5. Send the minted bank tokens to the destination address.
    CompleteTransferAndConvert {
        /// VAA to submit. The VAA should be encoded in the standard wormhole
        /// wire format.
        vaa: Binary,
    },

    /// Convert bank tokens into the equivalent (locked) cw20 tokens and trigger a wormhole token bridge transfer.
    /// This function will:
    /// 1. Validate that the bank tokens originated from cw20 tokens that are locked in this contract.
    /// 2. Burn the bank tokens using the token factory.
    /// 3. Unlock the equivalent cw20 tokens.
    /// 4. Cross-call into the wormhole token bridge to initiate a cross-chain transfer.
    ConvertAndTransfer { coins: Coin },

    /// Convert bank tokens into cw20 tokens using the token factory.
    /// This function will:
    /// 1. Validate that the bank tokens originated from cw20 tokens that are locked in this contract.
    /// 2. Burn the bank tokens using the token factory.
    /// 3. Unlock the equivalent cw20 tokens.
    /// 4. Send the unlocked cw20 tokens back to the caller.
    ConvertBankToCw20 { coins: Coin },

    /// Implements the CW20 receiver interface to recieve cw20 tokens and act on them.
    /// Cw20ReceiveMsg.msg will be deserialized into the ReceiveAction type.
    Receive(Cw20ReceiveMsg),
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ReceiveAction {
    /// Action that specifies to convert cw20 tokens into bank tokens using the token factory.
    /// This action will:
    /// 1. Lock the cw20 tokens.
    /// 2. Mint an equivalent amount of bank tokens using the token factory.
    /// 3. Send the minted bank tokens back to the caller.
    ConvertToBank,
}
