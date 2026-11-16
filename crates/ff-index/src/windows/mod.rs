//! Implementações específicas para Windows / NTFS (§2.1, §2.3 e Checkpoints A, C, D, E).

#![cfg(windows)]

pub mod mft;
pub mod usn;
pub mod volume;

pub use mft::{MftError, MftSource};
pub use usn::{UsnError, UsnJournalDataV0, UsnRecordV2, UsnSource, UsnWatcher};
pub use volume::{format_volume_device_path, open_volume};
