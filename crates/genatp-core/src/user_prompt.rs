//! Host-side callback when the agent needs a human answer mid-run.

use async_trait::async_trait;

use crate::error::{CoreError, Result};

/// Prompt the operator for input (CLI stdin, SDK callback, MCP elicitation, …).
#[async_trait]
pub trait UserPrompt: Send + Sync {
    /// Ask `question`, optionally with multiple-choice `options`.
    ///
    /// # Errors
    /// Returns [`CoreError::Config`] if the host cannot collect an answer
    /// (e.g. non-interactive TTY with no callback registered).
    async fn ask(&self, question: &str, options: Option<&[String]>) -> Result<String>;
}

/// Stdin-backed prompt for CLI / local interactive runs.
#[derive(Debug, Default, Clone, Copy)]
pub struct StdinUserPrompt;

#[async_trait]
impl UserPrompt for StdinUserPrompt {
    async fn ask(&self, question: &str, options: Option<&[String]>) -> Result<String> {
        use std::io::{Write, stdin, stdout};

        // Blocking stdin on a worker thread so we do not stall the runtime.
        let question = question.to_owned();
        let options = options.map(<[String]>::to_vec);
        tokio::task::spawn_blocking(move || {
            eprintln!();
            eprintln!("=== genATP needs your input ===");
            eprintln!("{question}");
            if let Some(opts) = options.as_ref() {
                for (i, o) in opts.iter().enumerate() {
                    eprintln!("  [{}] {o}", i + 1);
                }
                eprint!("Answer (text or option number): ");
            } else {
                eprint!("Answer: ");
            }
            let _ = stdout().flush();
            let mut line = String::new();
            stdin()
                .read_line(&mut line)
                .map_err(|e| CoreError::Config(format!("ask_user stdin: {e}")))?;
            let answer = line.trim().to_owned();
            if answer.is_empty() {
                return Err(CoreError::Config(
                    "ask_user: empty answer from stdin".into(),
                ));
            }
            if let Some(opts) = options.as_ref()
                && let Ok(n) = answer.parse::<usize>()
                && (1..=opts.len()).contains(&n)
            {
                return Ok(opts[n - 1].clone());
            }
            Ok(answer)
        })
        .await
        .map_err(|e| CoreError::Config(format!("ask_user join: {e}")))?
    }
}

/// Callback used by language bindings (Python/Node) — stores an answer
/// producer that may block.
pub struct CallbackUserPrompt<F>
where
    F: Fn(&str, Option<&[String]>) -> std::result::Result<String, String> + Send + Sync,
{
    pub callback: F,
}

#[async_trait]
impl<F> UserPrompt for CallbackUserPrompt<F>
where
    F: Fn(&str, Option<&[String]>) -> std::result::Result<String, String> + Send + Sync,
{
    async fn ask(&self, question: &str, options: Option<&[String]>) -> Result<String> {
        let question = question.to_owned();
        let options = options.map(<[String]>::to_vec);
        let cb = &self.callback;
        // Run sync callback off the async runtime when it may block on input().
        let opts_ref = options.as_deref();
        match cb(&question, opts_ref) {
            Ok(s) if !s.trim().is_empty() => Ok(s),
            Ok(_) => Err(CoreError::Config("ask_user: empty answer".into())),
            Err(e) => Err(CoreError::Config(format!("ask_user: {e}"))),
        }
    }
}
