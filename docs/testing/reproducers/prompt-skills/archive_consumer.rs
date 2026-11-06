use peritus_model_protocol::{ContentBlock, ProtocolLimits, Role, decode_messages};
use std::{
    collections::BTreeSet,
    error::Error,
    fs,
    path::{Path, PathBuf},
};

const COMMON_MARKER: &str = "prompt-vitamins-v1";
const DOMAIN_MARKER: &str = "prompt-vitamins-domain-design-v2";
const DOMAIN_SECTIONS: [&str; 6] = [
    "## Implicit Domain Knowledge and Engineering Obligations",
    "### Mandatory behavior",
    "### Research before architecture",
    "### Supplied design expansion",
    "### Question discipline",
    "### Governing principle",
];

#[derive(Default)]
struct Summary {
    decoded_prompt_views: usize,
    observed_roles: BTreeSet<&'static str>,
    violations: Vec<String>,
}

fn count(text: &str, needle: &str) -> usize {
    text.match_indices(needle).count()
}

fn classify(system: &str) -> &'static str {
    if system.contains("# Automatic architect contract") {
        "automatic-architect"
    } else if system.contains("interactive Design mode") {
        "design"
    } else if system.contains("read-only planning mode") {
        "plan"
    } else if system.contains("fresh independent read-only review")
        || system.contains("Review independently against literal user requirements")
    {
        "reviewer"
    } else if system.contains("Find the underlying cause and fix it completely") {
        "fixer"
    } else if system.contains("Implement the accepted task design") {
        "developer"
    } else if system.contains("conversational coding assistant") {
        "chat"
    } else {
        "unknown"
    }
}

fn expects_domain_policy(role: &str) -> bool {
    matches!(role, "automatic-architect" | "plan" | "design")
}

fn message_text(message: &peritus_model_protocol::Message) -> Vec<&str> {
    message
        .content()
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text(text) => Some(text.expose_for_wire()),
            _ => None,
        })
        .collect()
}

fn inspect(path: &Path, out: &Path, summary: &mut Summary) -> Result<(), Box<dyn Error>> {
    let bytes = fs::read(path)?;
    let Ok(messages) = decode_messages(&bytes, ProtocolLimits::PRODUCTION) else {
        return Ok(());
    };
    let contains_prompt_policy = messages
        .iter()
        .flat_map(message_text)
        .any(|text| text.contains(COMMON_MARKER) || text.contains(DOMAIN_MARKER));
    if !contains_prompt_policy {
        return Ok(());
    }

    let system = messages
        .iter()
        .filter(|message| message.role() == Role::System)
        .flat_map(message_text)
        .collect::<Vec<_>>()
        .join("\n");
    let role = classify(&system);
    summary.observed_roles.insert(role);

    let all_domain_count = messages
        .iter()
        .flat_map(message_text)
        .map(|text| count(text, DOMAIN_MARKER))
        .sum::<usize>();
    let system_domain_count = count(&system, DOMAIN_MARKER);
    let mut violations = Vec::new();
    if all_domain_count != system_domain_count {
        violations.push("domain policy marker appeared outside Role::System".to_owned());
    }
    if expects_domain_policy(role) {
        if system_domain_count != 1 {
            violations.push(format!(
                "{role} requires exactly one domain policy marker; observed {system_domain_count}"
            ));
        }
        for section in DOMAIN_SECTIONS {
            let observed = count(&system, section);
            if observed != 1 {
                violations.push(format!(
                    "{role} requires exactly one {section:?} section; observed {observed}"
                ));
            }
        }
    } else if system_domain_count != 0 {
        violations.push(format!(
            "{role} must not receive the domain design policy; observed {system_domain_count} markers"
        ));
    }

    let view = messages
        .iter()
        .map(|message| {
            serde_json::json!({
                "role": format!("{:?}", message.role()),
                "text": message_text(message),
            })
        })
        .collect::<Vec<_>>();
    fs::write(
        out.join(format!("view-{:05}.json", summary.decoded_prompt_views)),
        serde_json::to_vec_pretty(&serde_json::json!({
            "source": path,
            "classified_role": role,
            "domain_policy_count": all_domain_count,
            "domain_policy_system_count": system_domain_count,
            "violations": violations,
            "messages": view,
        }))?,
    )?;
    summary.decoded_prompt_views += 1;
    summary
        .violations
        .extend(violations.into_iter().map(|violation| format!("{}: {violation}", path.display())));
    Ok(())
}

fn scan(path: &Path, out: &Path, summary: &mut Summary) -> Result<(), Box<dyn Error>> {
    if path.is_dir() {
        let mut entries = fs::read_dir(path)?.collect::<Result<Vec<_>, _>>()?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            scan(&entry.path(), out, summary)?;
        }
    } else {
        inspect(path, out, summary)?;
    }
    Ok(())
}

fn required_roles(value: Option<&String>) -> BTreeSet<&str> {
    value
        .into_iter()
        .flat_map(|roles| roles.split(','))
        .map(str::trim)
        .filter(|role| !role.is_empty())
        .collect()
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = std::env::args().collect::<Vec<_>>();
    if !(3..=4).contains(&args.len()) {
        return Err(
            "usage: archive-consumer OWNED_CONTEXT OUTPUT_DIRECTORY [REQUIRED_ROLES_CSV]".into()
        );
    }
    let root = PathBuf::from(&args[1]);
    let out = PathBuf::from(&args[2]);
    fs::create_dir_all(&out)?;
    let mut summary = Summary::default();
    scan(&root, &out, &mut summary)?;

    let required = required_roles(args.get(3));
    for role in required {
        if !summary.observed_roles.contains(role) {
            summary.violations.push(format!("required role {role:?} was not observed"));
        }
    }
    let result = serde_json::json!({
        "decoded_prompt_views": summary.decoded_prompt_views,
        "observed_roles": summary.observed_roles,
        "violations": summary.violations,
    });
    println!("{}", serde_json::to_string_pretty(&result)?);
    if result["violations"].as_array().is_some_and(|items| !items.is_empty()) {
        return Err("prompt policy archive validation failed".into());
    }
    Ok(())
}
