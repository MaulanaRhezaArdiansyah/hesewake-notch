//! The Odoo API key, read from Windows Credential Manager where the Office edition keeps it. It is never written to a
//! file, a log, a message to the notch or an error text.

use std::ffi::c_void;
use windows::core::{w, PCWSTR};
use windows::Win32::Security::Credentials::{CredFree, CredReadW, CREDENTIALW, CRED_TYPE_GENERIC};

const TARGET: PCWSTR = w!("HesewakeNotchOffice/Odoo");

pub fn get_key() -> Option<String> {
    unsafe {
        let mut p: *mut CREDENTIALW = std::ptr::null_mut();
        CredReadW(TARGET, CRED_TYPE_GENERIC, 0, &mut p).ok()?;
        if p.is_null() {
            return None;
        }
        let c = &*p;
        let out = if c.CredentialBlob.is_null() || c.CredentialBlobSize == 0 {
            None
        } else {
            String::from_utf8(std::slice::from_raw_parts(c.CredentialBlob, c.CredentialBlobSize as usize).to_vec()).ok()
        };
        CredFree(p as *const c_void);
        out.filter(|k| !k.is_empty())
    }
}

pub fn has_key() -> bool {
    get_key().is_some()
}
