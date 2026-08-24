//! Dynamic WFP session that permits only the exact AppContainer-to-proxy route.

use core::{ffi::c_void, ptr};
use std::{fmt, net::IpAddr};

use windows_sys::{
    Win32::{
        Foundation::{HANDLE, LocalFree},
        NetworkManagement::WindowsFilteringPlatform::{
            FWP_ACTION_BLOCK, FWP_ACTION_PERMIT, FWP_CONDITION_VALUE0, FWP_CONDITION_VALUE0_0,
            FWP_MATCH_EQUAL, FWP_SID, FWP_UINT8, FWP_UINT16, FWP_UINT32, FWP_VALUE0, FWP_VALUE0_0,
            FWPM_ACTION0, FWPM_ACTION0_0, FWPM_CONDITION_ALE_PACKAGE_ID,
            FWPM_CONDITION_IP_PROTOCOL, FWPM_CONDITION_IP_REMOTE_ADDRESS,
            FWPM_CONDITION_IP_REMOTE_PORT, FWPM_DISPLAY_DATA0, FWPM_FILTER_CONDITION0,
            FWPM_FILTER_FLAG_CLEAR_ACTION_RIGHT, FWPM_FILTER_FLAG_DISABLED, FWPM_FILTER0,
            FWPM_LAYER_ALE_AUTH_CONNECT_V4, FWPM_LAYER_ALE_AUTH_CONNECT_V6,
            FWPM_SESSION_FLAG_DYNAMIC, FWPM_SESSION0, FWPM_SUBLAYER0, FwpmEngineClose0,
            FwpmEngineOpen0, FwpmFilterAdd0, FwpmSubLayerAdd0,
        },
        Security::{Authorization::ConvertStringSidToSidW, PSID},
        System::Rpc::RPC_C_AUTHN_WINNT,
    },
    core::GUID,
};

use crate::{
    ProxyRoute, TokenProfile, WindowsError, WindowsErrorKind, WindowsOperation, WindowsRecovery,
};

mod keys;
use keys::PolicyKeys;

const TCP_PROTOCOL: u8 = 6;
const POLICY_SUBLAYER_WEIGHT: u16 = u16::MAX;
const ALLOW_FILTER_WEIGHT: u8 = 15;
const BLOCK_FILTER_WEIGHT: u8 = 1;

/// Unique owner of a dynamic BFE session and its nonpersistent filters.
pub(crate) struct WfpSession {
    engine: usize,
    policy_digest: peritus_types::Sha256Digest,
}

impl WfpSession {
    pub(crate) fn install(profile: &TokenProfile, route: ProxyRoute) -> Result<Self, WindowsError> {
        let sid = exact_app_container_sid(profile)?;
        let keys = PolicyKeys::for_route(profile.principal_sid(), route);
        let mut session = Self::open(keys.session)?;
        session.add_sublayer(keys.sublayer)?;
        let IpAddr::V4(address) = route.endpoint().ip() else {
            return Err(wfp_error("managed Windows proxy route is not IPv4 loopback"));
        };
        session.add_proxy_permit(
            keys.allow_v4,
            keys.sublayer,
            sid.as_ptr(),
            u32::from_be_bytes(address.octets()),
            route.endpoint().port(),
        )?;
        session.add_identity_block(
            keys.block_v4,
            keys.sublayer,
            FWPM_LAYER_ALE_AUTH_CONNECT_V4,
            sid.as_ptr(),
            false,
        )?;
        session.add_identity_block(
            keys.block_v6,
            keys.sublayer,
            FWPM_LAYER_ALE_AUTH_CONNECT_V6,
            sid.as_ptr(),
            false,
        )?;
        session.policy_digest = route.filter_digest();
        Ok(session)
    }

    pub(crate) fn probe(profile: &TokenProfile, identity: peritus_types::Sha256Digest) -> bool {
        let Ok(sid) = exact_app_container_sid(profile) else {
            return false;
        };
        let keys = PolicyKeys::for_probe(profile.principal_sid(), identity);
        let Ok(mut session) = Self::open(keys.session) else {
            return false;
        };
        if session.add_sublayer(keys.sublayer).is_err()
            || session
                .add_identity_block(
                    keys.block_v4,
                    keys.sublayer,
                    FWPM_LAYER_ALE_AUTH_CONNECT_V4,
                    sid.as_ptr(),
                    true,
                )
                .is_err()
        {
            return false;
        }
        session.release().is_ok()
    }

    pub(crate) fn release(&mut self) -> Result<(), WindowsError> {
        if self.engine == 0 {
            return Ok(());
        }
        // SAFETY: `engine` is the uniquely owned handle returned by FwpmEngineOpen0.
        if unsafe { FwpmEngineClose0(self.engine as HANDLE) } != 0 {
            return Err(wfp_cleanup_error("dynamic WFP session cannot be closed"));
        }
        self.engine = 0;
        Ok(())
    }

    fn open(session_key: GUID) -> Result<Self, WindowsError> {
        let mut name = wide("Peritus managed sandbox session");
        let session = FWPM_SESSION0 {
            sessionKey: session_key,
            displayData: display(&mut name),
            flags: FWPM_SESSION_FLAG_DYNAMIC,
            txnWaitTimeoutInMSec: 5_000,
            ..FWPM_SESSION0::default()
        };
        let mut engine = ptr::null_mut();
        // SAFETY: local engine open uses current credentials and a live dynamic-session record.
        let status = unsafe {
            FwpmEngineOpen0(
                ptr::null(),
                RPC_C_AUTHN_WINNT,
                ptr::null(),
                &raw const session,
                &raw mut engine,
            )
        };
        if status != 0 || engine.is_null() {
            return Err(wfp_error("BFE denied or could not open a dynamic WFP session"));
        }
        Ok(Self {
            engine: engine as usize,
            policy_digest: peritus_types::Sha256Digest::new([0; 32]),
        })
    }

    fn add_sublayer(&self, key: GUID) -> Result<(), WindowsError> {
        let mut name = wide("Peritus exact managed proxy isolation");
        let sublayer = FWPM_SUBLAYER0 {
            subLayerKey: key,
            displayData: display(&mut name),
            weight: POLICY_SUBLAYER_WEIGHT,
            ..FWPM_SUBLAYER0::default()
        };
        // SAFETY: the engine is live and BFE copies the complete sublayer record during this call.
        if unsafe { FwpmSubLayerAdd0(self.handle(), &raw const sublayer, ptr::null_mut()) } != 0 {
            return Err(wfp_error("BFE denied creation of the dynamic Peritus sublayer"));
        }
        Ok(())
    }

    fn add_proxy_permit(
        &self,
        key: GUID,
        sublayer: GUID,
        sid: PSID,
        address: u32,
        port: u16,
    ) -> Result<(), WindowsError> {
        let mut conditions = [
            sid_condition(sid),
            scalar_condition(FWPM_CONDITION_IP_REMOTE_ADDRESS, FWP_UINT32, Scalar::U32(address)),
            scalar_condition(FWPM_CONDITION_IP_REMOTE_PORT, FWP_UINT16, Scalar::U16(port)),
            scalar_condition(FWPM_CONDITION_IP_PROTOCOL, FWP_UINT8, Scalar::U8(TCP_PROTOCOL)),
        ];
        self.add_filter(
            key,
            sublayer,
            FWPM_LAYER_ALE_AUTH_CONNECT_V4,
            &mut conditions,
            FWP_ACTION_PERMIT,
            ALLOW_FILTER_WEIGHT,
            FWPM_FILTER_FLAG_CLEAR_ACTION_RIGHT,
            "Peritus permit exact loopback proxy",
        )
    }

    fn add_identity_block(
        &self,
        key: GUID,
        sublayer: GUID,
        layer: GUID,
        sid: PSID,
        disabled: bool,
    ) -> Result<(), WindowsError> {
        let mut conditions = [sid_condition(sid)];
        let flags = FWPM_FILTER_FLAG_CLEAR_ACTION_RIGHT
            | if disabled { FWPM_FILTER_FLAG_DISABLED } else { 0 };
        self.add_filter(
            key,
            sublayer,
            layer,
            &mut conditions,
            FWP_ACTION_BLOCK,
            BLOCK_FILTER_WEIGHT,
            flags,
            "Peritus block other AppContainer outbound",
        )
    }

    #[allow(clippy::too_many_arguments, reason = "one field per exact WFP filter dimension")]
    fn add_filter(
        &self,
        key: GUID,
        sublayer: GUID,
        layer: GUID,
        conditions: &mut [FWPM_FILTER_CONDITION0],
        action_type: u32,
        weight: u8,
        flags: u32,
        display_name: &str,
    ) -> Result<(), WindowsError> {
        let mut name = wide(display_name);
        let filter = FWPM_FILTER0 {
            filterKey: key,
            displayData: display(&mut name),
            flags,
            layerKey: layer,
            subLayerKey: sublayer,
            weight: FWP_VALUE0 { r#type: FWP_UINT8, Anonymous: FWP_VALUE0_0 { uint8: weight } },
            numFilterConditions: u32::try_from(conditions.len())
                .map_err(|_| wfp_error("WFP condition count exceeds Windows bounds"))?,
            filterCondition: conditions.as_mut_ptr(),
            action: FWPM_ACTION0 {
                r#type: action_type,
                Anonymous: FWPM_ACTION0_0 { filterType: GUID::from_u128(0) },
            },
            ..FWPM_FILTER0::default()
        };
        let mut id = 0_u64;
        // SAFETY: engine, conditions, SID, and display data remain live; BFE copies filter data.
        if unsafe { FwpmFilterAdd0(self.handle(), &raw const filter, ptr::null_mut(), &raw mut id) }
            != 0
            || id == 0
        {
            return Err(wfp_error("BFE denied installation of an exact managed-egress filter"));
        }
        Ok(())
    }

    const fn handle(&self) -> HANDLE {
        self.engine as *mut c_void
    }
}

impl fmt::Debug for WfpSession {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WfpSession")
            .field("active", &(self.engine != 0))
            .field("policy_digest", &self.policy_digest)
            .finish()
    }
}

impl Drop for WfpSession {
    fn drop(&mut self) {
        let _ = self.release();
    }
}

#[derive(Clone, Copy)]
enum Scalar {
    U8(u8),
    U16(u16),
    U32(u32),
}

const fn scalar_condition(field: GUID, kind: i32, value: Scalar) -> FWPM_FILTER_CONDITION0 {
    let value = match value {
        Scalar::U8(value) => FWP_CONDITION_VALUE0_0 { uint8: value },
        Scalar::U16(value) => FWP_CONDITION_VALUE0_0 { uint16: value },
        Scalar::U32(value) => FWP_CONDITION_VALUE0_0 { uint32: value },
    };
    FWPM_FILTER_CONDITION0 {
        fieldKey: field,
        matchType: FWP_MATCH_EQUAL,
        conditionValue: FWP_CONDITION_VALUE0 { r#type: kind, Anonymous: value },
    }
}

const fn sid_condition(sid: PSID) -> FWPM_FILTER_CONDITION0 {
    FWPM_FILTER_CONDITION0 {
        fieldKey: FWPM_CONDITION_ALE_PACKAGE_ID,
        matchType: FWP_MATCH_EQUAL,
        conditionValue: FWP_CONDITION_VALUE0 {
            r#type: FWP_SID,
            Anonymous: FWP_CONDITION_VALUE0_0 { sid: sid.cast() },
        },
    }
}

struct OwnedSid(PSID);

impl OwnedSid {
    fn parse(value: &str) -> Result<Self, WindowsError> {
        let wide = wide(value);
        let mut sid = ptr::null_mut();
        // SAFETY: input is NUL-terminated and output points to valid PSID storage.
        if unsafe { ConvertStringSidToSidW(wide.as_ptr(), &raw mut sid) } == 0 {
            return Err(wfp_error("AppContainer package SID cannot be parsed for WFP"));
        }
        Ok(Self(sid))
    }

    const fn as_ptr(&self) -> PSID {
        self.0
    }
}

impl Drop for OwnedSid {
    fn drop(&mut self) {
        // SAFETY: ConvertStringSidToSidW returned uniquely owned LocalAlloc storage.
        unsafe { LocalFree(self.0) };
    }
}

fn exact_app_container_sid(profile: &TokenProfile) -> Result<OwnedSid, WindowsError> {
    match profile {
        TokenProfile::AppContainer(_) => OwnedSid::parse(profile.principal_sid()),
        TokenProfile::RestrictedLowIntegrity { .. } => Err(crate::error::unsupported(
            WindowsOperation::Prepare,
            "restricted-token managed egress lacks an exact WFP package identity",
        )),
    }
}

const fn display(name: &mut [u16]) -> FWPM_DISPLAY_DATA0 {
    FWPM_DISPLAY_DATA0 { name: name.as_mut_ptr(), description: ptr::null_mut() }
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(core::iter::once(0)).collect()
}

fn wfp_error(detail: &'static str) -> WindowsError {
    WindowsError::new(
        WindowsErrorKind::Network,
        WindowsOperation::Prepare,
        WindowsRecovery::ConfigureHost,
        detail,
    )
}

fn wfp_cleanup_error(detail: &'static str) -> WindowsError {
    WindowsError::new(
        WindowsErrorKind::RecoveryIndeterminate,
        WindowsOperation::Release,
        WindowsRecovery::RetryCleanup,
        detail,
    )
}
