use anyhow::{Context, Result};
use itertools::Itertools;
use mmb_lib::core::exchanges::common::{Amount, CurrencyPair, ExchangeAccountId};
use mmb_lib::core::lifecycle::launcher::{launch_trading_engine, EngineBuildConfig, InitSettings};
use mmb_lib::core::settings::{
    AppSettings, BaseStrategySettings, CoreSettings, CurrencyPairSetting, ExchangeSettings,
};
use rust_decimal_macros::dec;
use std::env;
use std::fs;

macro_rules! get_binance_credentials_or_exit {
    () => {{
        let api_key = std::env::var("BINANCE_API_KEY");
        let api_key = match api_key {
            Ok(v) => v,
            Err(_) => {
                dbg!("Environment variable BINANCE_API_KEY are not set. Unable to continue test");
                return Ok(());
            }
        };

        let secret_key = std::env::var("BINANCE_SECRET_KEY");
        let secret_key = match secret_key {
            Ok(v) => v,
            Err(_) => {
                dbg!("Environment variable BINANCE_SECRET_KEY are not set. Unable to continue test");
                return Ok(());
            }
        };

        (api_key, secret_key)
    }};
}

#[derive(Default, Clone)]
pub struct ExampleStrategySettings {}

impl BaseStrategySettings for ExampleStrategySettings {
    fn exchange_account_id(&self) -> ExchangeAccountId {
        "Binance0"
            .parse()
            .expect("Binance should be specified for example strategy")
    }

    fn currency_pair(&self) -> CurrencyPair {
        CurrencyPair::from_codes("eos".into(), "btc".into())
    }

    fn max_amount(&self) -> Amount {
        dec!(1)
    }
}

#[allow(dead_code)]
#[actix_web::main]
async fn main() -> Result<()> {
    let (api_key, secret_key) = get_binance_credentials_or_exit!();

    let engine_config = EngineBuildConfig::standard();

    //let app_settings = AppSettings::<ExampleStrategySettings> {
    //    strategy: ExampleStrategySettings::default(),
    //    core: CoreSettings {
    //        exchanges: vec![ExchangeSettings {
    //            exchange_account_id: "Binance0".parse().expect("It should be valid format"),
    //            api_key,
    //            secret_key,
    //            is_margin_trading: false,
    //            currency_pairs: Some(vec![
    //                CurrencyPairSetting {
    //                    base: "phb".into(),
    //                    quote: "btc".into(),
    //                    currency_pair: None,
    //                },
    //                CurrencyPairSetting {
    //                    base: "eth".into(),
    //                    quote: "btc".into(),
    //                    currency_pair: None,
    //                },
    //                CurrencyPairSetting {
    //                    base: "eos".into(),
    //                    quote: "btc".into(),
    //                    currency_pair: None,
    //                },
    //            ]),
    //            websocket_channels: vec!["depth20"] // vec!["trade", "depth"]
    //                .into_iter()
    //                .map(|x| x.into())
    //                .collect_vec(),
    //            web_socket_host: "".to_string(),
    //            web_socket2_host: "".to_string(),
    //            rest_host: "".to_string(),
    //            subscribe_to_market_data: true,
    //        }],
    //    },
    //};
    // FIXME remove comments
    //let init_settings = InitSettings::Directly(app_settings);
    //let engine = launch_trading_engine(&engine_config, init_settings).await;

    //// let ctx = engine.context();
    //// let _ = tokio::spawn(async move {
    ////     tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    ////     ctx.application_manager
    ////         .clone()
    ////         .spawn_graceful_shutdown("test".to_owned());
    //// });

    let core_settings = CoreSettings {
        exchanges: vec![ExchangeSettings {
            exchange_account_id: "Binance0".parse().expect("It should be valid format"),
            api_key,
            secret_key,
            is_margin_trading: false,
            currency_pairs: Some(vec![
                CurrencyPairSetting {
                    base: "phb".into(),
                    quote: "btc".into(),
                    currency_pair: None,
                },
                CurrencyPairSetting {
                    base: "eth".into(),
                    quote: "btc".into(),
                    currency_pair: None,
                },
                CurrencyPairSetting {
                    base: "eos".into(),
                    quote: "btc".into(),
                    currency_pair: None,
                },
            ]),
            websocket_channels: vec!["depth20"] // vec!["trade", "depth"]
                .into_iter()
                .map(|x| x.into())
                .collect_vec(),
            web_socket_host: "".to_string(),
            web_socket2_host: "".to_string(),
            rest_host: "".to_string(),
            subscribe_to_market_data: true,
        }],
    };

    let config = fs::read_to_string("config.toml").context("Unable to parse file")?;
    dbg!(&config);

    let decoded: CoreSettings =
        toml::from_str(&config).context("Unable to deserialize to CoreSettings")?;
    dbg!(&decoded);

    assert_eq!(decoded, core_settings);

    Ok(())
}
