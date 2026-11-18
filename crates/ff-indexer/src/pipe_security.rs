//! Segurança do Named Pipe (Windows): só o usuário que iniciou o indexador (e o SYSTEM) pode abrir o pipe.
//!
//! Sem `SECURITY_ATTRIBUTES`, o Windows aplica a DACL padrão do pipe, que dá leitura a *Everyone* e
//! *Anonymous*: outro usuário local conseguiria consultar o índice de nomes (dado sensível).

use std::ffi::c_void;
use std::io;
use std::ptr;

use windows_sys::Win32::Foundation::{CloseHandle, LocalFree, HANDLE};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows_sys::Win32::Security::{
    GetTokenInformation, TokenUser, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES, TOKEN_QUERY,
    TOKEN_USER,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

/// Descritor de segurança dono-apenas, pronto para `CreateNamedPipeW`.
pub struct PipeSecurity {
    descriptor: PSECURITY_DESCRIPTOR,
    attributes: SECURITY_ATTRIBUTES,
}

impl PipeSecurity {
    /// Cria a DACL protegida: acesso total apenas ao SID do usuário atual e ao SYSTEM.
    pub fn for_current_user() -> io::Result<Self> {
        let sid = current_user_sid_string()?;
        Self::from_sddl(&sddl_for_sid(&sid))
    }

    fn from_sddl(sddl: &str) -> io::Result<Self> {
        let wide: Vec<u16> = sddl.encode_utf16().chain(std::iter::once(0)).collect();
        let mut descriptor: PSECURITY_DESCRIPTOR = ptr::null_mut();
        // SAFETY: `wide` é UTF-16 terminada em NUL e viva durante a chamada; `descriptor` é um ponteiro
        // local para saída, e o resultado é liberado com `LocalFree` em `Drop`.
        let ok = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                wide.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                ptr::null_mut(),
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self {
            descriptor,
            attributes: SECURITY_ATTRIBUTES {
                nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: descriptor,
                bInheritHandle: 0,
            },
        })
    }

    /// Ponteiro para passar como `lpSecurityAttributes`; válido enquanto `self` viver.
    pub fn as_ptr(&self) -> *const SECURITY_ATTRIBUTES {
        &self.attributes
    }
}

impl Drop for PipeSecurity {
    fn drop(&mut self) {
        // SAFETY: `descriptor` foi alocado por `ConvertStringSecurityDescriptorToSecurityDescriptorW`
        // (que exige `LocalFree`) e é liberado uma única vez aqui.
        unsafe { LocalFree(self.descriptor) };
    }
}

// SAFETY: o descritor é imutável após a criação e só é liberado em `Drop`.
unsafe impl Send for PipeSecurity {}

/// `D:P` = DACL protegida (sem herança); `GA` = acesso total; `SY` = LocalSystem.
fn sddl_for_sid(sid: &str) -> String {
    format!("D:P(A;;GA;;;{sid})(A;;GA;;;SY)")
}

fn current_user_sid_string() -> io::Result<String> {
    let mut token: HANDLE = ptr::null_mut();
    // SAFETY: `GetCurrentProcess` é um pseudo-handle sempre válido e `token` é local para saída.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let result = token_user_sid(token);
    // SAFETY: `token` foi aberto acima e é fechado uma única vez.
    unsafe { CloseHandle(token) };
    result
}

fn token_user_sid(token: HANDLE) -> io::Result<String> {
    let mut len = 0u32;
    // SAFETY: consulta de tamanho: buffer nulo com tamanho 0; falha esperada com `len` preenchido.
    unsafe { GetTokenInformation(token, TokenUser, ptr::null_mut(), 0, &mut len) };
    if len == 0 {
        return Err(io::Error::last_os_error());
    }
    // `u64` garante alinhamento de 8 bytes para `TOKEN_USER` (contém ponteiro).
    let mut buf = vec![0u64; (len as usize).div_ceil(8)];
    // SAFETY: `buf` tem pelo menos `len` bytes e vive durante a chamada.
    let ok = unsafe {
        GetTokenInformation(
            token,
            TokenUser,
            buf.as_mut_ptr() as *mut c_void,
            len,
            &mut len,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: a chamada acima preencheu `buf` com um `TOKEN_USER` válido e alinhado.
    let sid = unsafe { (*(buf.as_ptr() as *const TOKEN_USER)).User.Sid };
    let mut wide: *mut u16 = ptr::null_mut();
    // SAFETY: `sid` aponta para dentro de `buf` (vivo); `wide` é saída liberada com `LocalFree` abaixo.
    if unsafe { ConvertSidToStringSidW(sid, &mut wide) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let mut n = 0usize;
    // SAFETY: `wide` é uma string UTF-16 terminada em NUL devolvida pelo sistema.
    while unsafe { *wide.add(n) } != 0 {
        n += 1;
    }
    // SAFETY: `wide` tem `n` unidades u16 válidas (medidas acima).
    let s = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(wide, n) });
    // SAFETY: `wide` foi alocado pelo sistema e exige `LocalFree`; não é usado depois.
    unsafe { LocalFree(wide as *mut c_void) };
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::Security::Authorization::ConvertSecurityDescriptorToStringSecurityDescriptorW;
    use windows_sys::Win32::Security::DACL_SECURITY_INFORMATION;

    fn dacl_string(sec: &PipeSecurity) -> String {
        let mut out: *mut u16 = ptr::null_mut();
        // SAFETY: `descriptor` é válido enquanto `sec` viver; `out` é saída liberada abaixo.
        let ok = unsafe {
            ConvertSecurityDescriptorToStringSecurityDescriptorW(
                sec.descriptor,
                SDDL_REVISION_1,
                DACL_SECURITY_INFORMATION,
                &mut out,
                ptr::null_mut(),
            )
        };
        assert_ne!(ok, 0);
        let mut n = 0;
        // SAFETY: `out` é UTF-16 terminada em NUL devolvida pelo sistema.
        while unsafe { *out.add(n) } != 0 {
            n += 1;
        }
        // SAFETY: `out` tem `n` unidades válidas e é liberada uma vez.
        let s = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(out, n) });
        // SAFETY: `out` foi alocado pelo sistema e não é usado depois.
        unsafe { LocalFree(out as *mut c_void) };
        s
    }

    #[test]
    fn dacl_is_protected_and_only_user_and_system() {
        let sid = current_user_sid_string().unwrap();
        assert!(sid.starts_with("S-1-5-"), "SID inesperado: {sid}");
        let sec = PipeSecurity::for_current_user().unwrap();
        let dacl = dacl_string(&sec);
        assert!(dacl.starts_with("D:P"), "DACL deve ser protegida: {dacl}");
        assert!(dacl.contains(&sid), "usuário atual ausente: {dacl}");
        assert!(dacl.contains(";;;SY)"), "SYSTEM ausente: {dacl}");
        assert_eq!(dacl.matches("(A;").count(), 2, "só 2 ACEs: {dacl}");
        for broad in [";;;WD)", ";;;AN)", ";;;BU)", ";;;AU)", ";;;BA)"] {
            assert!(!dacl.contains(broad), "ACE ampla {broad}: {dacl}");
        }
    }

    #[test]
    fn same_user_can_connect_to_protected_pipe() {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
        use windows_sys::Win32::Storage::FileSystem::PIPE_ACCESS_DUPLEX;
        use windows_sys::Win32::System::Pipes::{
            ConnectNamedPipe, CreateNamedPipeW, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE,
            PIPE_WAIT,
        };

        let name = format!(r"\\.\pipe\fastfile-sectest-{}", std::process::id());
        let wide: Vec<u16> = std::ffi::OsStr::new(&name)
            .encode_wide()
            .chain(Some(0))
            .collect();
        let sec = PipeSecurity::for_current_user().unwrap();
        // SAFETY: `wide` é UTF-16 terminada em NUL e `sec` vive além da chamada.
        let pipe = unsafe {
            CreateNamedPipeW(
                wide.as_ptr(),
                PIPE_ACCESS_DUPLEX,
                PIPE_TYPE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                4096,
                4096,
                0,
                sec.as_ptr(),
            )
        };
        assert_ne!(pipe, INVALID_HANDLE_VALUE);
        let raw = pipe as usize;
        let t = std::thread::spawn(move || {
            // SAFETY: `raw` é o handle do pipe criado acima; só esta thread o usa.
            unsafe { ConnectNamedPipe(raw as HANDLE, ptr::null_mut()) };
        });
        let client = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&name);
        assert!(
            client.is_ok(),
            "o próprio usuário deve conseguir conectar: {client:?}"
        );
        t.join().unwrap();
        // SAFETY: `pipe` é válido e fechado uma única vez.
        unsafe { CloseHandle(pipe) };
    }
}
