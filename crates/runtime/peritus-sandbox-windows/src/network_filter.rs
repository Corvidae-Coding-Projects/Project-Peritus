//! Session-owned Windows Filtering Platform policy lifecycle.

use crate::{ProxyRoute, TokenProfile, WindowsError, WindowsOperation};

/// Owner of the exact dynamic WFP policy for one managed proxy route.
#[derive(Debug)]
pub(crate) struct NetworkFilterOwner {
    #[cfg(target_os = "windows")]
    native: Option<crate::native::wfp::WfpSession>,
    managed: bool,
}

impl NetworkFilterOwner {
    pub(crate) const fn inactive() -> Self {
        Self {
            #[cfg(target_os = "windows")]
            native: None,
            managed: false,
        }
    }

    pub(crate) fn install(profile: &TokenProfile, route: ProxyRoute) -> Result<Self, WindowsError> {
        if !profile.is_app_container() {
            return Err(crate::error::unsupported(
                WindowsOperation::Prepare,
                "managed WFP egress requires an exact AppContainer package SID",
            ));
        }
        #[cfg(target_os = "windows")]
        {
            let native = crate::native::wfp::WfpSession::install(profile, route)?;
            Ok(Self { native: Some(native), managed: true })
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = route;
            Err(crate::error::unsupported(
                WindowsOperation::Prepare,
                "dynamic Windows Filtering Platform enforcement is unavailable on this host",
            ))
        }
    }

    #[must_use = "filter teardown evidence must be included in the release report"]
    #[allow(
        clippy::missing_const_for_fn,
        reason = "the Windows implementation closes a native WFP engine handle"
    )]
    #[allow(
        clippy::unnecessary_wraps,
        reason = "the Windows implementation returns native WFP teardown failure"
    )]
    pub(crate) fn release(&mut self) -> Result<bool, WindowsError> {
        #[cfg(target_os = "windows")]
        if let Some(native) = self.native.as_mut() {
            native.release()?;
            self.native = None;
        }
        self.managed = false;
        Ok(true)
    }
}
