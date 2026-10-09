//! Pristine DACL and retained-object inventory for the exact planned target subtrees.

use super::acl_error;
use crate::{
    ResolvedWindowsPath, WindowsError, WindowsOperation, WindowsPath,
    native::acl::{AclObject, ObjectId, VolumeReservations},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

pub(super) struct Snapshots {
    objects: Vec<AclObject>,
    identities: BTreeMap<ObjectId, usize>,
    created: BTreeSet<usize>,
    principal: String,
    temporary: Vec<(bool, u32)>,
    reserved: VolumeReservations,
}

impl Snapshots {
    pub(super) const fn new() -> Self {
        Self {
            objects: Vec::new(),
            identities: BTreeMap::new(),
            created: BTreeSet::new(),
            principal: String::new(),
            temporary: Vec::new(),
            reserved: VolumeReservations::new(),
        }
    }

    pub(super) fn configure(&mut self, plan: &crate::AclPlan) {
        use peritus_sandbox::{FileOperation, RuleEffect};
        self.principal.clone_from(&plan.principal_sid);
        self.temporary = plan
            .entries
            .iter()
            .map(|entry| {
                let mask = [
                    (FileOperation::Discover, 1),
                    (FileOperation::Metadata, 0x80),
                    (FileOperation::Read, 1),
                    (FileOperation::Execute, 0x20),
                    (FileOperation::Create, 4),
                    (FileOperation::Write, 2),
                    (FileOperation::Remove, 0x1_0000),
                ]
                .into_iter()
                .filter(|(operation, _)| entry.access().contains(*operation))
                .fold(0, |mask, (_, bit)| mask | bit);
                (entry.effect() == RuleEffect::Deny, mask)
            })
            .collect();
    }

    pub(super) fn reserve(&mut self, plan: &crate::AclPlan) -> Result<(), WindowsError> {
        self.reserved = VolumeReservations::acquire(plan)?;
        Ok(())
    }

    pub(super) fn capture_target(
        &mut self,
        path: &Path,
        created: Option<&std::fs::File>,
    ) -> Result<usize, WindowsError> {
        let Some(file) = created else {
            return self.capture(path, false);
        };
        let file = file.try_clone().map_err(|_| {
            acl_error(WindowsOperation::InstallAcl, "owned anchor handle cannot be retained")
        })?;
        let object = AclObject::from_retained(file, path, Some(&self.reserved))?;
        let index = self.objects.len();
        self.identities.insert(object.identity(), index);
        self.objects.push(object);
        self.created.insert(index);
        Ok(index)
    }

    pub(super) fn capture(&mut self, path: &Path, created: bool) -> Result<usize, WindowsError> {
        for index in &self.created {
            if self.objects[*index].is_original_path(path)? {
                return Ok(*index);
            }
        }
        let path = WindowsPath::from_os_str(path.as_os_str())?;
        ResolvedWindowsPath::resolve(path.clone())?;
        let object = AclObject::capture_reserved(&path.to_path_buf(), Some(&self.reserved))?;
        if let Some(index) = self.identities.get(&object.identity()) {
            return Ok(*index);
        }
        let index = self.objects.len();
        self.identities.insert(object.identity(), index);
        self.objects.push(object);
        if created {
            self.created.insert(index);
        }
        Ok(index)
    }

    pub(super) fn capture_descendants(
        &mut self,
        private_backup: &Path,
    ) -> Result<(), WindowsError> {
        let backup = WindowsPath::from_canonicalized(
            &std::fs::canonicalize(private_backup).map_err(|_| {
                acl_error(WindowsOperation::InstallAcl, "ACL backup identity cannot be resolved")
            })?,
        )?;
        let mut pending = (0..self.objects.len()).collect::<Vec<_>>();
        let mut visited = BTreeSet::new();
        while let Some(index) = pending.pop() {
            if !visited.insert(index)
                || self.created.contains(&index)
                || !self.objects[index].is_directory()
            {
                continue;
            }
            let _stable = self.objects[index].stabilize()?;
            let path = self.objects[index].path()?;
            let target = WindowsPath::from_os_str(path.as_os_str())?;
            if target.contains(&backup) {
                return Err(acl_error(
                    WindowsOperation::InstallAcl,
                    "ACL backup must be outside every affected target subtree",
                ));
            }
            for entry in std::fs::read_dir(&path).map_err(|_| {
                acl_error(
                    WindowsOperation::InstallAcl,
                    "ACL target subtree cannot be enumerated before mutation",
                )
            })? {
                let entry = entry.map_err(|_| {
                    acl_error(
                        WindowsOperation::InstallAcl,
                        "ACL descendant cannot be inspected before mutation",
                    )
                })?;
                let child = self.capture(&entry.path(), false)?;
                if !self.objects[index].same_volume(&self.objects[child]) {
                    return Err(acl_error(
                        WindowsOperation::InstallAcl,
                        "ACL descendant escaped its target volume",
                    ));
                }
                pending.push(child);
            }
        }
        Ok(())
    }

    pub(super) fn get(&self, index: usize) -> &AclObject {
        &self.objects[index]
    }

    pub(super) fn stabilize_all(&self) -> Result<crate::native::acl::Stabilizers, WindowsError> {
        let mut handles = crate::native::acl::Stabilizers::new();
        for (index, object) in self.objects.iter().enumerate() {
            if self.created.contains(&index) {
                object.stabilize_created_into(&mut handles)?;
            } else {
                object.stabilize_into(&mut handles)?;
            }
        }
        Ok(handles)
    }

    pub(super) fn restore_exact(&self) -> Result<(), WindowsError> {
        let mut failed = false;
        for (index, object) in self.objects.iter().enumerate() {
            if !self.created.contains(&index) {
                failed |= object.restore_exact().is_err();
            }
        }
        if failed {
            Err(acl_error(
                WindowsOperation::RestoreAcl,
                "one or more original DACLs failed exact restoration or verification",
            ))
        } else {
            Ok(())
        }
    }

    pub(super) fn restore_descendant_inheritance(&self) -> Result<(), WindowsError> {
        let mut failed = false;
        // A preexisting directory may have moved during execution; its retained handle still
        // owns restoration of inherited temporary ACEs on children created below that object.
        for (index, object) in self.objects.iter().enumerate() {
            if object.is_directory() && !self.created.contains(&index) {
                match object.delete_pending() {
                    Ok(false) => failed |= object.restore_inheritance().is_err(),
                    Ok(true) => {}
                    Err(_) => failed = true,
                }
            }
        }
        if failed {
            Err(acl_error(
                WindowsOperation::RestoreAcl,
                "descendant inheritance restoration remains incomplete",
            ))
        } else {
            Ok(())
        }
    }

    pub(super) fn verify_new_descendants(&self) -> Result<(), WindowsError> {
        let mut pending = Vec::new();
        let mut excluded = Vec::new();
        for (index, object) in self.objects.iter().enumerate() {
            if self.created.contains(&index) || object.delete_pending()? {
                excluded.push(index);
            } else if object.is_directory() {
                pending.push(object.current_clone()?);
            }
        }
        let mut visited = BTreeSet::new();
        while let Some(parent) = pending.pop() {
            if !visited.insert(parent.identity()) {
                continue;
            }
            let (path, _stable) = parent.stabilize_current()?;
            for entry in std::fs::read_dir(path).map_err(|_| {
                acl_error(WindowsOperation::RestoreAcl, "new descendants cannot be inspected")
            })? {
                let path = entry
                    .map_err(|_| {
                        acl_error(
                            WindowsOperation::RestoreAcl,
                            "new descendant entry cannot be inspected",
                        )
                    })?
                    .path();
                let mut skip = false;
                for index in &excluded {
                    if self.objects[*index].is_original_path(&path)? {
                        skip = true;
                        break;
                    }
                }
                if skip {
                    continue;
                }
                let child = AclObject::capture(&path)?;
                if !parent.same_volume(&child) {
                    return Err(acl_error(
                        WindowsOperation::RestoreAcl,
                        "cleanup descendant escaped its parent volume",
                    ));
                }
                if !self.identities.contains_key(&child.identity()) {
                    child.verify_new_child(&parent, &self.principal, &self.temporary)?;
                }
                if child.is_directory() {
                    pending.push(child);
                }
            }
        }
        Ok(())
    }

    pub(super) fn quarantine_process_lifetime(&mut self) {
        self.reserved.quarantine_process_lifetime();
    }

    pub(super) fn clear(&mut self) {
        self.objects.clear();
        self.identities.clear();
        self.created.clear();
        self.reserved.clear();
    }
}
