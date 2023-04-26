use cw_storage_plus::{Item, Map};
use cw_token_bridge::msg::TransferInfoResponse;

pub const TOKEN_BRIDGE_CONTRACT: Item<String> = Item::new("token_bridge_contract");
pub const WORMHOLE_CONTRACT: Item<String> = Item::new("wormhole_contract");

// Holds temp state for the wormhole message that the contract is currently processing
pub const CURRENT_TRANSFER: Item<TransferInfoResponse> = Item::new("current_transfer");

// Maps cw20 address -> bank token denom
pub const CW_DENOMS: Map<String, String> = Map::new("cw_denoms");
