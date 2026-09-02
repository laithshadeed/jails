//! The typed evolution policies a rename or field change is asked for.
//!
//! Each is a closed set: a strategy the compiler cannot lower refuses by
//! name rather than being ignored.

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum RenameStrategy {
    PreserveTable,
    SingleCutover,
    Rolling,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum ExternalRenamePolicy {
    Preserve,
    Rename,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum ColumnRenamePolicy {
    Preserve,
    SingleCutover,
    Rolling,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum TypeChangeStrategy {
    Safe,
    ExpandContract,
}
