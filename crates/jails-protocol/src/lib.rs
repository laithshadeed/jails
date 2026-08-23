//! The values every closed jails format is built from.
//!
//! plan.md §R1.1: *"`Recipe`, `Name`, `Package`, `FieldSpec`, `IndexSpec`,
//! `CapabilityId` and `ProjectPath` are types, not string aliases … Their
//! constructors are the only place that accepts strings, and every wire
//! decoder calls the same constructors."*
//!
//! That last clause is the load-bearing one. A decoder with its own idea of a
//! valid path is a second validator, and two validators drift — which is how a
//! value rejected at the CLI arrives through a recovered journal instead.
//! There is one constructor per type and the codec calls it.

pub mod bootstrap;
pub mod change;
pub mod conflict;
pub mod context;
pub mod declaration;
pub mod edit;
pub mod effect;
pub mod entity;
pub mod envelope;
pub mod fact;
pub mod identity;
pub mod ownership;
pub mod pending;
pub mod plan;
pub mod provenance;
pub mod recipe;
pub mod render;
pub mod request;
pub mod resource;
pub mod snapshot;
pub mod transition;

pub(crate) use jails_support::Result;
