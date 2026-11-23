//! Relatórios: agrupamento por período, totais por categoria/conta e renderização.

pub mod group;
pub mod render;
pub mod summary;

pub use group::{FiscalCalendar, Period};
pub use summary::{CategoryTotals, Summary};
