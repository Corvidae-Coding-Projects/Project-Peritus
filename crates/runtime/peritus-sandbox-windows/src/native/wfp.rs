//! Dynamic WFP session that permits only the exact AppContainer-to-proxy route.

use core::{ffi::c_void, ptr};
use std::{
    fmt,
    net::{IpAddr, Ipv4Addr},
    sync::atomic::{AtomicU64, Ordering},
};

use windows_sys::{
    Win32::{
        Foundation::{ERROR_ACCESS_DENIED, FWP_E_ALREADY_EXISTS, HANDLE, LocalFree},
        NetworkManagement::WindowsFilteringPlatform::{
            FWP_ACTION_BLOCK, FWP_ACTION_PERMIT, FWP_CONDITION_VALUE0, FWP_CONDITION_VALUE0_0,
            FWP_MATCH_EQUAL, FWP_SID, FWP_UINT8, FWP_UINT16, FWP_UINT32, FWP_VALUE0, FWP_VALUE0_0,
            FWPM_ACTION0, FWPM_ACTION0_0, FWPM_CONDITION_ALE_PACKAGE_ID,
            FWPM_CONDITION_IP_PROTOCOL, FWPM_CONDITION_IP_REMOTE_ADDRESS,
            FWPM_CONDITION_IP_REMOTE_PORT, FWPM_DISPLAY_DATA0, FWPM_FILTER_CONDITION0,
            FWPM_FILTER_FLAG_CLEAR_ACTION_RIGHT, FWPM_FILTER0,
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
    AppContainerProfile, ProxyRoute, TokenProfile, WindowsError, WindowsErrorKind,
    WindowsErrorSource, WindowsOperation, WindowsRecovery,
};

mod keys;
use keys::PolicyKeys;

const TCP_PROTOCOL: u8 = 6;
const POLICY_SUBLAYER_WEIGHT: u16 = u16::MAX;
const ALLOW_FILTER_WEIGHT: u8 = 15;
const BLOCK_FILTER_WEIGHT: u8 = 1;
static PROBE_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct WfpProbeEvidence {
    pub(super) installation: bool,
    pub(super) enabled_filters: bool,
    pub(super) connection_qualified: bool,
}

impl WfpProbeEvidence {
    pub(super) const fn qualified(self) -> bool {
        self.installation && self.enabled_filters && self.connection_qualified
    }
}

/// Unique owner of a dynamic BFE session and its nonpersistent filters.
pub(crate) struct WfpSession {
    engine: usize,
    policy_digest: peritus_types::Sha256Digest,
    ownership_digest: peritus_types::Sha256Digest,
}

impl WfpSession {
    pub(crate) fn install(profile: &TokenProfile, route: ProxyRoute) -> Result<Self, WindowsError> {
        let IpAddr::V4(address) = route.endpoint().ip() else {
            return Err(wfp_error("managed Windows proxy route is not IPv4 loopback"));
        };
        let sid = exact_app_container_sid(profile)?;
        let keys = PolicyKeys::for_route(profile.principal_sid(), route);
        let mut session = Self::open(keys.session, keys.ownership_digest)?;
        session.add_sublayer(keys.sublayer)?;
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
        )?;
        session.add_identity_block(
            keys.block_v6,
            keys.sublayer,
            FWPM_LAYER_ALE_AUTH_CONNECT_V6,
            sid.as_ptr(),
        )?;
        session.policy_digest = route.filter_digest();
        Ok(session)
    }

    pub(super) fn probe(
        profile: &TokenProfile,
        identity: peritus_types::Sha256Digest,
    ) -> WfpProbeEvidence {
        let Ok(probe_profile) = isolated_probe_profile(profile, identity) else {
            return WfpProbeEvidence::default();
        };
        let probe_profile = TokenProfile::AppContainer(probe_profile);
        let Ok(sid) = exact_app_container_sid(&probe_profile) else {
            return WfpProbeEvidence::default();
        };
        let keys = PolicyKeys::for_probe(probe_profile.principal_sid(), identity);
        let Ok(mut session) = Self::open(keys.session, keys.ownership_digest) else {
            return WfpProbeEvidence::default();
        };
        let installation = session.add_sublayer(keys.sublayer).is_ok();
        let enabled_filters = installation
            && session
                .add_proxy_permit(
                    keys.allow_v4,
                    keys.sublayer,
                    sid.as_ptr(),
                    u32::from_be_bytes(Ipv4Addr::LOCALHOST.octets()),
                    9,
                )
                .is_ok()
            && session
                .add_identity_block(
                    keys.block_v4,
                    keys.sublayer,
                    FWPM_LAYER_ALE_AUTH_CONNECT_V4,
                    sid.as_ptr(),
                )
                .is_ok()
            && session
                .add_identity_block(
                    keys.block_v6,
                    keys.sublayer,
                    FWPM_LAYER_ALE_AUTH_CONNECT_V6,
                    sid.as_ptr(),
                )
                .is_ok();
        if session.release().is_err() {
            return WfpProbeEvidence::default();
        }
        WfpProbeEvidence { installation, enabled_filters, connection_qualified: false }
    }

    pub(crate) fn release(&mut self) -> Result<(), WindowsError> {
        if self.engine == 0 {
            return Ok(());
        }
        // SAFETY: `engine` is the uniquely owned handle returned by FwpmEngineOpen0.
        let status = unsafe { FwpmEngineClose0(self.engine as HANDLE) };
        if status != 0 {
            return Err(wfp_cleanup_status_error(
                "dynamic WFP session cannot be closed",
                status,
            ));
        }
        self.engine = 0;
        Ok(())
    }

    pub(crate) fn custody_identity(&self) -> Option<peritus_types::Sha256Digest> {
        if self.engine == 0 || self.policy_digest == peritus_types::Sha256Digest::new([0; 32]) {
            return None;
        }
        let mut bytes = Vec::from(b"PERITUS-WINDOWS-WFP-OWNER-V2\0".as_slice());
        bytes.extend_from_slice(&(self.engine as u64).to_be_bytes());
        bytes.extend_from_slice(self.policy_digest.as_bytes());
        bytes.extend_from_slice(self.ownership_digest.as_bytes());
        Some(peritus_codec::sha256(&bytes))
    }

    fn open(
        session_key: GUID,
        ownership_digest: peritus_types::Sha256Digest,
    ) -> Result<Self, WindowsError> {
        let mut name = wide("Peritus managed sandbox session");
        let session = FWPM_SESSION0 {
            sessionKey: session_key,
            displayData: display(&mut name),
            flags: FWPM_SESSION_FLAG_DYNAMIC,
            txnWaitTimeoutInMSec: 0,
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
        if status != 0 {
            return Err(wfp_status_error(
                "BFE denied or could not open a dynamic WFP session",
                status,
                WindowsRecovery::ConfigureHost,
            ));
        }
        if engine.is_null() {
            return Err(wfp_error("BFE returned no dynamic WFP session handle"));
        }
        Ok(Self {
            engine: engine as usize,
            policy_digest: peritus_types::Sha256Digest::new([0; 32]),
            ownership_digest,
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
        let status = unsafe {
            FwpmSubLayerAdd0(self.handle(), &raw const sublayer, ptr::null_mut())
        };
        if status != 0 {
            return Err(wfp_status_error(
                "BFE denied creation of the dynamic Peritus sublayer",
                status,
                WindowsRecovery::ConfigureHost,
            ));
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
    ) -> Result<(), WindowsError> {
        let mut conditions = [sid_condition(sid)];
        self.add_filter(
            key,
            sublayer,
            layer,
            &mut conditions,
            FWP_ACTION_BLOCK,
            BLOCK_FILTER_WEIGHT,
            FWPM_FILTER_FLAG_CLEAR_ACTION_RIGHT,
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
        let status = unsafe {
            FwpmFilterAdd0(self.handle(), &raw const filter, ptr::null_mut(), &raw mut id)
        };
        if status != 0 {
            return Err(wfp_status_error(
                "BFE denied installation of an exact managed-egress filter",
                status,
                WindowsRecovery::ConfigureHost,
            ));
        }
        if id == 0 {
            return Err(wfp_error("BFE returned no identity for an installed managed-egress filter"));
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
            .field("ownership_digest", &self.ownership_digest)
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

fn isolated_probe_profile(
    profile: &TokenProfile,
    identity: peritus_types::Sha256Digest,
) -> Result<AppContainerProfile, WindowsError> {
    if !profile.is_app_container() {
        return Err(crate::error::unsupported(
            WindowsOperation::Probe,
            "managed WFP probing requires an AppContainer identity",
        ));
    }
    let sequence = PROBE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let mut bytes = Vec::from(b"PERITUS-WINDOWS-WFP-PROBE-PROFILE-V1\0".as_slice());
    bytes.extend_from_slice(&std::process::id().to_be_bytes());
    bytes.extend_from_slice(&sequence.to_be_bytes());
    bytes.extend_from_slice(profile.principal_sid().as_bytes());
    bytes.extend_from_slice(identity.as_bytes());
    let digest = peritus_codec::sha256(&bytes);
    let mut name = String::from("Peritus.WfpProbe.");
    append_hex_prefix(&mut name, digest.as_bytes(), 16);
    AppContainerProfile::derive_for_current_host(name)
}

fn append_hex_prefix(target: &mut String, bytes: &[u8], count: usize) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for byte in bytes.iter().take(count) {
        target.push(char::from(HEX[usize::from(byte >> 4)]));
        target.push(char::from(HEX[usize::from(byte & 0x0f)]));
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

fn wfp_status_error(
    detail: &'static str,
    status: u32,
    fallback: WindowsRecovery,
) -> WindowsError {
    let recovery = if status == ERROR_ACCESS_DENIED {
        WindowsRecovery::Reauthorize
    } else if status == FWP_E_ALREADY_EXISTS as u32 {
        WindowsRecovery::ReconcileCleanup
    } else {
        fallback
    };
    WindowsError::new(
        WindowsErrorKind::Network,
        WindowsOperation::Prepare,
        recovery,
        detail,
    )
    .with_source(WindowsErrorSource::WindowsStatus(status))
}

fn wfp_cleanup_status_error(detail: &'static str, status: u32) -> WindowsError {
    WindowsError::new(
        WindowsErrorKind::RecoveryIndeterminate,
        WindowsOperation::Release,
        WindowsRecovery::RetryCleanup,
        detail,
    )
    .with_source(WindowsErrorSource::WindowsStatus(status))
}
