use cw_storage_plus::{Map, Item};

pub const TOKEN_BRIDGE_CONTRACT: Item<String> = Item::new("token_bridge_contract");

// Maps cw20 address -> bank token denom
pub const CW_DENOMS: Map<String, String> = Map::new("cw_denoms");
