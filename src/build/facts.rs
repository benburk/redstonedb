//! The few facts every build has, each one of a few values.

named_values! {
    /// The way a build's bits run.
    Orientation {
        Horizontal = "horz", "Horizontal";
        Vertical = "vert", "Vertical";
        Diagonal = "diag", "Diagonal";
    }
}

/// A build's bits run across unless it says otherwise.
impl Default for Orientation {
    fn default() -> Self {
        Orientation::Horizontal
    }
}

named_values! {
    /// How a build's numbers are written on its lines.
    Encoding {
        /// A bit per line, on levers.
        Binary = "bin", "Binary";
        /// A digit per line, as a signal strength.
        Hex = "hex", "Hex";
    }
}

named_values! {
    /// Whether a build remembers anything.
    Logic {
        /// Once its inputs stop changing, its outputs depend only on them.
        Stateless = "stateless", "Stateless";
        /// It remembers.
        Stateful = "stateful", "Stateful";
    }
}
