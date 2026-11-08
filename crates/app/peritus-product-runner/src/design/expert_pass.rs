//! Durable expert work, independent of the model transcript and published expansion.

use std::fs::{File, OpenOptions};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::SourceDesignSnapshot;
use crate::{ProductRunInput, ProductRunnerError, ProductRunnerErrorKind};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ExpertPass {
    schema: u32,
    pub version: u64,
    pub source: String,
    #[serde(default)]
    pub source_bytes: u64,
    pub input_revision: u64,
    pub domains: Vec<Domain>,
    pub amendments: Vec<Amendment>,
    pub imported_records: Option<Value>,
    pub imported_markdown: Option<String>,
    pub last_call: Option<String>,
    #[serde(default)]
    pub selected_file: Option<Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Domain {
    pub discipline: String,
    pub applicability: String,
    pub risks: Vec<String>,
    pub questions: Vec<Question>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Question {
    pub question: String,
    pub external_authority_required: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Amendment {
    pub question_id: String,
    pub heading: String,
    pub target_section: String,
    pub markdown: String,
    pub class: String,
    pub rationale: String,
    pub affected_contracts: Vec<String>,
    pub failure_behavior: String,
    pub acceptance: Vec<String>,
    pub evidence: Vec<super::evidence_input::Evidence>,
}

pub(super) fn failure(detail: impl Into<String>) -> ProductRunnerError {
    ProductRunnerError::new(ProductRunnerErrorKind::Repository, "retain expert design pass", detail)
}

impl ExpertPass {
    pub fn load(
        input: &ProductRunInput,
        source: &SourceDesignSnapshot,
    ) -> Result<Self, ProductRunnerError> {
        let digest = crate::encode_hex(source.metadata().sha256().as_bytes());
        let identity: Value = serde_json::from_str(source.metadata().source_identity_json())
            .map_err(|e| failure(e.to_string()))?;
        let mut selected_file = if identity["kind"] == "supplied_workspace" {
            let sha: [u8; 32] = serde_json::from_value(identity["observed_sha256"].clone())
                .map_err(|e| failure(e.to_string()))?;
            Some(json!({"path":identity["path"],"sha256":crate::encode_hex(&sha)}))
        } else {
            None
        };
        if let Some(bytes) =
            super::retention::read(&input.trace_path.with_extension("design-expert.json"))?
        {
            let pass: Self = serde_json::from_slice(&bytes).map_err(|e| failure(e.to_string()))?;
            if pass.schema != 1 || pass.version == 0 {
                return Err(failure("unsupported expert-pass schema or version"));
            }
            if pass.source == digest
                && pass.input_revision == input.conversation.incorporated_revision()
            {
                return Ok(pass);
            }
            if identity["kind"] == "steered" {
                let prior: [u8; 32] = serde_json::from_value(identity["prior_sha256"].clone())
                    .map_err(|e| failure(e.to_string()))?;
                if crate::encode_hex(&prior) == pass.source {
                    selected_file.clone_from(&pass.selected_file);
                }
            }
            if selected_file.is_none()
                && pass.source_bytes > 0
                && let Ok(length) = usize::try_from(pass.source_bytes)
                && let Some(prefix) = source.text().as_bytes().get(..length)
                && crate::encode_hex(peritus_codec::sha256(prefix).as_bytes()) == pass.source
            {
                selected_file.clone_from(&pass.selected_file);
            }
        }
        Ok(Self {
            schema: 1,
            version: 1,
            source: digest,
            source_bytes: source.metadata().bytes(),
            input_revision: input.conversation.incorporated_revision(),
            domains: Vec::new(),
            amendments: Vec::new(),
            imported_records: None,
            imported_markdown: None,
            last_call: None,
            selected_file,
        })
    }

    pub fn lock(input: &ProductRunInput) -> Result<File, ProductRunnerError> {
        let path = input.trace_path.with_extension("design-expert.lock");
        if let Ok(metadata) = std::fs::symlink_metadata(&path)
            && (!metadata.is_file() || metadata.file_type().is_symlink())
        {
            return Err(failure("expert-pass lock is not a regular file"));
        }
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            options.mode(0o600);
        }
        let file = options.open(path).map_err(|e| failure(e.to_string()))?;
        fs4::FileExt::lock(&file).map_err(|e| failure(e.to_string()))?;
        Ok(file)
    }

    pub fn commit(
        &mut self,
        input: &ProductRunInput,
        call_digest: String,
    ) -> Result<(), ProductRunnerError> {
        if input.conversation.revision() != self.input_revision
            || input.conversation.incorporated_revision() != self.input_revision
        {
            return Err(failure("expert-pass input changed before commit"));
        }
        let source =
            SourceDesignSnapshot::open(&input.trace_path, input.run_id, input.workspace_id)?;
        if crate::encode_hex(source.metadata().sha256().as_bytes()) != self.source {
            return Err(failure("expert-pass source changed before commit"));
        }
        let path = input.trace_path.with_extension("design-expert.json");
        if let Some(bytes) = super::retention::read(&path)? {
            let digest = crate::encode_hex(peritus_codec::sha256(&bytes).as_bytes());
            super::retention::write(
                &input.trace_path.with_extension(format!("design-expert-{digest}.history")),
                &bytes,
            )?;
            super::retention::write(
                &input.trace_path.with_extension("design-expert-prior.json"),
                &bytes,
            )?;
        }
        self.version =
            self.version.checked_add(1).ok_or_else(|| failure("expert-pass version overflow"))?;
        self.last_call = Some(call_digest);
        let bytes = serde_json::to_vec(self).map_err(|e| failure(e.to_string()))?;
        super::retention::write(&path, &bytes)?;
        crate::efficiency::event(
            &input.trace_path,
            json!({"event":"design-expert-checkpoint", "draft_version":self.version,
            "input_revision":self.input_revision, "amendments":self.amendments.len(), "questions":self.questions().len()}),
        );
        Ok(())
    }

    pub fn questions(&self) -> Vec<(String, &Question)> {
        self.domains
            .iter()
            .enumerate()
            .flat_map(|(d, domain)| {
                domain
                    .questions
                    .iter()
                    .enumerate()
                    .map(move |(q, question)| (format!("q-{}-{}", d + 1, q + 1), question))
            })
            .collect()
    }

    pub fn unresolved(&self) -> Vec<Value> {
        self.questions().into_iter().filter(|(id,_)| !self.amendments.iter().any(|a| &a.question_id == id && !matches!(a.class.as_str(),"unknown"|"conflict")))
            .map(|(id,q)| json!({"id":id,"question":q.question,"external_authority_required":q.external_authority_required})).collect()
    }

    pub fn observation(&self) -> Value {
        json!({"draft_version":self.version,"input_revision":self.input_revision,"source_sha256":self.source,"source_bytes":self.source_bytes,"selected_workspace_design":self.selected_file,
            "unresolved_questions":self.unresolved(),"amendments":self.amendments.iter().map(|a| json!({"question_id":a.question_id,"heading":a.heading,"target_section":a.target_section})).collect::<Vec<_>>(),
            "has_recovered_draft":self.imported_records.is_some(),
            "next_action":if self.domains.is_empty() && self.imported_records.is_none() {"Use design_research_plan to identify focused domain questions; research external obligations with web_search and web_open."}
                else if self.unresolved().is_empty() {"Review the retained draft against the complete source, then call design_publish; do not resend unchanged Markdown or records."}
                else {"Research the first unresolved question using primary sources, then save a design_amendment with evidence and acceptance. State observations do not resolve questions."}})
    }

    pub fn markdown(&self, source: &SourceDesignSnapshot) -> String {
        let mut text = format!(
            "# Expanded task design\n\n## Original source and current instructions\n\n{}\n",
            source.text()
        );
        if let Some(imported) = &self.imported_markdown {
            text.push_str("\n\n## Retained design, reconciled by this pass\n\n");
            text.push_str(imported);
        }
        for amendment in &self.amendments {
            use std::fmt::Write as _;
            write!(text,"\n\n## {}\n\nApplies to: {}\n\n{}\n\nRationale: {}\n\nFailure behavior: {}\n\nAcceptance:\n",amendment.heading,amendment.target_section,amendment.markdown,amendment.rationale,amendment.failure_behavior).expect("format String");
            for acceptance in &amendment.acceptance {
                writeln!(text, "- {acceptance}").expect("format String");
            }
        }
        text
    }

    pub fn records(
        &self,
        source: &SourceDesignSnapshot,
    ) -> Result<super::ExpansionRecords, ProductRunnerError> {
        let source_records = json!({
            "domains":[{"id":"expert-source","discipline":"Source preservation","applicability":"The complete source and incorporated user constraints govern this task","expected_sources":["exact source design"],"risks":["lost requirements"],"research_required":true,"external_authority_required":false,"internal_mechanical":null,"research_skip_rationale":null}],
            "evidence":[{"id":"expert-source-e","domain_ids":["expert-source"],"obligation_ids":["expert-source-o"],"title":"Complete immutable source contract","version":null,"authority":"source_design","reference":{"kind":"source_range","start":0,"end":source.metadata().bytes()}}],
            "obligations":[{"id":"expert-source-o","class":"user","statement":"Preserve all requirements, exclusions, values and current user constraints in the complete source design","origin":"exact source design","rationale":"Source preservation is the expansion contract","applicability":"entire implementation and review","affected_contracts":["source design"],"failure_behavior":"Keep unresolved conflicts explicit and preserve original bytes","evidence_ids":["expert-source-e"],"acceptance_ids":["expert-source-a"],"status":"supported"}],
            "acceptance_mappings":[{"id":"expert-source-a","criterion":"Original source bytes remain unchanged and the expanded design preserves every source requirement and current instruction","obligation_ids":["expert-source-o"]}],
            "change_ledger":[{"id":"expert-source-c","kind":"preserved","source_evidence_ids":["expert-source-e"],"obligation_ids":["expert-source-o"],"summary":"Preserve the complete exact source contract"}],
            "material_question":null,"expanded_markdown":null,"partial_markdown":null,"limitations":[]
        });
        let mut records = self.imported_records.clone().unwrap_or_else(|| source_records.clone());
        if self.imported_records.is_some() {
            for field in
                ["domains", "evidence", "obligations", "acceptance_mappings", "change_ledger"]
            {
                for entry in source_records[field]
                    .as_array()
                    .ok_or_else(|| failure("missing source contract"))?
                {
                    if let Some(old) = records[field]
                        .as_array_mut()
                        .ok_or_else(|| failure("invalid imported records"))?
                        .iter_mut()
                        .find(|old| old["id"] == entry["id"])
                    {
                        // These reserved rows describe the host's current source contract,
                        // including newly incorporated steering, never imported conclusions.
                        *old = entry.clone();
                    } else {
                        push(&mut records, field, entry.clone())?;
                    }
                }
            }
        }
        if let Some(file) = &self.selected_file {
            let current = json!({"id":"expert-source-file-e","domain_ids":["expert-source"],"obligation_ids":["expert-source-o"],"title":"Selected source design file","version":null,"authority":"repository","reference":{"kind":"repository","path":file["path"],"sha256":file["sha256"]}});
            if let Some(old) = records["evidence"]
                .as_array_mut()
                .ok_or_else(|| failure("missing evidence"))?
                .iter_mut()
                .find(|e| e["id"] == "expert-source-file-e")
            {
                *old = current;
            } else {
                push(&mut records, "evidence", current)?;
            }
            let obligation = records["obligations"]
                .as_array_mut()
                .and_then(|items| items.iter_mut().find(|item| item["id"] == "expert-source-o"))
                .ok_or_else(|| failure("source obligation missing"))?;
            obligation["evidence_ids"]
                .as_array_mut()
                .ok_or_else(|| failure("source evidence links missing"))?
                .push(json!("expert-source-file-e"));
        }
        let prefix = format!("expert-{}", &self.source[..12]);
        for (index, domain) in self.domains.iter().enumerate() {
            push(
                &mut records,
                "domains",
                json!({"id":format!("{prefix}-domain-{}",index+1),"discipline":domain.discipline,"applicability":domain.applicability,
                "expected_sources":domain.questions.iter().map(|q|q.question.clone()).collect::<Vec<_>>(),"risks":domain.risks,"research_required":true,
                "external_authority_required":domain.questions.iter().any(|q|q.external_authority_required),"internal_mechanical":null,"research_skip_rationale":null}),
            )?;
        }
        for (index, amendment) in self.amendments.iter().enumerate() {
            self.append_amendment(&mut records, &prefix, index, amendment)?;
        }
        super::ExpansionRecords::from_json(
            &serde_json::to_vec(&records).map_err(|e| failure(e.to_string()))?,
        )
    }
    fn append_amendment(
        &self,
        records: &mut Value,
        prefix: &str,
        index: usize,
        amendment: &Amendment,
    ) -> Result<(), ProductRunnerError> {
        let d = self
            .domains
            .iter()
            .enumerate()
            .find(|(di, domain)| {
                domain
                    .questions
                    .iter()
                    .enumerate()
                    .any(|(qi, _)| amendment.question_id == format!("q-{}-{}", di + 1, qi + 1))
            })
            .map(|(di, _)| format!("{prefix}-domain-{}", di + 1))
            .ok_or_else(|| failure("amendment names an unknown research question"))?;
        let o = format!("{prefix}-obligation-{}", index + 1);
        let mut evidence_ids = Vec::new();
        let mut source_ids = Vec::new();
        let mut acceptance_ids = Vec::new();
        for (ei, evidence) in amendment.evidence.iter().enumerate() {
            let id = format!("{prefix}-evidence-{}-{}", index + 1, ei + 1);
            evidence_ids.push(id.clone());
            if evidence.is_source() {
                source_ids.push(id.clone());
            }
            let mut evidence = evidence.value()?;
            evidence["id"] = json!(id);
            evidence["domain_ids"] = json!([d]);
            evidence["obligation_ids"] = json!([o]);
            push(records, "evidence", evidence)?;
        }
        for (ai, acceptance) in amendment.acceptance.iter().enumerate() {
            let id = format!("{prefix}-acceptance-{}-{}", index + 1, ai + 1);
            acceptance_ids.push(id.clone());
            push(
                records,
                "acceptance_mappings",
                json!({"id":id,"criterion":acceptance,"obligation_ids":[o]}),
            )?;
        }
        push(
            records,
            "obligations",
            json!({"id":o,"class":amendment.class,"statement":amendment.heading,"origin":amendment.question_id,
                "rationale":amendment.rationale,"applicability":amendment.target_section,"affected_contracts":amendment.affected_contracts,"failure_behavior":amendment.failure_behavior,
                "evidence_ids":evidence_ids,"acceptance_ids":acceptance_ids,"status":if matches!(amendment.class.as_str(),"conflict"|"unknown"){"unresolved"}else{"supported"}}),
        )?;
        push(
            records,
            "change_ledger",
            json!({"id":format!("{prefix}-change-{}",index+1),"kind":match amendment.class.as_str(){"user"=>"clarified","material_recommendation"|"optional"=>"recommended","conflict"|"unknown"=>"conflicting",_=>"added"},"source_evidence_ids":source_ids,"obligation_ids":[o],"summary":amendment.heading}),
        )?;
        Ok(())
    }
}

fn push(records: &mut Value, field: &str, value: Value) -> Result<(), ProductRunnerError> {
    records
        .get_mut(field)
        .and_then(Value::as_array_mut)
        .ok_or_else(|| failure("invalid imported design records"))?
        .push(value);
    Ok(())
}
