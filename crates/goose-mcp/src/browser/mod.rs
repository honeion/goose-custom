//! Browser automation extension using chromiumoxide
//!
//! Provides tools for controlling Chrome/Edge browser via Chrome DevTools Protocol.
//! Supports both headless and headed (visible) modes.

pub mod browser_ext;

pub use browser_ext::BrowserExtension;
