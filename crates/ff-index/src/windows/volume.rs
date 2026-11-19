//! Abertura e manipulação segura de volumes Windows / NTFS.

#![cfg(windows)]

use std::ffi::OsStr;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use windows_sys::Win32::Foundation::{GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_NO_BUFFERING, FILE_SHARE_READ,
    FILE_SHARE_WRITE, OPEN_EXISTING,
};

/// Normaliza um identificador de volume para o formato Win32 Device (`\\.\C:`).
pub fn format_volume_device_path(target: &str) -> String {
    let clean = target.trim().trim_end_matches(['\\', '/']);
    if clean.starts_with(r"\\.\") {
        clean.to_string()
    } else if clean.len() == 2 && clean.ends_with(':') {
        format!(r"\\.\{clean}")
    } else if clean.len() == 1 && clean.chars().next().unwrap().is_ascii_alphabetic() {
        format!(r"\\.\{clean}:")
    } else if let Some(drive_letter) = Path::new(clean)
        .components()
        .next()
        .and_then(|c| c.as_os_str().to_str())
    {
        let drive = drive_letter.trim_end_matches(['\\', '/']);
        if drive.ends_with(':') {
            format!(r"\\.\{drive}")
        } else {
            format!(r"\\.\{clean}")
        }
    } else {
        format!(r"\\.\{clean}")
    }
}

/// Abre um manipulador para o volume NTFS especificado (exige privilégio elevado para leitura física).
pub fn open_volume(target: &str, write_access: bool) -> io::Result<HANDLE> {
    open_volume_with_flags(target, write_access, 0)
}

/// Abre o volume somente leitura **sem o cache do sistema** (`FILE_FLAG_NO_BUFFERING`).
///
/// Evita que o cache de arquivos fatie e copie leituras grandes e sequenciais (como a `$MFT` inteira).
/// Exige que o deslocamento, o tamanho e o endereço do buffer de cada leitura sejam múltiplos do tamanho
/// do setor (ver `VolumeReader::alignment`).
pub fn open_volume_unbuffered(target: &str) -> io::Result<HANDLE> {
    open_volume_with_flags(target, false, FILE_FLAG_NO_BUFFERING)
}

fn open_volume_with_flags(
    target: &str,
    write_access: bool,
    extra_flags: u32,
) -> io::Result<HANDLE> {
    let dev_path = format_volume_device_path(target);
    let mut wide: Vec<u16> = OsStr::new(&dev_path).encode_wide().collect();
    wide.push(0);

    let access = if write_access {
        GENERIC_READ | GENERIC_WRITE
    } else {
        GENERIC_READ
    };

    // SAFETY: `wide` é uma string UTF-16 terminada em NUL e viva durante a chamada.
    let handle = unsafe {
        CreateFileW(
            wide.as_ptr(),
            access,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | extra_flags,
            std::ptr::null_mut(),
        )
    };

    if handle == INVALID_HANDLE_VALUE {
        Err(io::Error::last_os_error())
    } else {
        Ok(handle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_volume_device_path() {
        assert_eq!(format_volume_device_path("C:"), r"\\.\C:");
        assert_eq!(format_volume_device_path("C:\\"), r"\\.\C:");
        assert_eq!(format_volume_device_path("c"), r"\\.\c:");
        assert_eq!(format_volume_device_path(r"\\.\D:"), r"\\.\D:");
    }
}
