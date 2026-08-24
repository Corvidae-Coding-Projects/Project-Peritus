//! Explicit Tokio runtime ownership for ordinary integration tests.

use core::future::Future;

pub fn block_on<T>(future: impl Future<Output = T>) -> T {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test Tokio runtime must build")
        .block_on(future)
}
