use crate::state::StateCoordinator;
use crate::sync::SyncCoordinator;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

pub struct TimerScheduler {
    state: Arc<RwLock<StateCoordinator>>,
    sync_coordinator: Arc<SyncCoordinator>,
}

impl TimerScheduler {
    pub fn new(
        state: Arc<RwLock<StateCoordinator>>,
        sync_coordinator: Arc<SyncCoordinator>,
    ) -> Self {
        Self {
            state,
            sync_coordinator,
        }
    }

    /// Run the scheduler loop. It triggers a check every 3 hours and updates the countdown.
    pub async fn run(&self) {
        // tokio::time::interval ticks immediately the first time.
        let mut interval = tokio::time::interval(Duration::from_secs(3 * 3600));

        loop {
            interval.tick().await;

            let next_check = std::time::Instant::now() + Duration::from_secs(3 * 3600);
            {
                let mut state = self.state.write().await;
                state.set_next_check(next_check);
                state
                    .add_log_line("Periodic timer: triggering automatic update check.".to_string());
            }

            let sync = self.sync_coordinator.clone();
            tokio::spawn(async move {
                sync.check_all(false).await;
            });
        }
    }
}
