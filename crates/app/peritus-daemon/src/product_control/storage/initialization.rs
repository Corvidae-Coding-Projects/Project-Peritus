//! Atomic initialization audit and retained C1 installed-manifest evidence.

use super::{ControlStore, Error};
use peritus_codec::sha256;
use peritus_journal::StateInstall;
use peritus_product_runner::control::{
    ControlError, ControlIntent, ControlOperation, ControlReceipt,
};

const INITIALIZATION_MANIFEST_NAMESPACE: u16 = 3543;

impl ControlStore {
    pub(crate) fn accept_initialization(
        &mut self,
        operation: &ControlOperation,
        manifest: Vec<u8>,
    ) -> Result<ControlReceipt, Error> {
        let ControlIntent::RecordInitialization { transaction_manifest_digest, .. } =
            operation.intent()
        else {
            return Err(ControlError::InvalidInput.into());
        };
        if sha256(&manifest).as_bytes() != transaction_manifest_digest {
            return Err(ControlError::InvalidInput.into());
        }
        if let Some(receipt) = self.resolve(operation)? {
            return Ok(receipt);
        }
        self.accept_installs(
            operation,
            vec![StateInstall::new(
                INITIALIZATION_MANIFEST_NAMESPACE,
                operation.id().as_bytes().to_vec(),
                None,
                1,
                manifest,
            )?],
        )
    }

    pub(super) fn verify_initialization_archive(
        &self,
        operation: &ControlOperation,
        position: u64,
    ) -> Result<(), Error> {
        let ControlIntent::RecordInitialization { transaction_manifest_digest, .. } =
            operation.intent()
        else {
            return Err(ControlError::InvalidInput.into());
        };
        let manifest = self
            .journal
            .state_record(INITIALIZATION_MANIFEST_NAMESPACE, operation.id().as_bytes())?
            .ok_or(Error::Corrupt("initialization transaction evidence missing"))?;
        if manifest.revision() != 1
            || manifest.producing_position() != position
            || sha256(manifest.bytes()).as_bytes() != transaction_manifest_digest
        {
            return Err(Error::Corrupt("initialization evidence differs from committed audit"));
        }
        Ok(())
    }
}
