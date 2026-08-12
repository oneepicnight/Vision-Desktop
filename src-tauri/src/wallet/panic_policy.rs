//! Process-wide production panic policy for the private wallet boundary.
//!
//! Rust invokes the panic hook before `catch_unwind`. The default hook formats payloads and may
//! emit a backtrace, so wallet initialization must never occur until this silent fixed policy is
//! installed. The outer fail-closed guards still invalidate authority or terminate the process.

use std::sync::Once;

static INSTALL_PANIC_POLICY: Once = Once::new();

pub(crate) fn install_production_panic_policy() {
    INSTALL_PANIC_POLICY.call_once(|| {
        std::panic::set_hook(Box::new(|_panic_information| {
            // Deliberately empty: never format or emit payloads, arguments, paths, native buffers,
            // wallet state, or backtraces. Fail-closed boundaries handle authority separately.
        }));
    });
}

#[cfg(test)]
mod tests {
    #[test]
    fn production_hook_source_contains_no_output_or_payload_formatting() {
        let source = include_str!("panic_policy.rs");
        let production = source.split("#[cfg(test)]").next().unwrap();
        for forbidden in [
            "eprintln!",
            "println!",
            "dbg!",
            "format!(",
            "payload()",
            "location()",
            "Backtrace",
        ] {
            assert!(
                !production.contains(forbidden),
                "forbidden panic output: {forbidden}"
            );
        }
        assert!(production.contains("std::panic::set_hook"));
    }
}
