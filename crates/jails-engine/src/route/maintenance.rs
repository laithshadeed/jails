//! The mutations whose subject is the project itself.
//!
//! §R6.2 groups `app init`, `rename`, `adopt layout`, `adopt legacy` and
//! `format` under one rule: *"plan one typed maintenance subject; never invent
//! a desired entity to carry it."* That rule is the whole module. None of
//! these produces something jails then owns and reconciles -- seeding a
//! manifest hands the file to the reader, a rename moves what is already
//! there, adoption records what was found, and formatting rewrites bytes
//! without changing what any of them mean. Giving any of them an entity would
//! put a row in the store that the next reconciliation would have to decide
//! about, and there is nothing to decide.

//! **"Maintenance" is a filing category, not a secret**, so this is a module
//! root and one file per command -- `pending.md` §8.1. What the four share is
//! the rule above, which is a rule about *not* creating something, and a rule
//! nobody can violate by accident does not need them in one file to hold.

mod adopt;
mod app_init;
mod format;
mod rename;

pub use adopt::adopt_layout;
pub use app_init::app_init;
pub use format::format;
pub use rename::{RenameResourceInvocation, rename, rename_resource, rename_storage};

use super::*;
