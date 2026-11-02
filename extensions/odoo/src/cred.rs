//! The Odoo API key in Windows Credential Manager. It is never written to a file, a log, a message to the notch or an
//! error text. The setup window saves it under this extension's own name; a key the Office edition saved is still read,
//! so an existing setup keeps working.

use std::ffi::c_void;
use windows::core::{w, PCWSTR, PWSTR};
use windows::Win32::Security::Credentials::{CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_FLAGS, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC};

const OWN: PCWSTR = w!("HesewakeOdoo/Odoo");
const OFFICE: PCWSTR = w!("HesewakeNotchOffice/Odoo");

fn read(target: PCWSTR) -> Option<String> {
    unsafe {
        let mut p: *mut CREDENTIALW = std::ptr::null_mut();
        CredReadW(target, CRED_TYPE_GENERIC, 0, &mut p).ok()?;
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

pub fn get_key() -> Option<String> {
    read(OWN).or_else(|| read(OFFICE))
}

pub fn has_key() -> bool {
    get_key().is_some()
}

/// Save the key (UTF-8, as the Office edition did) under this extension's name.
pub fn set_key(login: &str, key: &str) -> Result<(), String> {
    if cfg!(test) {
        return Ok(());
    }
    let mut user: Vec<u16> = login.encode_utf16().chain(Some(0)).collect();
    let mut blob = key.as_bytes().to_vec();
    let c = CREDENTIALW {
        Flags: CRED_FLAGS(0),
        Type: CRED_TYPE_GENERIC,
        TargetName: PWSTR(OWN.0 as *mut u16),
        CredentialBlobSize: blob.len() as u32,
        CredentialBlob: blob.as_mut_ptr(),
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        UserName: PWSTR(user.as_mut_ptr()),
        ..Default::default()
    };
    let r = unsafe { CredWriteW(&c, 0) };
    blob.iter_mut().for_each(|b| *b = 0);
    r.map_err(|_| "Could not save the API key in Credential Manager".to_string())
}
