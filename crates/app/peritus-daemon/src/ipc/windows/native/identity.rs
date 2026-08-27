//! Bounded Win32 token and SID decoding.

use std::{io, mem::size_of, ptr, slice};

use windows_sys::Win32::Security::{
    GetLengthSid, GetTokenInformation, IsValidSid, TOKEN_USER, TokenUser,
};

use super::{MAX_SID_BYTES, MAX_TOKEN_INFORMATION_BYTES, MINIMUM_SID_BYTES, OwnedToken};

pub(super) fn token_user_sid(token: &OwnedToken) -> io::Result<Vec<u8>> {
    let mut required = 0_u32;
    // SAFETY: the null buffer/zero length pair is the documented sizing query and required is
    // writable. The token handle remains owned for both calls.
    unsafe {
        GetTokenInformation(token.0, TokenUser, ptr::null_mut(), 0, &mut required);
    }
    let required = usize::try_from(required).map_err(|_| invalid_native_data())?;
    if required < size_of::<TOKEN_USER>() || required > MAX_TOKEN_INFORMATION_BYTES {
        return Err(invalid_native_data());
    }
    let mut storage = vec![0_u64; required.div_ceil(size_of::<u64>())];
    let mut written = 0_u32;
    // SAFETY: storage is aligned for TOKEN_USER and provides at least required writable bytes.
    if unsafe {
        GetTokenInformation(
            token.0,
            TokenUser,
            storage.as_mut_ptr().cast(),
            u32::try_from(required).map_err(|_| invalid_native_data())?,
            &mut written,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let written = usize::try_from(written).map_err(|_| invalid_native_data())?;
    if written < size_of::<TOKEN_USER>() || written > required {
        return Err(invalid_native_data());
    }
    let base = storage.as_ptr().cast::<u8>().addr();
    let end = base.checked_add(written).ok_or_else(invalid_native_data)?;
    // SAFETY: storage is TOKEN_USER-aligned and the successful call initialized its prefix.
    let token_user = unsafe { &*storage.as_ptr().cast::<TOKEN_USER>() };
    let sid_address = token_user.User.Sid.addr();
    let minimum_end = sid_address.checked_add(MINIMUM_SID_BYTES).ok_or_else(invalid_native_data)?;
    if sid_address < base || minimum_end > end {
        return Err(invalid_native_data());
    }
    // SAFETY: the successful call initialized exactly written bytes in this live allocation.
    let initialized = unsafe { slice::from_raw_parts(storage.as_ptr().cast::<u8>(), written) };
    let sid_offset = sid_address.checked_sub(base).ok_or_else(invalid_native_data)?;
    let sub_authorities = usize::from(initialized[sid_offset + 1]);
    let claimed_length = MINIMUM_SID_BYTES
        .checked_add(sub_authorities.checked_mul(size_of::<u32>()).ok_or_else(invalid_native_data)?)
        .ok_or_else(invalid_native_data)?;
    if claimed_length > MAX_SID_BYTES
        || sid_offset.checked_add(claimed_length).is_none_or(|value| value > written)
    {
        return Err(invalid_native_data());
    }
    // SAFETY: the complete claimed SID range was proven to lie within initialized storage.
    if unsafe { IsValidSid(token_user.User.Sid) } == 0 {
        return Err(invalid_native_data());
    }
    // SAFETY: IsValidSid succeeded for this in-buffer SID pointer.
    let sid_length = usize::try_from(unsafe { GetLengthSid(token_user.User.Sid) })
        .map_err(|_| invalid_native_data())?;
    let sid_end = sid_address.checked_add(sid_length).ok_or_else(invalid_native_data)?;
    if sid_length < MINIMUM_SID_BYTES || sid_length > MAX_SID_BYTES || sid_end > end {
        return Err(invalid_native_data());
    }
    Ok(initialized[sid_offset..sid_offset + sid_length].to_vec())
}

pub(super) fn aligned_sid(bytes: &[u8]) -> io::Result<Vec<u32>> {
    if bytes.len() < MINIMUM_SID_BYTES || bytes.len() > MAX_SID_BYTES {
        return Err(invalid_native_data());
    }
    let mut words = Vec::with_capacity(bytes.len().div_ceil(size_of::<u32>()));
    for chunk in bytes.chunks(size_of::<u32>()) {
        let mut word = [0_u8; size_of::<u32>()];
        word[..chunk.len()].copy_from_slice(chunk);
        words.push(u32::from_ne_bytes(word));
    }
    // SAFETY: words is DWORD-aligned, contains the exact SID bytes, and has the minimum header.
    if unsafe { IsValidSid(words.as_mut_ptr().cast()) } == 0 {
        return Err(invalid_native_data());
    }
    // SAFETY: the preceding validation succeeded for this same live SID pointer.
    let length = usize::try_from(unsafe { GetLengthSid(words.as_mut_ptr().cast()) })
        .map_err(|_| invalid_native_data())?;
    if length != bytes.len() {
        return Err(invalid_native_data());
    }
    Ok(words)
}

pub(super) fn invalid_native_data() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "Win32 returned invalid security data")
}
