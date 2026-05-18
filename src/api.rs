use anyhow::{Context, Result, anyhow};
use serde::Deserialize;
use tracing::{debug, info, instrument};

use crate::market::MarketCondition;

pub const MARKET_STATE_URL: &str = "https://dogonline.net/auction/api/getstate";

#[derive(Debug, Deserialize)]
pub struct MarketState {
    #[serde(rename = "marketConditions")]
    pub market_conditions: MarketConditionResponse,
}

#[derive(Debug, Deserialize)]
pub struct MarketConditionResponse {
    pub id: u64,
    pub message: String,
}

impl MarketState {
    #[instrument(skip(auth_token), err)]
    pub fn fetch(auth_token: &str) -> Result<Self> {
        let cookie = format!("__Secure-better-auth.session_token={auth_token}");

        let mut response = ureq::get(MARKET_STATE_URL)
            .header("Cookie", &cookie)
            .call()
            .context("request to market API failed")?;
        debug!(response = ?response, "fetched market status");

        let state: Self = response
            .body_mut()
            .read_json()
            .context("failed to decode market API response")?;

        info!(
            condition_message = %state.market_conditions.message,
            condition_id = %state.market_conditions.id,
            "fetched market state"
        );
        Ok(state)
    }

    pub fn market_condition(&self) -> Result<MarketCondition> {
        match self.market_conditions.id {
            1 => Ok(MarketCondition::InASlump),
            2 => Ok(MarketCondition::ChuggingAlong),
            3 => Ok(MarketCondition::LookingHot),
            4 => Ok(MarketCondition::HotHotHot),
            id => Err(anyhow!(format!(
                "failed to parse condition: {} ({})",
                id, self.market_conditions.message
            ))),
        }
    }
}
