//! Confirmation for destructive commands.

use std::io::{BufRead, IsTerminal, Write};

use crate::error::{CliError, CliResult};

/// Shared `--yes` flag.
#[derive(clap::Args, Debug)]
pub struct ConfirmArgs {
    /// Do not ask for confirmation. Required when there is no terminal.
    #[arg(long, short = 'y')]
    pub yes: bool,
}

/// True when both stdin and stderr are terminals, so a question can be asked.
pub fn is_interactive() -> bool {
    std::io::stdin().is_terminal() && std::io::stderr().is_terminal()
}

/// Checks that `question` (for example "remove printer `mk4`") may go ahead.
pub fn confirm(
    question: &str,
    yes: bool,
    interactive: bool,
    input: &mut dyn BufRead,
    prompt: &mut dyn Write,
) -> CliResult<()> {
    if yes {
        return Ok(());
    }
    if !interactive {
        return Err(CliError::NeedsConfirmation(format!(
            "refusing to {question} without --yes (not running in a terminal)"
        )));
    }
    let mut chars = question.chars();
    let capitalized: String = chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default();
    write!(prompt, "{capitalized}? [y/N] ")?;
    prompt.flush()?;
    let mut line = String::new();
    input.read_line(&mut line)?;
    match line.trim().to_ascii_lowercase().as_str() {
        "y" | "yes" => Ok(()),
        _ => Err(CliError::Cancelled),
    }
}

/// [`confirm`] against the real stdin and stderr.
pub fn confirm_here(question: &str, yes: bool) -> CliResult<()> {
    confirm(
        question,
        yes,
        is_interactive(),
        &mut std::io::stdin().lock(),
        &mut std::io::stderr(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ask(yes: bool, interactive: bool, input: &str) -> (CliResult<()>, String) {
        let mut prompt = Vec::new();
        let r = confirm(
            "remove printer `mk4` from starstar",
            yes,
            interactive,
            &mut input.as_bytes(),
            &mut prompt,
        );
        (r, String::from_utf8(prompt).unwrap())
    }

    #[test]
    fn confirm_rules() {
        assert!(ask(true, false, "").0.is_ok());
        let (r, _) = ask(false, false, "y\n");
        let Err(CliError::NeedsConfirmation(m)) = r else {
            panic!("expected a refusal")
        };
        assert!(m.contains("--yes"));
        for yes in ["y\n", "YES\n"] {
            let (r, prompt) = ask(false, true, yes);
            assert!(r.is_ok());
            assert_eq!(prompt, "Remove printer `mk4` from starstar? [y/N] ");
        }
        for no in ["n\n", "\n", ""] {
            assert!(
                matches!(ask(false, true, no).0, Err(CliError::Cancelled)),
                "{no:?}"
            );
        }
    }
}
