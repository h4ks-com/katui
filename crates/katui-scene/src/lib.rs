//! The scene: layer 2 of the stack in ARCHITECTURE.md. Retained,
//! backend-independent state. Cell planes hold text, pixel surfaces hold
//! RGBA, presenters turn the diff of two scenes into terminal bytes.

pub mod cell;

/// A rectangular size in terminal cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellSize {
    /// Columns, the horizontal extent.
    pub columns: u16,
    /// Rows, the vertical extent.
    pub rows: u16,
}

#[cfg(test)]
mod tests {
    use super::CellSize;

    #[test]
    fn cell_sizes_compare_by_value() {
        let a = CellSize {
            columns: 80,
            rows: 24,
        };
        let b = CellSize {
            columns: 80,
            rows: 24,
        };
        assert_eq!(a, b);
        let c = CellSize {
            columns: 81,
            rows: 24,
        };
        assert_ne!(a, c);
    }
}
