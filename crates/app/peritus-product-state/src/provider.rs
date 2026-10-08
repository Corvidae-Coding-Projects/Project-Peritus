//! Durable provider choices without credentials or live-status claims.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{
    ProductStateError,
    identity::{decode_hex, encode_hex},
};

mod account;
mod kind;
pub use kind::ProviderKind;

/// Wire family selected for an explicitly compatible endpoint.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CompatibleProtocol {
    /// `OpenAI` Responses-compatible request and stream shapes.
    Responses,
    /// `OpenAI` Chat Completions-compatible request and stream shapes.
    ChatCompletions,
    /// Anthropic Messages wire contract for a hosted service.
    AnthropicMessages,
    /// Google Generate Content wire contract for a hosted service.
    GoogleGenerateContent,
}

const COMPATIBLE_CATALOG_BINDING_VERSION: u16 = 1;
const MAX_PROVIDER_ROUTES: usize = 256;

/// Stable identity of one configured provider route and its durable runtime owner.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProviderRouteIdentity([u8; 16]);

impl ProviderRouteIdentity {
    /// Creates one exact nonzero route identity.
    ///
    /// # Errors
    /// Returns an identity failure when the value is zero.
    pub fn new(bytes: [u8; 16]) -> Result<Self, ProductStateError> {
        if bytes == [0; 16] {
            return Err(ProductStateError::InvalidIdentity("provider route identity"));
        }
        Ok(Self(bytes))
    }

    /// Parses the canonical lowercase hexadecimal representation.
    ///
    /// # Errors
    /// Returns an identity failure for a malformed or zero value.
    pub fn parse(value: &str) -> Result<Self, ProductStateError> {
        decode_hex(value, "provider route identity").map(Self)
    }

    /// Borrows the exact identity bytes consumed by runtime provider ownership.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

impl core::fmt::Display for ProviderRouteIdentity {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(&encode_hex(self.0))
    }
}

impl Serialize for ProviderRouteIdentity {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&encode_hex(self.0))
    }
}

impl<'de> Deserialize<'de> for ProviderRouteIdentity {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(serde::de::Error::custom)
    }
}

/// One enabled route linking product selection to the runtime provider owner.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ProviderRouteChoice {
    kind: ProviderKind,
    identity: ProviderRouteIdentity,
}

impl ProviderRouteChoice {
    /// Returns the exact runtime profile identity.
    #[must_use]
    pub const fn identity(self) -> ProviderRouteIdentity {
        self.identity
    }

    /// Returns the route's adapter family.
    #[must_use]
    pub const fn kind(self) -> ProviderKind {
        self.kind
    }
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
struct CompatibleCatalogBinding {
    version: u16,
    endpoint: String,
}

impl CompatibleCatalogBinding {
    fn new(endpoint: String) -> Self {
        Self { version: COMPATIBLE_CATALOG_BINDING_VERSION, endpoint }
    }

    fn is_valid(&self) -> bool {
        self.version == COMPATIBLE_CATALOG_BINDING_VERSION
            && bounded_text(&self.endpoint, 2_048)
    }
}

/// Durable non-secret configuration for one direct provider route.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DirectProviderProfile {
    kind: ProviderKind,
    route_identity: ProviderRouteIdentity,
    credential_reference: String,
    endpoint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    catalog_binding: Option<CompatibleCatalogBinding>,
    model: String,
    compatible_protocol: Option<CompatibleProtocol>,
    credential_header: Option<String>,
    #[serde(skip)]
    legacy_route_identity: bool,
}

impl DirectProviderProfile {
    /// Creates one bounded direct route containing only an opaque credential reference.
    ///
    /// # Errors
    ///
    /// Rejects account routes, missing required fields, misplaced compatible fields, or unsafe
    /// text bounds. Exact endpoint and model semantics are validated by the production adapter.
    pub fn new(
        kind: ProviderKind,
        credential_reference: String,
        endpoint: Option<String>,
        model: String,
        compatible_protocol: Option<CompatibleProtocol>,
        credential_header: Option<String>,
    ) -> Result<Self, ProductStateError> {
        Self::new_with_route_identity_and_catalog_endpoint(
            kind,
            kind.route_identity(),
            credential_reference,
            endpoint,
            None,
            model,
            compatible_protocol,
            credential_header,
        )
    }

    /// Creates one bounded direct route with an optional resolved compatible catalog binding.
    ///
    /// # Errors
    ///
    /// Rejects malformed route fields or a catalog binding on a non-compatible provider.
    pub fn new_with_catalog_endpoint(
        kind: ProviderKind,
        credential_reference: String,
        endpoint: Option<String>,
        catalog_endpoint: Option<String>,
        model: String,
        compatible_protocol: Option<CompatibleProtocol>,
        credential_header: Option<String>,
    ) -> Result<Self, ProductStateError> {
        Self::new_with_route_identity_and_catalog_endpoint(
            kind,
            kind.route_identity(),
            credential_reference,
            endpoint,
            catalog_endpoint,
            model,
            compatible_protocol,
            credential_header,
        )
    }

    /// Creates a direct route with an explicit durable runtime identity.
    ///
    /// # Errors
    /// Rejects malformed route fields or an identity that conflicts when selected with another
    /// route.
    pub fn new_with_route_identity(
        kind: ProviderKind,
        route_identity: ProviderRouteIdentity,
        credential_reference: String,
        endpoint: Option<String>,
        model: String,
        compatible_protocol: Option<CompatibleProtocol>,
        credential_header: Option<String>,
    ) -> Result<Self, ProductStateError> {
        Self::new_with_route_identity_and_catalog_endpoint(
            kind,
            route_identity,
            credential_reference,
            endpoint,
            None,
            model,
            compatible_protocol,
            credential_header,
        )
    }

    /// Creates a direct route with explicit runtime identity and compatible catalog binding.
    ///
    /// # Errors
    /// Rejects malformed route fields or a catalog binding on a non-compatible provider.
    pub fn new_with_route_identity_and_catalog_endpoint(
        kind: ProviderKind,
        route_identity: ProviderRouteIdentity,
        credential_reference: String,
        endpoint: Option<String>,
        catalog_endpoint: Option<String>,
        model: String,
        compatible_protocol: Option<CompatibleProtocol>,
        credential_header: Option<String>,
    ) -> Result<Self, ProductStateError> {
        let profile = Self {
            kind,
            route_identity,
            credential_reference,
            endpoint,
            catalog_binding: catalog_endpoint.map(CompatibleCatalogBinding::new),
            model,
            compatible_protocol,
            credential_header,
            legacy_route_identity: false,
        };
        profile.validate()?;
        Ok(profile)
    }

    /// Returns the direct provider kind.
    #[must_use]
    pub const fn kind(&self) -> ProviderKind {
        self.kind
    }

    /// Returns the durable identity used by the runtime provider registry and recovery records.
    #[must_use]
    pub const fn route_identity(&self) -> ProviderRouteIdentity {
        self.route_identity
    }

    /// Borrows the opaque credential reference.
    #[must_use]
    pub fn credential_reference(&self) -> &str {
        &self.credential_reference
    }

    /// Borrows the configured endpoint when required by the adapter.
    #[must_use]
    pub fn endpoint(&self) -> Option<&str> {
        self.endpoint.as_deref()
    }

    /// Borrows the resolved compatible catalog endpoint when discovery was bound during setup.
    #[must_use]
    pub fn catalog_endpoint(&self) -> Option<&str> {
        self.catalog_binding.as_ref().map(|binding| binding.endpoint.as_str())
    }

    /// Borrows the selected provider model.
    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Returns the compatible wire family when this is a compatible endpoint.
    #[must_use]
    pub const fn compatible_protocol(&self) -> Option<CompatibleProtocol> {
        self.compatible_protocol
    }

    /// Borrows an optional compatible raw credential header.
    #[must_use]
    pub fn credential_header(&self) -> Option<&str> {
        self.credential_header.as_deref()
    }

    fn validate(&self) -> Result<(), ProductStateError> {
        let endpoint_required = matches!(
            self.kind,
            ProviderKind::AnthropicApi
                | ProviderKind::GoogleGeminiApi
                | ProviderKind::CompatibleEndpoint
        );
        let compatible = self.kind == ProviderKind::CompatibleEndpoint;
        let hosted = self.kind.hosted_service().is_some();
        if !self.kind.is_direct()
            || endpoint_required != self.endpoint.is_some()
            || (compatible || hosted) != self.compatible_protocol.is_some()
            || compatible
                && matches!(
                    self.compatible_protocol,
                    Some(
                        CompatibleProtocol::AnthropicMessages
                            | CompatibleProtocol::GoogleGenerateContent
                    )
                )
            || !compatible && self.credential_header.is_some()
            || self
                .catalog_binding
                .as_ref()
                .is_some_and(|binding| !compatible || !binding.is_valid())
            || !bounded_text(&self.model, 256)
            || !bounded_text(&self.credential_reference, 256)
            || !self.credential_reference.starts_with("peritus-secret-v1:")
            || self.endpoint.as_deref().is_some_and(|value| !bounded_text(value, 2_048))
            || self.credential_header.as_deref().is_some_and(|value| !bounded_text(value, 128))
        {
            return Err(ProductStateError::InvalidPayload(
                "direct provider profile is malformed or exceeds its bounds".to_owned(),
            ));
        }
        Ok(())
    }

    fn finish_legacy_route_migration(&mut self) {
        self.legacy_route_identity = false;
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DirectProviderProfileWire {
    kind: ProviderKind,
    #[serde(default)]
    route_identity: Option<ProviderRouteIdentity>,
    credential_reference: String,
    endpoint: Option<String>,
    #[serde(default)]
    catalog_binding: Option<CompatibleCatalogBinding>,
    model: String,
    compatible_protocol: Option<CompatibleProtocol>,
    credential_header: Option<String>,
}

impl<'de> Deserialize<'de> for DirectProviderProfile {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = DirectProviderProfileWire::deserialize(deserializer)?;
        let legacy_route_identity = wire.route_identity.is_none();
        let profile = Self {
            kind: wire.kind,
            route_identity: wire.route_identity.unwrap_or_else(|| wire.kind.route_identity()),
            credential_reference: wire.credential_reference,
            endpoint: wire.endpoint,
            catalog_binding: wire.catalog_binding,
            model: wire.model,
            compatible_protocol: wire.compatible_protocol,
            credential_header: wire.credential_header,
            legacy_route_identity,
        };
        profile.validate().map_err(serde::de::Error::custom)?;
        Ok(profile)
    }
}

/// Canonical enabled-provider set and optional default.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderSelection {
    #[serde(default)]
    account_models: BTreeMap<ProviderKind, String>,
    #[serde(default)]
    account_executables: BTreeMap<ProviderKind, String>,
    #[serde(default)]
    account_routes: BTreeMap<ProviderKind, ProviderRouteIdentity>,
    enabled: Vec<ProviderKind>,
    default: Option<ProviderKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    default_route: Option<ProviderRouteIdentity>,
    #[serde(default)]
    automatic_failover: bool,
    #[serde(default)]
    direct_profiles: Vec<DirectProviderProfile>,
}

impl ProviderSelection {
    /// Validates, sorts, and stores selected providers.
    ///
    /// An empty selection is the explicit offline-browse mode and has no default.
    ///
    /// # Errors
    ///
    /// Returns invalid payload when a default is not enabled.
    pub fn new(
        mut enabled: Vec<ProviderKind>,
        default: Option<ProviderKind>,
    ) -> Result<Self, ProductStateError> {
        enabled.sort_unstable();
        enabled.dedup();
        if default.is_some_and(|kind| !enabled.contains(&kind))
            || enabled.is_empty() && default.is_some()
        {
            return Err(ProductStateError::InvalidPayload(
                "default provider must belong to the enabled provider set".to_owned(),
            ));
        }
        Self::with_direct_profiles(enabled, default, Vec::new())
    }

    /// Validates and stores selected providers plus non-secret direct-route profiles.
    ///
    /// # Errors
    ///
    /// Returns invalid payload unless every enabled direct route has exactly one matching profile.
    pub fn with_direct_profiles(
        enabled: Vec<ProviderKind>,
        default: Option<ProviderKind>,
        direct_profiles: Vec<DirectProviderProfile>,
    ) -> Result<Self, ProductStateError> {
        Self::with_direct_profiles_and_failover(enabled, default, direct_profiles, false)
    }

    /// Validates and stores provider routes plus explicit automatic-failover consent.
    ///
    /// # Errors
    ///
    /// Returns invalid payload unless every direct route is complete and failover has at least
    /// two enabled provider choices.
    pub fn with_direct_profiles_and_failover(
        enabled: Vec<ProviderKind>,
        default: Option<ProviderKind>,
        direct_profiles: Vec<DirectProviderProfile>,
        automatic_failover: bool,
    ) -> Result<Self, ProductStateError> {
        let default_route = match default {
            None => None,
            Some(kind) if kind.is_account() => Some(kind.route_identity()),
            Some(kind) => {
                let mut matching = direct_profiles
                    .iter()
                    .filter(|profile| profile.kind == kind)
                    .map(DirectProviderProfile::route_identity);
                let route = matching.next();
                if route.is_none() || matching.next().is_some() {
                    return Err(ProductStateError::InvalidPayload(
                        "a same-kind direct default requires an explicit route identity".to_owned(),
                    ));
                }
                route
            }
        };
        let selection = Self::with_routes_and_failover(
            enabled,
            default_route,
            direct_profiles,
            automatic_failover,
        )?;
        if selection.default != default {
            return Err(ProductStateError::InvalidPayload(
                "default provider must belong to the enabled provider routes".to_owned(),
            ));
        }
        Ok(selection)
    }

    /// Validates and stores exact provider routes plus explicit automatic-failover consent.
    ///
    /// Multiple direct routes may share a provider kind, but their identities must be distinct.
    /// The exact default route is independent of automatic failover consent.
    ///
    /// # Errors
    /// Returns invalid payload unless every route is complete, uniquely identified, and enabled.
    pub fn with_routes_and_failover(
        mut enabled: Vec<ProviderKind>,
        default_route: Option<ProviderRouteIdentity>,
        mut direct_profiles: Vec<DirectProviderProfile>,
        automatic_failover: bool,
    ) -> Result<Self, ProductStateError> {
        enabled.sort_unstable();
        enabled.dedup();
        direct_profiles.sort_unstable();
        let account_routes = enabled
            .iter()
            .copied()
            .filter(|kind| kind.is_account())
            .map(|kind| (kind, kind.route_identity()))
            .collect::<BTreeMap<_, _>>();
        let route_count = account_routes.len().checked_add(direct_profiles.len()).ok_or_else(|| {
            ProductStateError::InvalidPayload("provider route count overflowed".to_owned())
        })?;
        let route_count_u64 = u64::try_from(route_count).map_err(|_| {
            ProductStateError::InvalidPayload("provider route count exceeds u64".to_owned())
        })?;
        let mut identities = account_routes.values().copied().collect::<BTreeSet<_>>();
        let direct_identities_are_unique = direct_profiles
            .iter()
            .all(|profile| identities.insert(profile.route_identity));
        let default = default_route.and_then(|identity| {
            account_routes
                .iter()
                .find_map(|(kind, route)| (*route == identity).then_some(*kind))
                .or_else(|| {
                    direct_profiles
                        .iter()
                        .find(|profile| profile.route_identity == identity)
                        .map(DirectProviderProfile::kind)
                })
        });
        if route_count > MAX_PROVIDER_ROUTES
            || !crate::verified::provider_failover_shape_exec(
                route_count_u64,
                automatic_failover,
            )
            || !direct_identities_are_unique
            || direct_profiles
                .iter()
                .any(|profile| {
                    profile.validate().is_err()
                        || profile.legacy_route_identity
                        || !enabled.contains(&profile.kind)
                })
            || enabled
                .iter()
                .filter(|kind| kind.is_direct())
                .any(|kind| !direct_profiles.iter().any(|profile| profile.kind == *kind))
            || enabled.is_empty() != (route_count == 0)
            || default_route.is_some() != default.is_some()
        {
            return Err(ProductStateError::InvalidPayload(
                "provider selection and exact route identities do not match".to_owned(),
            ));
        }
        Ok(Self {
            account_routes,
            enabled,
            default,
            default_route,
            automatic_failover,
            direct_profiles,
            account_models: BTreeMap::new(),
            account_executables: BTreeMap::new(),
        })
    }

    /// Borrows the canonical enabled providers.
    #[must_use]
    pub fn enabled(&self) -> &[ProviderKind] {
        &self.enabled
    }

    /// Returns the default provider, when model-backed runs are enabled.
    #[must_use]
    pub const fn default(&self) -> Option<ProviderKind> {
        self.default
    }

    /// Returns the exact default runtime route, independent of failover consent.
    #[must_use]
    pub const fn default_route(&self) -> Option<ProviderRouteIdentity> {
        self.default_route
    }

    /// Returns whether the user allowed a role to switch after its selected provider exhausts
    /// ordinary recovery.
    #[must_use]
    pub const fn automatic_failover(&self) -> bool {
        self.automatic_failover
    }

    /// Borrows canonical direct-route profiles.
    #[must_use]
    pub fn direct_profiles(&self) -> &[DirectProviderProfile] {
        &self.direct_profiles
    }

    /// Borrows one direct-route profile by provider kind.
    #[must_use]
    pub fn direct_profile(&self, kind: ProviderKind) -> Option<&DirectProviderProfile> {
        self.direct_profiles.iter().find(|profile| profile.kind == kind)
    }

    /// Borrows one direct-route profile by its durable runtime identity.
    #[must_use]
    pub fn direct_profile_by_route(
        &self,
        identity: ProviderRouteIdentity,
    ) -> Option<&DirectProviderProfile> {
        self.direct_profiles.iter().find(|profile| profile.route_identity == identity)
    }

    /// Returns all enabled routes in canonical order.
    #[must_use]
    pub fn routes(&self) -> Vec<ProviderRouteChoice> {
        let mut routes = self
            .account_routes
            .iter()
            .map(|(kind, identity)| ProviderRouteChoice {
                identity: *identity,
                kind: *kind,
            })
            .chain(self.direct_profiles.iter().map(|profile| ProviderRouteChoice {
                identity: profile.route_identity,
                kind: profile.kind,
            }))
            .collect::<Vec<_>>();
        routes.sort_unstable();
        routes
    }

    /// Returns the number of exact selectable routes.
    #[must_use]
    pub fn route_count(&self) -> usize {
        self.account_routes.len() + self.direct_profiles.len()
    }

    /// Returns whether setup has an enabled provider.
    #[must_use]
    pub const fn is_configured(&self) -> bool {
        !self.enabled.is_empty()
    }

    pub(crate) fn validate(&self) -> Result<(), ProductStateError> {
        let canonical = Self::with_routes_and_failover(
            self.enabled.clone(),
            self.default_route,
            self.direct_profiles.clone(),
            self.automatic_failover,
        )?
        .with_account_models(self.account_models.clone())?
        .with_account_executables(self.account_executables.clone())?;
        if &canonical != self {
            return Err(ProductStateError::InvalidPayload(
                "enabled providers are not canonical".to_owned(),
            ));
        }
        Ok(())
    }

    pub(crate) fn finish_legacy_route_migration(&mut self) -> Result<(), ProductStateError> {
        let mut direct_profiles = self.direct_profiles.clone();
        for profile in &mut direct_profiles {
            profile.finish_legacy_route_migration();
        }
        let migrated = Self::with_direct_profiles_and_failover(
            self.enabled.clone(),
            self.default,
            direct_profiles,
            self.automatic_failover,
        )?
        .with_account_models(self.account_models.clone())?
        .with_account_executables(self.account_executables.clone())?;
        *self = migrated;
        Ok(())
    }
}

fn bounded_text(value: &str, maximum: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum
        && value.bytes().all(|byte| !byte.is_ascii_control())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failover_is_explicit_and_requires_two_routes() {
        let primary = ProviderSelection::new(
            vec![ProviderKind::CodexAccount, ProviderKind::ClaudeAccount],
            Some(ProviderKind::CodexAccount),
        )
        .expect("primary-only selection");
        assert!(!primary.automatic_failover());

        let failover = ProviderSelection::with_direct_profiles_and_failover(
            vec![ProviderKind::CodexAccount, ProviderKind::ClaudeAccount],
            Some(ProviderKind::CodexAccount),
            Vec::new(),
            true,
        )
        .expect("explicit failover selection");
        assert!(failover.automatic_failover());
        assert!(
            ProviderSelection::with_direct_profiles_and_failover(
                vec![ProviderKind::CodexAccount],
                Some(ProviderKind::CodexAccount),
                Vec::new(),
                true,
            )
            .is_err()
        );
    }

    #[test]
    fn old_state_shape_defaults_failover_off() {
        let mut selection: ProviderSelection = serde_json::from_str(
            r#"{"enabled":["codex-account"],"default":"codex-account","direct_profiles":[]}"#,
        )
        .expect("old selection shape");
        assert!(!selection.automatic_failover());
        assert!(selection.validate().is_err());
        selection.finish_legacy_route_migration().expect("explicit route migration");
        selection.validate().expect("migrated selection remains valid");
    }
}
