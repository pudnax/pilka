#![warn(unsafe_op_in_unsafe_fn)]
#![allow(
    // We use loops for getting early-out of scope without closures.
    clippy::never_loop,
    // We don't use syntax sugar where it's not necessary.
    clippy::match_like_matches_macro,
    // Redundant matching is more explicit.
    clippy::redundant_pattern_matching,
    // Explicit lifetimes are often easier to reason about.
    clippy::needless_lifetimes,
    // No need for defaults in the internal types.
    clippy::new_without_default,
    // For some reason `rustc` can warn about these in const generics even
    // though they are required.
    unused_braces,
)]
#![warn(trivial_casts, trivial_numeric_casts, unused_extern_crates)]

mod renderer;
pub use renderer::AshRender;

mod pvk;
pub use ash::*;
pub use pvk::*;
