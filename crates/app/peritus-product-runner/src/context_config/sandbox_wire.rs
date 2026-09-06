//! Explicit serde bridge preserves strict platform-tagged operator configuration.

use super::LocalCompactorSandbox;
use serde::Deserialize;
use serde::Serialize;
use serde::ser::SerializeMap;
use std::path::PathBuf;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Declaration {
    platform: String,
    helper: PathBuf,
    bubblewrap: Option<PathBuf>,
    cgroup_root: Option<PathBuf>,
    seatbelt: Option<PathBuf>,
}

impl<'de> Deserialize<'de> for LocalCompactorSandbox {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let Declaration { platform, helper, bubblewrap, cgroup_root, seatbelt } =
            Declaration::deserialize(deserializer)?;
        match (platform.as_str(), bubblewrap, cgroup_root, seatbelt) {
            ("linux", Some(bubblewrap), Some(cgroup_root), None) => {
                Ok(Self::Linux { helper, bubblewrap, cgroup_root })
            }
            ("macos", None, None, Some(seatbelt)) => Ok(Self::Macos { helper, seatbelt }),
            ("windows", None, None, None) => Ok(Self::Windows { helper }),
            _ => Err(serde::de::Error::custom("invalid local sandbox platform fields")),
        }
    }
}

impl Serialize for LocalCompactorSandbox {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(match self {
            Self::Linux { .. } => 4,
            Self::Macos { .. } => 3,
            Self::Windows { .. } => 2,
        }))?;
        match self {
            Self::Linux { helper, bubblewrap, cgroup_root } => {
                map.serialize_entry("platform", "linux")?;
                map.serialize_entry("helper", helper)?;
                map.serialize_entry("bubblewrap", bubblewrap)?;
                map.serialize_entry("cgroup_root", cgroup_root)?;
            }
            Self::Macos { helper, seatbelt } => {
                map.serialize_entry("platform", "macos")?;
                map.serialize_entry("helper", helper)?;
                map.serialize_entry("seatbelt", seatbelt)?;
            }
            Self::Windows { helper } => {
                map.serialize_entry("platform", "windows")?;
                map.serialize_entry("helper", helper)?;
            }
        }
        map.end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_fields_are_exact_and_roundtrip_without_widening() {
        for input in [
            r#"{"platform":"linux","helper":"/helper","bubblewrap":"/bwrap","cgroup_root":"/delegated"}"#,
            r#"{"platform":"macos","helper":"/helper","seatbelt":"/sandbox-exec"}"#,
            r#"{"platform":"windows","helper":"C:/helper.exe"}"#,
        ] {
            let config: LocalCompactorSandbox = serde_json::from_str(input).unwrap();
            let encoded = serde_json::to_string(&config).unwrap();
            assert_eq!(serde_json::from_str::<LocalCompactorSandbox>(&encoded).unwrap(), config);
        }
        for input in [
            r#"{"platform":"linux","helper":"/helper"}"#,
            r#"{"platform":"windows","helper":"C:/helper.exe","seatbelt":"/unexpected"}"#,
            r#"{"platform":"windows","helper":"C:/helper.exe","network":true}"#,
        ] {
            assert!(serde_json::from_str::<LocalCompactorSandbox>(input).is_err());
        }
    }
}
