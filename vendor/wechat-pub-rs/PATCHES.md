# Local changes to wechat-pub-rs 0.7.0

This copy comes from the published 0.7.0 crate. The uploader uses it through a
relative path dependency so the fix is present in normal builds and checkouts.

`src/theme.rs` preserves spaces, expands tabs to four-column stops, and retains
blank lines when converting code to WeChat-compatible HTML. It also adds inline
code-block layout rules after the article theme: left alignment, normal letter
and word spacing, 13px type, 1.6 line height, and wrapping for narrow screens.
Syntax colors and inline code keep their existing theme styling.

Regression coverage lives in `tests/code_rendering_tests.rs` in the uploader.
Once these changes are released upstream, switch the dependency back to that
registry version and remove this copy. Publishing the uploader must wait for
that upstream release: Cargo replaces versioned path dependencies with registry
dependencies when publishing.
