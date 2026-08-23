//! Whole commands, on the transaction protocol.
//!
//! Everything below this crate is a piece: a recipe that plans, a translation
//! that states the plan as desired state, a capture, a preparation, an
//! executor. This is where one request becomes one transition, and it is a
//! separate crate for a reason plan.md §R6.1 makes explicit — the routes are
//! built and tested while default dispatch is still V1, and a binary cannot
//! hold code nothing calls.

pub mod route;
