//! Stable, machine-readable identity for compiled browser products.
//!
//! Release and packaging gates consume this output, so keep feature names and
//! ordering stable unless those consumers are migrated in the same change.

const COMPILED_FEATURES: [(&str, bool); 11] = [
    ("desktop-product", cfg!(feature = "desktop-product")),
    ("desktop-dev", cfg!(feature = "desktop-dev")),
    ("desktop-test", cfg!(feature = "desktop-test")),
    ("mock-runtime", cfg!(feature = "mock-runtime")),
    ("desktop-ui", cfg!(feature = "desktop-ui")),
    ("tui", cfg!(feature = "tui")),
    (
        "chat-client-reticulum",
        cfg!(feature = "chat-client-reticulum"),
    ),
    ("chat-client-rns", cfg!(feature = "chat-client-rns")),
    (
        "chat-client-rns-clean",
        cfg!(feature = "chat-client-rns-clean"),
    ),
    ("native-reticulum", cfg!(feature = "native-reticulum")),
    ("native-network", cfg!(feature = "native-network")),
];

/// Return the stable comma-separated feature identity used by release gates.
pub fn compiled_feature_summary() -> String {
    render_feature_summary(&COMPILED_FEATURES)
}

/// Return the canonical profile name represented by the compiled feature set.
pub fn product_profile() -> &'static str {
    if cfg!(feature = "desktop-test") {
        "desktop-test"
    } else if cfg!(feature = "desktop-dev") {
        "desktop-dev"
    } else if cfg!(feature = "desktop-product") {
        "desktop-product"
    } else {
        "custom"
    }
}

/// Return the complete single-line identity printed by `--version`.
pub fn version_line() -> String {
    format!(
        "OMENbrowser_rs {} git_commit={} target={} profile={} features={}",
        env!("CARGO_PKG_VERSION"),
        env!("OMENBROWSER_BUILD_GIT_COMMIT"),
        env!("OMENBROWSER_BUILD_TARGET"),
        product_profile(),
        compiled_feature_summary()
    )
}

fn render_feature_summary(features: &[(&str, bool)]) -> String {
    features
        .iter()
        .map(|(name, enabled)| format!("{name}:{}", if *enabled { "on" } else { "off" }))
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feature_summary_has_stable_order_and_tokens() {
        assert_eq!(
            render_feature_summary(&[("product", true), ("mock", false), ("terminal", true)]),
            "product:on,mock:off,terminal:on"
        );
    }

    #[test]
    fn compiled_summary_contains_only_the_declared_feature_identity() {
        let summary = compiled_feature_summary();
        let names = summary
            .split(',')
            .map(|entry| entry.split_once(':').expect("feature state").0)
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            COMPILED_FEATURES
                .iter()
                .map(|(name, _)| *name)
                .collect::<Vec<_>>()
        );
        assert!(!summary.contains("native-rns-net"));
        assert!(!summary.contains("chat-client-rns-legacy"));
    }

    #[test]
    fn version_line_preserves_the_release_script_contract() {
        let line = version_line();
        let summary = compiled_feature_summary();
        assert!(line.starts_with(concat!(
            "OMENbrowser_rs ",
            env!("CARGO_PKG_VERSION"),
            " git_commit="
        )));
        assert_eq!(line.lines().count(), 1);
        assert!(line.contains(&format!(" target={}", env!("OMENBROWSER_BUILD_TARGET"))));
        assert!(line.contains(&format!(" profile={}", product_profile())));
        assert!(line.ends_with(&format!(" features={summary}")));
        let commit = env!("OMENBROWSER_BUILD_GIT_COMMIT");
        assert!(
            commit == "unknown"
                || (commit.len() >= 7
                    && commit.len() <= 64
                    && commit.bytes().all(|byte| byte.is_ascii_hexdigit()))
        );
    }

    #[test]
    fn compiled_profile_matches_feature_precedence() {
        let expected = if cfg!(feature = "desktop-test") {
            "desktop-test"
        } else if cfg!(feature = "desktop-dev") {
            "desktop-dev"
        } else if cfg!(feature = "desktop-product") {
            "desktop-product"
        } else {
            "custom"
        };
        assert_eq!(product_profile(), expected);
    }
}
