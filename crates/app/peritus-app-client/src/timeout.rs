//! Optional deadlines selected by the caller, with no product default.

pub fn optional_timeout<T>(
    timeout: Option<std::time::Duration>,
    operation: impl Future<Output = T>,
) -> impl Future<Output = Result<T, tokio::time::error::Elapsed>> {
    let operation = Box::pin(operation);
    async move {
        match timeout {
            Some(duration) => tokio::time::timeout(duration, operation).await,
            None => Ok(operation.await),
        }
    }
}
