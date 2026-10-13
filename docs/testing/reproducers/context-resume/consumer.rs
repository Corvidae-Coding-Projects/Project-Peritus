use peritus_context::ContextNodeId;
use peritus_context::working::{
    WorkingBinding, WorkingLimits, apply_working_event, decode_working_event, decode_working_state,
    render_working_state_with_headroom,
};
use peritus_model_protocol::{ContentBlock, ProtocolLimits, decode_messages};
use peritus_role::HarnessRole;
use peritus_types::{RunId, WorkspaceId};
use serde_json::{Value, json};
use std::path::Path;
fn id<const N: usize>(bytes: &[u8]) -> [u8; N] {
    bytes.try_into().unwrap()
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn artifact(root: &Path, desc: &Value) -> Vec<u8> {
    let hash = hex(&desc["digest"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_u64().unwrap() as u8)
        .collect::<Vec<_>>());
    let bytes =
        std::fs::read(root.join("artifacts/objects/sha256").join(&hash[..2]).join(&hash)).unwrap();
    assert_eq!(bytes.len() as u64, desc["bytes"].as_u64().unwrap());
    assert_eq!(hex(peritus_codec::sha256(&bytes).as_bytes()), hash);
    bytes
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).unwrap();
    let path = Path::new(&path);
    let snap: Value = serde_json::from_slice(&std::fs::read(path.join("snapshot.json"))?)?;
    let root = path.join("context/writer");
    let bytes = artifact(&root, &snap["checkpoint"]["working_state"]);
    let binding = WorkingBinding::new(
        RunId::new(id(&bytes[26..42])).unwrap(),
        WorkspaceId::new(id(&bytes[42..58])).unwrap(),
        ContextNodeId::new(id(&bytes[58..74])).unwrap(),
        HarnessRole::Writer,
        u64::from_be_bytes(id(&bytes[75..83])),
    );
    let mut state = decode_working_state(&bytes, binding, WorkingLimits::standard())?;
    for record in snap["events"].as_array().unwrap() {
        if let Some(obs) = record.get("observation") {
            state = apply_working_event(
                &state,
                &decode_working_event(
                    &artifact(&root, &obs["reducer"]),
                    binding,
                    WorkingLimits::standard(),
                )?,
            )?;
        } else if let Some(ev) = record.get("state_event") {
            state = apply_working_event(
                &state,
                &decode_working_event(
                    &artifact(&root, &ev["reducer"]),
                    binding,
                    WorkingLimits::standard(),
                )?,
            )?;
        }
    }
    let pins = state.protocol(state.binding())?;
    let pinbytes=pins.requirements().iter().map(|pin|{let source=snap["sources"].as_array().unwrap().iter().find(|source|source["sequence"].as_u64()==Some(pin.get())).unwrap();let bytes=artifact(&root,&source["artifact"]);let messages=decode_messages(&bytes,ProtocolLimits::PRODUCTION).unwrap();for message in &messages{for block in message.content(){if let ContentBlock::Text(text)=block{std::fs::write(path.join(format!("pin-{}.txt",pin.get())),text.expose_for_wire()).unwrap();}}}json!({"id":pin.get(),"bytes":bytes.len(),"sha256":hex(peritus_codec::sha256(&bytes).as_bytes())})}).collect::<Vec<_>>();
    let projection = verify_projection(path, &root, &snap, &state)?;
    std::fs::write(path.join("projection-report.json"), serde_json::to_vec_pretty(&projection)?)?;
    let mut renders = Vec::new();
    for available in [4096, 16384, 32768, 65536, 200000] {
        let result = render_working_state_with_headroom(&state, state.binding(), 4096, available);
        renders.push(match result{Ok(view)=>{let segments=view.plan().map(|p|p.segments()).unwrap_or(&[]);json!({"available":available,"ok":true,"selected":segments.iter().map(|s|hex(s.source_id().as_bytes())).collect::<Vec<_>>(),"bytes":segments.iter().map(|s|s.content().len()).sum::<usize>(),"omitted":view.omitted().len()})},Err(e)=>json!({"available":available,"ok":false,"error":format!("{e:?}")})});
    }
    println!(
        "{}",
        json!({"revision":state.revision(),"observations":state.through_observation(),"entries":state.entries(state.binding())?.len(),"pins":pinbytes,"pending":pins.pending().iter().map(|p|json!({"id":hex(p.id().as_bytes()),"source":p.source().get(),"state":format!("{:?}",p.state())})).collect::<Vec<_>>(),"renders":renders})
    );
    Ok(())
}

fn message(role: peritus_model_protocol::Role, text: String) -> peritus_model_protocol::Message {
    use peritus_model_protocol::{BoundedText, ContentBlock, Message, ProtocolLimits};
    Message::new(
        role,
        vec![ContentBlock::Text(BoundedText::new(text, ProtocolLimits::PRODUCTION).unwrap())],
        ProtocolLimits::PRODUCTION,
    )
    .unwrap()
}
fn text_source(root: &Path, snap: &Value, id: u64) -> String {
    let source = snap["sources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|source| source["sequence"].as_u64() == Some(id))
        .unwrap();
    let messages =
        decode_messages(&artifact(root, &source["artifact"]), ProtocolLimits::PRODUCTION).unwrap();
    messages
        .iter()
        .flat_map(|m| m.content())
        .filter_map(
            |b| if let ContentBlock::Text(t) = b { Some(t.expose_for_wire()) } else { None },
        )
        .collect::<Vec<_>>()
        .join("\n")
}
fn verify_projection(
    path: &Path,
    root: &Path,
    snap: &Value,
    state: &peritus_context::working::WorkingState,
) -> Result<Value, Box<dyn std::error::Error>> {
    use peritus_context::working::{
        ObservationId, ObservationKind, ObservationSource, WorkingProtocol, WorkingProtocolUpdate,
        apply_working_protocol, encode_working_state, ingest_working_observation,
    };
    use peritus_model_protocol::{Role, encode_messages};
    use peritus_product_runner::context_projection::{
        DeveloperEvidencePrompt, DeveloperEvidenceSources, project_developer_evidence,
    };
    let old = text_source(root, snap, 9768);
    let system = text_source(root, snap, 9767);
    let policy = text_source(root, snap, 9769);
    let (literal, evidence) =
        old.split_once("\n\nExact failed checks and conserved findings to address:\n").unwrap();
    let (diff, rest) = evidence
        .strip_prefix("Current diff:\n")
        .unwrap()
        .split_once("\n\nExact-target checks:\n")
        .unwrap();
    let (gates, ledger) = rest.rsplit_once("\n\nConserved review ledger:\n").unwrap();
    let instructions =
        format!("{literal}\n\nExact failed checks and conserved findings to address:\n{ledger}");
    let archive = DeveloperEvidenceSources::new(
        String::new(),
        diff.to_owned(),
        gates.to_owned(),
        String::new(),
        ledger.to_owned(),
        String::new(),
    );
    let required = render_working_state_with_headroom(state, state.binding(), 4096, 200000)?;
    let segments = required.plan().unwrap().segments();
    let working = segments
        .iter()
        .map(|s| String::from_utf8_lossy(s.content()).into_owned())
        .collect::<String>();
    let pending = snap["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|e| e.get("transcript"))
        .last()
        .unwrap()["manifest"]["pending"]
        .clone();
    let pending_text =
        format!("PENDING HOST OPERATIONS — OUTCOMES NOT ESTABLISHED; DO NOT REDISPATCH\n{pending}");
    let extra_messages = vec![
        message(Role::User, old.clone()),
        message(Role::Developer, policy.clone()),
        message(Role::User, pending_text.clone()),
        message(Role::User, working.clone()),
    ];
    let extra = peritus_agent::estimate_developer_request_tokens(&extra_messages, &[])
        - peritus_agent::estimate_developer_request_tokens(&[], &[]);
    let projected = project_developer_evidence(&DeveloperEvidencePrompt {
        system: &system,
        instructions: &instructions,
        tools: &[],
        diff,
        gates,
        max_input_tokens: 200000,
        additional_framing_tokens: extra,
    })?;
    assert!(projected.starts_with(&instructions));
    assert!(projected.contains("User: design and build https support"));
    assert!(projected.contains(ledger));
    std::fs::write(path.join("projected-input.txt"), &projected)?;
    let mut selected = vec![
        message(Role::System, system.clone()),
        message(Role::User, projected.clone()),
        message(Role::Developer, policy.clone()),
        message(Role::User, pending_text),
    ];
    let framing = peritus_agent::estimate_developer_request_tokens(&selected, &[]);
    let available = 200000u64.saturating_sub(framing).max(4096);
    let view = render_working_state_with_headroom(state, state.binding(), 4096, available)?;
    let ids = view.plan().unwrap().segments().iter().map(|s| s.source_id()).collect::<Vec<_>>();
    assert_eq!(ids.len(), 122);
    selected.push(message(Role::User, working));
    let final_tokens = peritus_agent::estimate_developer_request_tokens(&selected, &[]);
    assert!(final_tokens <= 200000);
    let bytes =
        encode_messages(&[message(Role::User, projected.clone())], ProtocolLimits::PRODUCTION)?;
    let source = ObservationSource::new(
        ObservationId::new(state.through_observation() + 1)?,
        peritus_codec::sha256(&bytes),
        bytes.len() as u64,
        0,
        bytes.len() as u64,
        ObservationKind::UserInstruction,
    )?;
    let next = ingest_working_observation(state, state.binding(), source)?;
    let protocol = state.protocol(state.binding())?;
    let requirements = protocol
        .requirements()
        .iter()
        .map(|id| if id.get() == 9768 { source.id() } else { *id })
        .collect::<Vec<_>>();
    let mut requirements = requirements;
    requirements.sort_by_key(|id| id.get());
    let next = apply_working_protocol(
        &next,
        &WorkingProtocolUpdate::new(
            next.binding(),
            next.revision(),
            WorkingProtocol::new(
                requirements,
                protocol.pending().to_vec(),
                WorkingLimits::standard(),
            )?,
        ),
    )?;
    let saved = encode_working_state(&next)?;
    std::fs::write(path.join("reopened-state.bin"), &saved)?;
    let reopened = decode_working_state(
        &std::fs::read(path.join("reopened-state.bin"))?,
        state.binding(),
        WorkingLimits::standard(),
    )?;
    assert_eq!(reopened.protocol(reopened.binding())?.pending(), protocol.pending());
    assert_eq!(reopened.entries(reopened.binding())?, state.entries(state.binding())?);
    let mut reads = Vec::new();
    for section in ["diff", "gates", "prior"] {
        let original = archive.section(section).unwrap();
        let mut offset = 0;
        let mut rebuilt = String::new();
        let mut pages = 0;
        loop {
            let page = archive.read(section, offset, 16384)?;
            assert_eq!(page["section"], section);
            assert_eq!(page["offset"], offset);
            assert_eq!(page["total_bytes"], original.len());
            assert_eq!(page["sha256"], hex(peritus_codec::sha256(original.as_bytes()).as_bytes()));
            assert!(page.to_string().len() <= 16384);
            rebuilt.push_str(page["bytes"].as_str().unwrap());
            pages += 1;
            if page["next_offset"].is_null() {
                break;
            }
            let next = page["next_offset"].as_u64().unwrap();
            assert!(next > offset);
            offset = next;
        }
        assert_eq!(rebuilt, original);
        reads.push(json!({"section":section,"bytes":rebuilt.len(),"pages":pages,"sha256":hex(peritus_codec::sha256(rebuilt.as_bytes()).as_bytes())}));
    }
    let mut cases = 0;
    for target in [1, 4096, 32768, 65536, 100000, 200000] {
        let p = project_developer_evidence(&DeveloperEvidencePrompt {
            system: &system,
            instructions: &instructions,
            tools: &[],
            diff,
            gates,
            max_input_tokens: target,
            additional_framing_tokens: extra,
        })?;
        assert!(p.starts_with(&instructions));
        assert!(p.contains("section=diff original_bytes="));
        assert!(p.contains("section=gates original_bytes="));
        cases += 1;
    }
    let invalid = archive.read("diff", 1, 1).unwrap_err().to_string();
    Ok(
        json!({"original_prompt_bytes":old.len(),"literal_and_design_bytes":literal.len(),"ledger_bytes":ledger.len(),"projected_prompt_bytes":projected.len(),"reserved_memory_tokens":extra,"framing_tokens":framing,"available_working_tokens":available,"assembled_tokens":final_tokens,"selected_required_entries":ids.len(),"preserved_unknown_source":protocol.pending()[0].source().get(),"original_state_sha256":hex(peritus_codec::sha256(&artifact(root,&snap["checkpoint"]["working_state"])).as_bytes()),"reopened_state_sha256":hex(peritus_codec::sha256(&saved).as_bytes()),"exact_retrieval":reads,"projection_targets_exercised":cases,"invalid_tiny_page_error":invalid}),
    )
}
