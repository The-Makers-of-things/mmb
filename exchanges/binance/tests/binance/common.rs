use std::sync::Arc;

use anyhow::Result;
use binance::binance::BinanceBuilder;
use jsonrpc_core::Value;
use mmb_core::exchanges::common::{Amount, Price};
use mmb_core::exchanges::traits::ExchangeClientBuilder;
use mmb_utils::hashmap;
use mmb_utils::infrastructure::WithExpect;

use mmb_core::exchanges::hosts::Hosts;
use mmb_core::{
    exchanges::common::ExchangeId,
    exchanges::{
        common::ExchangeAccountId,
        timeouts::requests_timeout_manager_factory::RequestsTimeoutManagerFactory,
        timeouts::timeout_manager::TimeoutManager,
    },
    exchanges::{
        common::SpecificCurrencyPair,
        rest_client::{self, RestClient},
    },
    lifecycle::launcher::EngineBuildConfig,
};
use mmb_utils::value_to_decimal::GetOrErr;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

pub(crate) fn get_binance_credentials() -> Result<(String, String)> {
    let api_key = std::env::var("BINANCE_API_KEY");
    let api_key = match api_key {
        Ok(v) => v,
        Err(_) => {
            return Err(anyhow::Error::msg(
                "Environment variable BINANCE_API_KEY are not set. Unable to continue test",
            ));
        }
    };

    let secret_key = std::env::var("BINANCE_SECRET_KEY");
    let secret_key = match secret_key {
        Ok(v) => v,
        Err(_) => {
            return Err(anyhow::Error::msg(
                "Environment variable BINANCE_SECRET_KEY are not set. Unable to continue test",
            ));
        }
    };

    Ok((api_key, secret_key))
}

// Get data to access binance account
#[macro_export]
macro_rules! get_binance_credentials_or_exit {
    () => {{
        match crate::binance::common::get_binance_credentials() {
            Ok((api_key, secret_key)) => (api_key, secret_key),
            Err(error) => {
                dbg!("{:?}", error);
                return;
            }
        }
    }};
}

pub(crate) fn get_timeout_manager(exchange_account_id: ExchangeAccountId) -> Arc<TimeoutManager> {
    let engine_build_config =
        EngineBuildConfig::standard(Box::new(BinanceBuilder) as Box<dyn ExchangeClientBuilder>);
    let timeout_arguments = engine_build_config.supported_exchange_clients
        [&ExchangeId::new("Binance".into())]
        .get_timeout_arguments();
    let request_timeout_manager = RequestsTimeoutManagerFactory::from_requests_per_period(
        timeout_arguments,
        exchange_account_id,
    );

    TimeoutManager::new(hashmap![exchange_account_id => request_timeout_manager])
}

async fn send_request(
    hosts: &Hosts,
    api_key: &String,
    url_path: &str,
    http_params: &Vec<(String, String)>,
) -> String {
    let rest_client = RestClient::new();

    let full_url = rest_client::build_uri(&hosts.rest_host, url_path, http_params)
        .expect("build_uri is failed");

    rest_client
        .get(full_url, api_key)
        .await
        .with_expect(|| format!("failed to request {}", url_path))
        .content
}

/// Automatic price calculation for orders. This function gets the price from the middle of order book bids side.
/// This helps to avoid creating orders in the top of the order book.
pub(crate) async fn get_default_price(
    currency_pair: SpecificCurrencyPair,
    hosts: &Hosts,
    api_key: &String,
) -> Price {
    #[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
    struct OrderBook {
        pub bids: Vec<(Decimal, Decimal)>,
    }

    let data = send_request(
        hosts,
        api_key,
        "/api/v3/depth",
        &vec![("symbol".to_owned(), currency_pair.as_str().to_owned())],
    )
    .await;

    let value: OrderBook = serde_json::from_str(data.as_str())
        .with_expect(|| format!("failed to deserialize data: {}", data));

    // getting price for order from the middle of the order book
    // use bids because this price is little lower then asks
    value
        .bids
        .get(value.bids.len() / 2)
        .expect("failed to get bid from the middle of the order book")
        .clone()
        .0
}

/// Automatic amount calculation for orders. This function calculate the amount for price and MIN_NOTIONAL filter.
pub(crate) async fn get_min_amount(
    currency_pair: SpecificCurrencyPair,
    hosts: &Hosts,
    api_key: &String,
    price: Price,
) -> Amount {
    let data = send_request(
        hosts,
        api_key,
        "/api/v3/exchangeInfo",
        &vec![("symbol".to_owned(), currency_pair.as_str().to_owned())],
    )
    .await;

    let value: Value = serde_json::from_str(data.as_str())
        .with_expect(|| format!("failed to deserialize data: {}", data));

    let filters = value
        .pointer("/symbols/0/filters")
        .expect("Failed to get filters")
        .as_array()
        .expect("/symbols/0/filters isn't an array");

    let min_notional_filter = filters
        .iter()
        .find(|value| {
            value["filterType"]
                .as_str()
                .expect("Failed to get filterType")
                == "MIN_NOTIONAL"
        })
        .expect("Failed to get min_notional_filter");

    let min_notional = min_notional_filter
        .get_as_decimal("minNotional")
        .expect("Failed to get min_notional");

    (min_notional / price).ceil()
}
