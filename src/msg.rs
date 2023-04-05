// 1. completeTransferAndConvert(VAA)
// 2. convertAndTransfer(bankTokens)
// 3. convertToBank(cw20Tokens)
// 4. convertToCW20(bankTokens)

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Binary, Coin};


#[cw_serde]
pub enum ExecuteMsg {
    /// Submit a VAA to complete a wormhole payload3 token bridge transfer.
    /// This function will:
    /// 1. complete the wormhole token bridge transfer.
    /// 2. Lock the newly minted cw20 tokens.
    /// 3. Mint an equivalent amount of bank tokens using the token factory.
    /// 4. Send the minted bank tokens to the destination address.
    CompleteTransferAndConvert {
        /// VAA to submit. The VAA should be encoded in the standard wormhole
        /// wire format.
        vaa: Binary
    },

    /// Convert bank tokens into the equivalent (locked) cw20 tokens and trigger a wormhole token bridge transfer.
    /// This function will:
    /// 1. Validate that the bank tokens originated from cw20 tokens that are locked in this contract.
    /// 2. Burn the bank tokens using the token factory.
    /// 3. Unlock the equivalent cw20 tokens.
    /// 4. Cross-call into the wormhole token bridge to initiate a cross-chain transfer.
    ConvertAndTransfer {
        coins: Coin
    },

    /// Convert cw20 tokens into bank tokens using the token factory.
    /// This function will:
    /// 1. Lock the cw20 tokens.
    /// 2. Mint an equivalent amount of bank tokens using the token factory.
    /// 3. Send the minted bank tokens back to the caller.
    ConvertCw20ToBank {
        coins: Coin
    },

    /// Convert bank tokens into cw20 tokens using the token factory.
    /// This function will:
    /// 1. Validate that the bank tokens originated from cw20 tokens that are locked in this contract.
    /// 2. Burn the bank tokens using the token factory.
    /// 3. Unlock the equivalent cw20 tokens.
    /// 4. Send the unlocked cw20 tokens back to the caller.
    ConvertBankToCw20 {
        coins: Coin
    }
}