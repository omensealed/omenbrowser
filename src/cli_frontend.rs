//! Typed recognition for frontend, help, and version CLI tokens.
//!
//! Complex native-network commands remain in the compatibility binary until
//! they can move with their validation and report contracts intact.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Frontend {
    Desktop,
    Tui,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimpleCommand {
    Frontend(Frontend),
    Help,
    Version,
}

/// Classify one command token without consuming any following option values.
pub fn classify_argument(argument: &str) -> Option<SimpleCommand> {
    match argument {
        "-h" | "--help" => Some(SimpleCommand::Help),
        "-V" | "--version" | "version" => Some(SimpleCommand::Version),
        "--desktop" | "--iced" => Some(SimpleCommand::Frontend(Frontend::Desktop)),
        "--tui" | "--terminal" => Some(SimpleCommand::Frontend(Frontend::Tui)),
        _ => None,
    }
}

/// Select the same no-argument frontend as the compatibility binary.
pub const fn default_frontend() -> Frontend {
    if cfg!(feature = "desktop-ui") {
        Frontend::Desktop
    } else {
        Frontend::Tui
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifier_preserves_all_simple_command_aliases() {
        for argument in ["-h", "--help"] {
            assert_eq!(classify_argument(argument), Some(SimpleCommand::Help));
        }
        for argument in ["-V", "--version", "version"] {
            assert_eq!(classify_argument(argument), Some(SimpleCommand::Version));
        }
        for argument in ["--desktop", "--iced"] {
            assert_eq!(
                classify_argument(argument),
                Some(SimpleCommand::Frontend(Frontend::Desktop))
            );
        }
        for argument in ["--tui", "--terminal"] {
            assert_eq!(
                classify_argument(argument),
                Some(SimpleCommand::Frontend(Frontend::Tui))
            );
        }
    }

    #[test]
    fn classifier_does_not_claim_complex_or_unknown_arguments() {
        for argument in [
            "help",
            "desktop",
            "tui",
            "--app-root",
            "--native-smoke",
            "--passphrase-file",
        ] {
            assert_eq!(classify_argument(argument), None, "claimed {argument}");
        }
    }

    #[test]
    fn default_frontend_matches_the_compiled_ui_profile() {
        assert_eq!(
            default_frontend(),
            if cfg!(feature = "desktop-ui") {
                Frontend::Desktop
            } else {
                Frontend::Tui
            }
        );
    }
}
