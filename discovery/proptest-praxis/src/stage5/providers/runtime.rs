//! Owned synthetic executables through actual Codex, Claude and OpenCode process adapters.

use super::common::{self, Case, Metrics, Mode};
use peritus_provider_anthropic::{ClaudeExecutable, ClaudeRuntimeConfig, ClaudeRuntimeProvider};
use peritus_provider_core::{ModelProvider, ProcessLimits, hosted::HostedService};
use peritus_provider_openai::{CodexExecutable, CodexRuntimeConfig, CodexRuntimeProvider};
use peritus_provider_opencode::{OpenCodeRequestLayout, OpenCodeRuntimeProvider};
use serde_json::{Value, json};
use std::{fs, path::Path, sync::Arc, time::Duration};

const CHILD: &str = r###"#!/usr/bin/python3
import json,os,pathlib,sys,time
root=pathlib.Path(__ROOT__)
case=json.loads((root/'case.json').read_text())
args=sys.argv[1:]
if args==['login','status']:
    print('Synthetic authenticated account fixture');sys.exit(0)
if args[:2]==['auth','status']:
    print('{"loggedIn":true}');sys.exit(0)
if args==['--version']:
    print('1.19.0');sys.exit(0)
if args[:1]==['models']:
    service=args[1]
    model='praxis-selected-'+str(case['model'])
    print(service+'/'+model)
    print(json.dumps({'id':model,'providerID':service,'name':model,'capabilities':{'toolcall':True},'limit':{'input':100000,'context':110000,'output':4096}}))
    sys.exit(0)
stdin=sys.stdin.buffer.read().decode('utf-8')
trace=root/'requests.jsonl'
prior=trace.read_text().splitlines() if trace.exists() else []
record={'argv':args,'stdin':stdin,'pid':os.getpid()}
if case['mode']=='OpenCode':
    record['config']=json.loads(pathlib.Path(os.environ['OPENCODE_CONFIG']).read_text())
with trace.open('a') as stream:
    stream.write(json.dumps(record)+chr(10));stream.flush();os.fsync(stream.fileno())
(root/'ready').write_text('ready')
if case['cancel_after'] is not None and case['cancel_after']>0:
    time.sleep(5)
tool={'name':'lookup','arguments':{'value':case['payload']}}
content='' if case['tool'] and case['mode']!='Claude' else case['payload']
mode=case['mode'];fault=case['fault']
usage={'input_tokens':case['input'],'cached_input_tokens':2,'cache_read_input_tokens':2,'cache_creation_input_tokens':1,'output_tokens':case['output'],'total_tokens':case['input']+case['output']}
if mode=='Codex':
    envelope={'content':content,'tool_calls':[{'name':'lookup','arguments_json':json.dumps(tool['arguments'])}] if case['tool'] else []}
    final=json.dumps(envelope,ensure_ascii=False)
    if '--output-last-message' in args:pathlib.Path(args[args.index('--output-last-message')+1]).write_text(final)
    rows=[{'type':'thread.started','thread_id':'fixture-thread'},{'type':'item.completed','item':{'type':'agent_message','text':final}},{'type':'turn.completed',**({'usage':usage} if case['usage'] else {})}]
    if fault=='MissingTerminal':rows=rows[:-1]
    if fault=='Duplicate':rows.insert(1,dict(rows[0]))
    if fault in ['ConflictingId','ChangedResponse']:rows.insert(1,{'type':'thread.started','thread_id':'foreign-thread'})
    if fault=='ExtraTerminal':rows.append(rows[-1])
    raw=(chr(10).join(json.dumps(row,ensure_ascii=False) for row in rows)+chr(10)).encode()
elif mode=='Claude':
    value={'is_error':False,'structured_output':{'content':content,'tool_calls':[tool] if case['tool'] else []}}
    if case['usage']:value['usage']=usage
    raw=json.dumps(value,ensure_ascii=False).encode()
    if fault in ['MissingTerminal','Duplicate','ConflictingId','ChangedResponse','ExtraTerminal']:raw+=bytes([10])+b'{}'
else:
    value={'content':content,'tool_calls':[tool] if case['tool'] else []}
    if case['retry'] and not prior:value={'content':7,'tool_calls':[]}
    body=json.dumps(value,ensure_ascii=False)
    def event(kind,part):return {'type':kind,'sessionID':'fixture-session','part':{'sessionID':'fixture-session','messageID':'fixture-message',**part}}
    rows=[event('step_start',{'type':'step-start'}),event('text',{'type':'text','text':body}),event('step_finish',{'type':'step-finish','reason':'stop',**({'tokens':{'input':case['input']-3,'cache':{'read':2,'write':1},'output':case['output']-2,'reasoning':2,'total':case['input']+case['output']}} if case['usage'] else {})})]
    if fault=='MissingTerminal':rows=rows[:-1]
    if fault=='Duplicate':rows.append(rows[-1])
    if fault in ['ConflictingId','ChangedResponse']:rows[1]['sessionID']='foreign-session'
    if fault=='ExtraTerminal':rows.append(rows[-1])
    raw=(chr(10).join(json.dumps(row,ensure_ascii=False) for row in rows)+chr(10)).encode()
if fault=='MalformedFrame':raw=b'{malformed native framing'
if fault=='PartialTool':raw=raw[:max(1,len(raw)//2)]
if isinstance(fault,dict) and 'Truncate' in fault:raw=raw[:len(raw)*fault['Truncate']//256]
widths=case['chunks'] or [4096]
offset=0;ordinal=0
while offset<len(raw):
    width=max(1,widths[ordinal%len(widths)])
    sys.stdout.buffer.write(raw[offset:offset+width]);sys.stdout.buffer.flush()
    offset+=width;ordinal+=1
"###;

pub fn check(case: &Case) -> Result<Metrics, String> {
    let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
    let root = directory.path();
    fs::write(root.join("case.json"), serde_json::to_vec(case).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let executable = root.join("synthetic-provider");
    let source = CHILD.replace(
        "__ROOT__",
        &serde_json::to_string(&root.to_string_lossy()).map_err(|e| e.to_string())?,
    );
    fs::write(&executable, source).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700))
            .map_err(|e| e.to_string())?;
    }
    let limits = ProcessLimits::new(1024 * 1024, 1024 * 1024, 64 * 1024, Duration::from_secs(8))
        .map_err(|e| e.to_string())?;
    let profile = common::profile(case.mode)?;
    let provider: Arc<dyn ModelProvider> = match case.mode {
        Mode::Codex => Arc::new(CodexRuntimeProvider::new(
            CodexRuntimeConfig::new(
                CodexExecutable::pin(&executable).map_err(|e| e.to_string())?,
                profile,
                limits,
            )
            .map_err(|e| e.to_string())?,
        )),
        Mode::Claude => Arc::new(ClaudeRuntimeProvider::new(
            ClaudeRuntimeConfig::new(
                ClaudeExecutable::pin(&executable).map_err(|e| e.to_string())?,
                profile,
                limits,
            )
            .map_err(|e| e.to_string())?,
        )),
        Mode::OpenCode => Arc::new(
            OpenCodeRuntimeProvider::with_executable_path(
                Some(executable),
                if case.variant { HostedService::OpenCodeGo } else { HostedService::OpenCodeZen },
                common::credential()?,
                Arc::new(common::SyntheticCredential),
                profile,
            )
            .map_err(|e| e.to_string())?
            .with_discovered_limits(false)
            .with_request_layout(if case.legacy {
                OpenCodeRequestLayout::Legacy
            } else {
                OpenCodeRequestLayout::OnceOnly
            }),
        ),
        _ => return Err("runtime fixture received an HTTP mode".into()),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    let mut metrics = Metrics::new();
    runtime.block_on(common::drive_owned(
        provider,
        case,
        &mut metrics,
        Some(root.join("ready")),
    ))?;
    inspect(case, root, &mut metrics)?;
    Ok(metrics)
}

fn inspect(case: &Case, root: &Path, metrics: &mut Metrics) -> Result<(), String> {
    let bytes = match fs::read_to_string(root.join("requests.jsonl")) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error.to_string()),
    };
    let requests = bytes
        .lines()
        .map(serde_json::from_str::<Value>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    if case.cancel_after == Some(0) || !case.mode.effort_supported(case.effort) {
        if !requests.is_empty() {
            return Err("cancelled/unsupported native request was submitted".into());
        }
        return Ok(());
    }
    if requests.is_empty() {
        return Err("owned executable never observed the admitted turn".into());
    }
    if requests.len() > 2 {
        return Err("native adapter exceeded one bounded correction".into());
    }
    for value in &requests {
        let args = value["argv"]
            .as_array()
            .ok_or("missing captured native argv")?
            .iter()
            .map(|v| v.as_str().ok_or("nonstring argv"))
            .collect::<Result<Vec<_>, _>>()?;
        let index =
            args.iter().position(|v| *v == "--model").ok_or("native model argument absent")?;
        let actual = args.get(index + 1).ok_or("native model value absent")?;
        if *actual != common::model(case) {
            return Err(format!("native transport changed selected model: {args:?}"));
        }
        if case.effort != 0 {
            let effort = if case.effort == 1 {
                "low"
            } else if case.effort == 2 {
                "high"
            } else {
                "ultra"
            };
            let preserved = if case.mode == Mode::Codex {
                args.iter().any(|v| *v == format!("model_reasoning_effort=\"{effort}\""))
            } else {
                args.windows(2).any(|v| v == ["--effort", effort])
            };
            if !preserved {
                return Err("owned executable lost selected effort".into());
            }
            common::bump(metrics, "preserved_selected_effort");
        }
        if case.mode == Mode::Codex
            && (!args.contains(&"--ignore-user-config")
                || !args.contains(&"--ignore-rules")
                || !args.windows(2).any(|v| v == ["--sandbox", "read-only"]))
        {
            return Err("Codex native authority was not isolated".into());
        }
        if case.mode == Mode::Claude
            && (!args.contains(&"--safe-mode")
                || !args.contains(&"--no-chrome")
                || !args.contains(&"--no-session-persistence"))
        {
            return Err("Claude native authority was not isolated".into());
        }
        if value["stdin"].as_str().is_none_or(str::is_empty) {
            return Err("native request prompt was empty".into());
        }
        if case.mode == Mode::OpenCode
            && (value.pointer("/config/permission/*").and_then(Value::as_str) != Some("deny")
                || value.pointer("/config/agent/peritus-router/tools/*") != Some(&json!(false))
                || value.pointer("/config/plugin") != Some(&json!([]))
                || value.pointer("/config/mcp") != Some(&json!({})))
        {
            return Err(
                "actual OpenCode private configuration did not disable native authority".into()
            );
        }
        if case.mode == Mode::OpenCode {
            let metadata = value["stdin"]
                .as_str()
                .ok_or("missing native input")?
                .lines()
                .find_map(|line| {
                    serde_json::from_str::<Value>(line)
                        .ok()
                        .filter(|value| value.get("peritus_tool_protocol").is_some())
                })
                .ok_or("missing actual OpenCode tool catalog")?;
            let tool = metadata
                .pointer("/peritus_tool_protocol/tools/0")
                .ok_or("OpenCode tool catalog lost declared function")?;
            if metadata.get("model").and_then(Value::as_str) != Some(common::model(case).as_str())
                || tool.get("name").and_then(Value::as_str) != Some("lookup")
            {
                return Err("OpenCode prompt metadata changed selected model/tool".into());
            }
            let expected = json!({"type":"object","properties":{"value":{"type":"string"}},"required":["value"],"additionalProperties":false});
            if case.legacy {
                if tool.get("arguments_schema") != Some(&expected) {
                    return Err("legacy OpenCode catalog lost full schema".into());
                }
                common::bump(metrics, "legacy_runtime_catalogs");
            } else {
                if tool.as_object().is_none_or(|object| object.len() != 1) {
                    return Err("once-only OpenCode prompt repeated schema metadata".into());
                }
                common::bump(metrics, "once_only_runtime_catalogs");
            }
        }
        common::bump(metrics, "preserved_selected_model");
        common::bump(metrics, "owned_process_turns");
    }
    if requests.len() == 2 {
        common::bump(metrics, "native_format_corrections");
    }
    Ok(())
}
