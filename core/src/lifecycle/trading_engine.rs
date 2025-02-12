use futures::FutureExt;
use mmb_utils::send_expected::SendExpected;
use std::panic;
use std::panic::AssertUnwindSafe;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use anyhow::Result;
use dashmap::DashMap;
use futures::future::join_all;
use itertools::Itertools;
use mmb_utils::cancellation_token::CancellationToken;
use tokio::sync::{broadcast, oneshot};
use tokio::time::Duration;

use crate::balance_manager::balance_manager::BalanceManager;
use crate::exchanges::block_reasons;
use crate::exchanges::common::ExchangeAccountId;
use crate::exchanges::events::{ExchangeEvent, ExchangeEvents};
use crate::exchanges::exchange_blocker::BlockType;
use crate::exchanges::exchange_blocker::ExchangeBlocker;
use crate::exchanges::general::exchange::Exchange;
use crate::exchanges::timeouts::timeout_manager::TimeoutManager;
use crate::lifecycle::shutdown::ShutdownService;
use crate::settings::CoreSettings;
use crate::{
    infrastructure::unset_application_manager, lifecycle::application_manager::ApplicationManager,
};
use parking_lot::Mutex;

use super::launcher::unwrap_or_handle_panic;

pub trait Service: Send + Sync + 'static {
    fn name(&self) -> &str;

    fn graceful_shutdown(self: Arc<Self>) -> Option<oneshot::Receiver<Result<()>>>;
}

pub struct EngineContext {
    pub app_settings: CoreSettings,
    pub exchanges: DashMap<ExchangeAccountId, Arc<Exchange>>,
    pub shutdown_service: Arc<ShutdownService>,
    pub exchange_blocker: Arc<ExchangeBlocker>,
    pub application_manager: Arc<ApplicationManager>,
    pub timeout_manager: Arc<TimeoutManager>,
    pub balance_manager: Arc<Mutex<BalanceManager>>,
    is_graceful_shutdown_started: AtomicBool,
    exchange_events: ExchangeEvents,
    finish_graceful_shutdown_sender: Mutex<Option<oneshot::Sender<()>>>,
}

impl EngineContext {
    pub(crate) fn new(
        app_settings: CoreSettings,
        exchanges: DashMap<ExchangeAccountId, Arc<Exchange>>,
        exchange_events: ExchangeEvents,
        finish_graceful_shutdown_sender: oneshot::Sender<()>,
        timeout_manager: Arc<TimeoutManager>,
        application_manager: Arc<ApplicationManager>,
        balance_manager: Arc<Mutex<BalanceManager>>,
    ) -> Arc<Self> {
        let exchange_account_ids = app_settings
            .exchanges
            .iter()
            .map(|x| x.exchange_account_id)
            .collect_vec();

        let engine_context = Arc::new(EngineContext {
            app_settings,
            exchanges,
            shutdown_service: Default::default(),
            exchange_blocker: ExchangeBlocker::new(exchange_account_ids),
            application_manager: application_manager.clone(),
            timeout_manager,
            balance_manager,
            is_graceful_shutdown_started: Default::default(),
            exchange_events,
            finish_graceful_shutdown_sender: Mutex::new(Some(finish_graceful_shutdown_sender)),
        });

        application_manager.setup_engine_context(engine_context.clone());

        engine_context
    }

    pub(crate) async fn graceful_shutdown(self: Arc<Self>) {
        if self
            .is_graceful_shutdown_started
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return;
        }

        log::info!("Graceful shutdown started");

        self.exchanges.iter().for_each(|x| {
            self.exchange_blocker.block(
                x.exchange_account_id,
                block_reasons::GRACEFUL_SHUTDOWN,
                BlockType::Manual,
            )
        });

        self.application_manager.stop_token().cancel();

        self.shutdown_service.graceful_shutdown().await;
        self.exchange_blocker.stop_blocker().await;

        let cancellation_token = CancellationToken::default();
        const TIMEOUT: Duration = Duration::from_secs(5);

        tokio::select! {
            _ = cancel_opened_orders(&self.exchanges, cancellation_token.clone(), true) => (),
            _ = tokio::time::sleep(TIMEOUT) => {
                cancellation_token.cancel();
                log::error!(
                    "Timeout {} secs is exceeded: cancel open orders has been stopped",
                    TIMEOUT.as_secs(),
                );
            }
        }

        let disconnect_websockets = self
            .exchanges
            .iter()
            .map(|exchange| exchange.clone().disconnect());
        join_all(disconnect_websockets).await;

        self.finish_graceful_shutdown_sender
            .lock()
            .take()
            .expect("'finish_graceful_shutdown_sender' should exists in EngineContext")
            .send_expected(());

        unset_application_manager();

        log::info!("Graceful shutdown finished");
    }

    pub fn get_events_channel(&self) -> broadcast::Receiver<ExchangeEvent> {
        self.exchange_events.get_events_channel()
    }
}

async fn cancel_opened_orders(
    exchanges: &DashMap<ExchangeAccountId, Arc<Exchange>>,
    cancellation_token: CancellationToken,
    add_missing_open_orders: bool,
) {
    log::info!("Canceling opened orders started");

    join_all(exchanges.iter().map(|x| {
        x.clone()
            .cancel_opened_orders(cancellation_token.clone(), add_missing_open_orders)
    }))
    .await;

    log::info!("Canceling opened orders finished");
}

pub struct TradingEngine {
    context: Arc<EngineContext>,
    finished_graceful_shutdown: oneshot::Receiver<()>,
}

impl TradingEngine {
    pub fn new(
        context: Arc<EngineContext>,
        finished_graceful_shutdown: oneshot::Receiver<()>,
    ) -> Self {
        TradingEngine {
            context,
            finished_graceful_shutdown,
        }
    }

    pub fn context(&self) -> Arc<EngineContext> {
        self.context.clone()
    }

    pub async fn run(self) {
        let action_outcome = AssertUnwindSafe(self.finished_graceful_shutdown)
            .catch_unwind()
            .await;

        let _ = unwrap_or_handle_panic(
            action_outcome,
            "Panic happened while TradingEngine was run",
            Some(self.context.application_manager.clone()),
        );
    }
}
