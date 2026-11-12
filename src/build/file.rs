//! What a build's details keep of its current file.

/// What a build's details keep of its current file: which file it is, the
/// box it takes, what reading it found, and how it is kept. The file
/// itself lives in its versions.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct File {
    /// Its hash, so a URL derived from the file can name its version, and
    /// a simulation result can be matched to it.
    pub sha256: String,
    /// The build's box less any outer layer holding only its interface.
    pub size: (i64, i64, i64),
    /// How many redstone torches it has; `None` until counted.
    pub torches: Option<i64>,
    /// The oldest Minecraft release it works in, such as `1.5`; `None`
    /// until it is read.
    pub works_from: Option<String>,
    /// The component or mechanic that sets `works_from`, such as
    /// `comparator`; `None` for a build of nothing newer than Alpha.
    pub works_from_by: Option<String>,
    /// Whether the build follows the building conventions, so the
    /// interface reader applies to it.
    pub conventional: bool,
    /// Whether it keeps where its uploader's `//paste` put it, which a new
    /// version or a reread file keeps too.
    pub keeps_paste: bool,
}
