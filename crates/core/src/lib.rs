//! Serialization-independent accounting, calendar rules, and application ports.
pub mod calendar;
pub mod domain;
pub mod journal;
pub mod number;
pub mod storage;

pub use domain::Provenance;
