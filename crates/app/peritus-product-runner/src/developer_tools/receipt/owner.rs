use super::*;

impl EffectReceiptLedger {
    pub(in crate::developer_tools) fn bind_native_command_owner(
        &mut self,
        call_id: &str,
        owner: NativeCommandOwner,
    ) -> Result<(), DeveloperLoopError> {
        self.load()?;
        let ordinal = self.next_ordinal;
        let existing = self
            .entries
            .get(&ordinal)
            .cloned()
            .ok_or_else(|| tool("native command owner has no started effect receipt"))?;
        if existing.state != ReceiptState::Started
            || existing.call_id != call_id
            || !command_effect(&existing.tool)
            || existing.native_owner.is_some()
        {
            return Err(tool("native command owner differs from its started effect receipt"));
        }
        let record =
            ReceiptRecord { version: FORMAT_VERSION, native_owner: Some(owner), ..existing };
        self.append(&record)?;
        self.entries.insert(ordinal, record.clone());
        self.all_entries.insert((record.scope.clone(), ordinal), record);
        Ok(())
    }

    pub(in crate::developer_tools) fn command_owner_for_handle(
        &mut self,
        handle: &str,
    ) -> Result<Option<NativeCommandOwner>, DeveloperLoopError> {
        self.load()?;
        Ok(self.all_entries.values().find_map(|record| {
            if !command_effect(&record.tool) {
                return None;
            }
            let owner = record.native_owner?;
            (native_action_hex(owner.action) == handle).then_some(owner)
        }))
    }

    pub(in crate::developer_tools) fn uncertain_command_owners(
        &mut self,
    ) -> Result<Vec<NativeCommandOwner>, DeveloperLoopError> {
        self.load()?;
        let mut owners = Vec::new();
        for record in self.all_entries.values() {
            if !matches!(record.state, ReceiptState::Started | ReceiptState::Ambiguous) {
                continue;
            }
            if let Some(owner) = record.native_owner
                && !owners.contains(&owner)
            {
                owners.push(owner);
            }
        }
        Ok(owners)
    }

    pub(in crate::developer_tools) fn reconcile_native_command_owner(
        &mut self,
        owner: NativeCommandOwner,
        disposition: peritus_process::RecoveryDisposition,
        value: &Value,
    ) -> Result<(), DeveloperLoopError> {
        let keys: Vec<_> = self
            .all_entries
            .iter()
            .filter_map(|(key, record)| {
                (record.native_owner == Some(owner)
                    && matches!(record.state, ReceiptState::Started | ReceiptState::Ambiguous))
                .then_some((key.clone(), record.tool.clone()))
            })
            .collect();
        for ((scope, ordinal), tool_name) in keys {
            match disposition {
                peritus_process::RecoveryDisposition::Terminal
                    if matches!(tool_name.as_str(), "run_command" | "command_start") =>
                {
                    let is_error = value.get("success").and_then(Value::as_bool) == Some(false);
                    self.complete_recovered_command(&scope, ordinal, owner, value, is_error, true)?;
                }
                peritus_process::RecoveryDisposition::Terminal
                | peritus_process::RecoveryDisposition::AbsentUnobserved => {
                    self.mark_recovered_command_unknown(&scope, ordinal, owner, true)?;
                }
                peritus_process::RecoveryDisposition::LiveOwned
                | peritus_process::RecoveryDisposition::Indeterminate => {}
            }
        }
        Ok(())
    }

    pub(in crate::developer_tools) fn complete_recovered_command(
        &mut self,
        scope: &str,
        ordinal: u32,
        owner: NativeCommandOwner,
        value: &Value,
        is_error: bool,
        owner_inactive: bool,
    ) -> Result<(), DeveloperLoopError> {
        let key = (scope.to_owned(), ordinal);
        let existing = self
            .all_entries
            .get(&key)
            .cloned()
            .ok_or_else(|| tool("recovered native command receipt disappeared"))?;
        if !matches!(existing.state, ReceiptState::Started | ReceiptState::Ambiguous)
            || existing.native_owner != Some(owner)
        {
            return Err(tool("recovered native command owner differs from its receipt"));
        }
        let applied = ReceiptRecord {
            version: FORMAT_VERSION,
            state: ReceiptState::Applied,
            owner_inactive,
            output: Some(value.clone()),
            is_error: Some(is_error),
            ..existing
        };
        self.append(&applied)?;
        let completed = ReceiptRecord { state: ReceiptState::Completed, ..applied };
        self.append(&completed)?;
        self.all_entries.insert(key, completed.clone());
        if scope == self.scope {
            self.entries.insert(ordinal, completed);
        }
        Ok(())
    }

    pub(in crate::developer_tools) fn mark_recovered_command_unknown(
        &mut self,
        scope: &str,
        ordinal: u32,
        owner: NativeCommandOwner,
        owner_inactive: bool,
    ) -> Result<(), DeveloperLoopError> {
        let key = (scope.to_owned(), ordinal);
        let existing = self
            .all_entries
            .get(&key)
            .cloned()
            .ok_or_else(|| tool("uncertain native command receipt disappeared"))?;
        if !matches!(existing.state, ReceiptState::Started | ReceiptState::Ambiguous)
            || existing.native_owner != Some(owner)
        {
            return Err(tool("uncertain native command owner differs from its receipt"));
        }
        if existing.state == ReceiptState::Ambiguous && existing.owner_inactive && owner_inactive {
            return Ok(());
        }
        let record = ReceiptRecord {
            version: FORMAT_VERSION,
            state: ReceiptState::Ambiguous,
            owner_inactive,
            ..existing
        };
        self.append(&record)?;
        self.all_entries.insert(key, record.clone());
        if scope == self.scope {
            self.entries.insert(ordinal, record);
        }
        Ok(())
    }
}
