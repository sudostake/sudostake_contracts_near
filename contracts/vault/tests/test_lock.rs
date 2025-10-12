#![cfg(feature = "integration-test")]

use std::sync::OnceLock;
use tokio::sync::{Mutex, MutexGuard};

static TEST_MUTEX: OnceLock<Mutex<()>> = OnceLock::new();

/// Acquire the global test mutex to serialize sandbox-based integration tests that
/// depend on shared blockchain state.
pub async fn acquire_test_mutex() -> MutexGuard<'static, ()> {
    TEST_MUTEX
        .get_or_init(|| Mutex::new(()))
        .lock()
        .await
}
