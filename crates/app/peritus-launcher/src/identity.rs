//! Operating-system random installation identity generation.

use peritus_product_state::InstallIdentity;

use crate::LauncherError;

pub fn generate() -> Result<InstallIdentity, LauncherError> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes).map_err(|error| LauncherError::Random(error.to_string()))?;
    let (store, actor) = bytes.split_at_mut(16);
    ensure_nonzero(store);
    ensure_nonzero(actor);
    let mut store_id = [0_u8; 16];
    store_id.copy_from_slice(store);
    let mut actor_id = [0_u8; 16];
    actor_id.copy_from_slice(actor);
    InstallIdentity::new(store_id, actor_id).map_err(LauncherError::from)
}

fn ensure_nonzero(bytes: &mut [u8]) {
    if bytes.iter().all(|byte| *byte == 0) {
        bytes[0] = 1;
    }
}
