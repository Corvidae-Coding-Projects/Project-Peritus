//! Independent Win32 fixture setup and raw descriptor oracle.
#![allow(unsafe_code, reason = "native ACL regression fixture and independent descriptor oracle")]

use core::ptr;
use std::{os::windows::ffi::OsStrExt as _, path::Path};
use windows_sys::Win32::{
    Foundation::LocalFree,
    Security::{
        Authorization::{
            ConvertSecurityDescriptorToStringSecurityDescriptorW,
            ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
        },
        DACL_SECURITY_INFORMATION, GetFileSecurityW, GetSecurityDescriptorControl,
        PROTECTED_DACL_SECURITY_INFORMATION, SetFileSecurityW,
        UNPROTECTED_DACL_SECURITY_INFORMATION,
    },
};

pub fn set(path: &Path, sddl: &str) {
    let name = path.as_os_str().encode_wide().chain([0]).collect::<Vec<_>>();
    let text = sddl.encode_utf16().chain([0]).collect::<Vec<_>>();
    let mut descriptor = ptr::null_mut();
    // SAFETY: terminated input, valid outputs, and the returned allocation stay live below.
    assert_ne!(
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                text.as_ptr(),
                SDDL_REVISION_1,
                &raw mut descriptor,
                ptr::null_mut(),
            )
        },
        0
    );
    let protection = if sddl.starts_with("D:P") {
        PROTECTED_DACL_SECURITY_INFORMATION
    } else {
        UNPROTECTED_DACL_SECURITY_INFORMATION
    };
    // SAFETY: the terminated path and complete descriptor are live; only fixture DACL is set.
    let result = unsafe {
        SetFileSecurityW(name.as_ptr(), DACL_SECURITY_INFORMATION | protection, descriptor)
    };
    // SAFETY: conversion transferred this LocalAlloc allocation to the fixture.
    unsafe { LocalFree(descriptor) };
    assert_ne!(result, 0);
}

pub fn snapshot(path: &Path) -> Vec<u32> {
    let name = path.as_os_str().encode_wide().chain([0]).collect::<Vec<_>>();
    let mut size = 0;
    // SAFETY: zero-capacity query with valid required-size output.
    unsafe {
        GetFileSecurityW(
            name.as_ptr(),
            DACL_SECURITY_INFORMATION,
            ptr::null_mut(),
            0,
            &raw mut size,
        )
    };
    assert!((20..=1_048_576).contains(&size));
    let mut buffer = vec![0_u32; (size as usize).div_ceil(4)];
    // SAFETY: aligned storage has at least size writable bytes and all inputs remain live.
    assert_ne!(
        unsafe {
            GetFileSecurityW(
                name.as_ptr(),
                DACL_SECURITY_INFORMATION,
                buffer.as_mut_ptr().cast(),
                size,
                &raw mut size,
            )
        },
        0
    );
    buffer
}

pub fn control(descriptor: &[u32]) -> u16 {
    let mut control = 0;
    let mut revision = 0;
    // SAFETY: callers pass complete aligned snapshots obtained from GetFileSecurityW.
    assert_ne!(
        unsafe {
            GetSecurityDescriptorControl(
                descriptor.as_ptr().cast_mut().cast(),
                &raw mut control,
                &raw mut revision,
            )
        },
        0
    );
    control
}

pub fn sddl(path: &Path) -> String {
    let descriptor = snapshot(path);
    let mut text = ptr::null_mut();
    let mut length = 0;
    // SAFETY: descriptor is complete/aligned and returned buffer outputs are valid.
    assert_ne!(
        unsafe {
            ConvertSecurityDescriptorToStringSecurityDescriptorW(
                descriptor.as_ptr().cast_mut().cast(),
                SDDL_REVISION_1,
                DACL_SECURITY_INFORMATION,
                &raw mut text,
                &raw mut length,
            )
        },
        0
    );
    // SAFETY: the API returned length initialized UTF-16 units, including the terminator.
    let units = unsafe { core::slice::from_raw_parts(text, length as usize) };
    let result = String::from_utf16(units.strip_suffix(&[0]).unwrap_or(units)).unwrap();
    // SAFETY: conversion transferred this LocalAlloc allocation to the fixture.
    unsafe { LocalFree(text.cast()) };
    result
}

/// Schedules native ACL fixture mutations; explicit overlap probes bypass this test-only gate.
pub fn serial() -> std::sync::MutexGuard<'static, ()> {
    static GATE: std::sync::Mutex<()> = std::sync::Mutex::new(());
    GATE.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}
