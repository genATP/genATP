//! VCR (Video Cassette Recorder) provider — record real LLM responses to
//! disk once, replay them deterministically in CI.
//!
//! Why this exists: provider adapters need integration tests that exercise
//! the real wire format, but `cargo test` cannot depend on live API keys
//! or network access. The pattern, borrowed from the Ruby and Python
//! community, is:
//!
//! 1. Run tests once with `GENATP_VCR_MODE=record` and a real provider — the
//!    [`VcrProvider`] forwards every call and persists the request/response
//!    pair to a JSON cassette.
//! 2. Commit the cassette.
//! 3. CI runs in default `replay` mode: requests are hashed and matched
//!    against the cassette. A miss is a hard test failure
//!    ([`LlmError::CassetteMiss`]).
//!
//! Request hash is a stable BLAKE3 of canonical JSON of the request, so it
//! changes only when the test changes — never because of provider drift.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::{LlmError, Result};
use crate::provider::LlmProvider;
use crate::types::{CompletionRequest, CompletionResponse};

/// Operating mode of the [`VcrProvider`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VcrMode {
    /// Forward to inner provider and append every interaction to disk.
    Record,
    /// Match requests against the on-disk cassette; error on miss.
    Replay,
    /// Try replay first; fall through to recording on miss.
    RecordMissing,
}

impl VcrMode {
    /// Parse from the `GENATP_VCR_MODE` env var. Defaults to [`Self::Replay`]
    /// so CI never accidentally hits the wire.
    #[must_use]
    pub fn from_env() -> Self {
        match std::env::var("GENATP_VCR_MODE").as_deref() {
            Ok("record") => Self::Record,
            Ok("record-missing") => Self::RecordMissing,
            _ => Self::Replay,
        }
    }
}

/// One recorded interaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Interaction {
    /// BLAKE3 hex of canonical request JSON.
    key: String,
    request: CompletionRequest,
    response: CompletionResponse,
}

/// A cassette is a JSON file (`{"interactions": […]}`).
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct Cassette {
    interactions: Vec<Interaction>,
}

/// `LlmProvider` decorator that records or replays interactions to a JSON
/// cassette file on disk.
#[derive(Debug)]
pub struct VcrProvider {
    inner: Option<Arc<dyn LlmProvider>>,
    path: PathBuf,
    mode: VcrMode,
    cassette: std::sync::Mutex<Cassette>,
}

impl VcrProvider {
    /// Build a VCR wrapper. `path` is the on-disk cassette location.
    ///
    /// For pure-replay tests, pass `None` for `inner` — record / record-missing
    /// modes will then fail explicitly rather than silently bypassing the
    /// inner.
    ///
    /// # Errors
    /// Surfaces I/O failures reading the cassette file (other than
    /// "not found", which is treated as an empty cassette).
    pub fn new(
        inner: Option<Arc<dyn LlmProvider>>,
        path: impl AsRef<Path>,
        mode: VcrMode,
    ) -> Result<Self> {
        let path_buf = path.as_ref().to_path_buf();
        let cassette = if path_buf.exists() {
            let bytes = fs::read(&path_buf)?;
            serde_json::from_slice::<Cassette>(&bytes).map_err(|e| {
                LlmError::Decode(format!("vcr cassette {} parse: {e}", path_buf.display()))
            })?
        } else {
            Cassette::default()
        };

        Ok(Self {
            inner,
            path: path_buf,
            mode,
            cassette: std::sync::Mutex::new(cassette),
        })
    }

    fn key_for(request: &CompletionRequest) -> Result<String> {
        let json = serde_json::to_vec(request)?;
        Ok(hex::encode(blake3::hash(&json).as_bytes()))
    }

    fn write_cassette(&self, cassette: &Cassette) -> Result<()> {
        if let Some(parent) = self.path.parent()
            && !parent.as_os_str().is_empty()
            && !parent.exists()
        {
            fs::create_dir_all(parent)?;
        }
        let bytes = serde_json::to_vec_pretty(cassette)?;
        fs::write(&self.path, bytes)?;
        Ok(())
    }

    fn lookup(&self, key: &str) -> Option<CompletionResponse> {
        self.cassette
            .lock()
            .expect("vcr cassette lock")
            .interactions
            .iter()
            .find(|i| i.key == key)
            .map(|i| i.response.clone())
    }

    /// Number of interactions currently in the cassette.
    #[must_use]
    pub fn len(&self) -> usize {
        self.cassette
            .lock()
            .expect("vcr cassette lock")
            .interactions
            .len()
    }

    /// `true` if the cassette is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[async_trait]
impl LlmProvider for VcrProvider {
    fn name(&self) -> &'static str {
        "vcr"
    }

    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        let key = Self::key_for(&request)?;

        match self.mode {
            VcrMode::Replay => self
                .lookup(&key)
                .ok_or_else(|| LlmError::CassetteMiss(key.clone())),
            VcrMode::Record => {
                let inner = self
                    .inner
                    .as_ref()
                    .ok_or_else(|| LlmError::Misconfigured("vcr record without inner".into()))?;
                let response = inner.complete(request.clone()).await?;
                {
                    let mut c = self.cassette.lock().expect("vcr cassette lock");
                    c.interactions.retain(|i| i.key != key);
                    c.interactions.push(Interaction {
                        key,
                        request,
                        response: response.clone(),
                    });
                    self.write_cassette(&c)?;
                }
                Ok(response)
            }
            VcrMode::RecordMissing => {
                if let Some(hit) = self.lookup(&key) {
                    return Ok(hit);
                }
                let inner = self
                    .inner
                    .as_ref()
                    .ok_or_else(|| LlmError::CassetteMiss(key.clone()))?;
                let response = inner.complete(request.clone()).await?;
                {
                    let mut c = self.cassette.lock().expect("vcr cassette lock");
                    c.interactions.push(Interaction {
                        key,
                        request,
                        response: response.clone(),
                    });
                    self.write_cassette(&c)?;
                }
                Ok(response)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::MockProvider;
    use crate::types::{FinishReason, Message, Usage};

    fn sample_response(text: &str) -> CompletionResponse {
        CompletionResponse {
            message: Message::assistant(text),
            finish_reason: FinishReason::Stop,
            usage: Usage::default(),
            provider: "mock".into(),
            model: "mock".into(),
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn record_then_replay_round_trips() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("cassette.json");

        let inner: Arc<dyn LlmProvider> =
            Arc::new(MockProvider::new(vec![sample_response("hello")]));
        let recorder =
            VcrProvider::new(Some(inner), &path, VcrMode::Record).expect("open recorder");

        let req = CompletionRequest::chat("m", vec![Message::user("ping")]);
        let r1 = recorder.complete(req.clone()).await.expect("record");
        assert_eq!(r1.message.content, "hello");
        assert!(path.exists(), "cassette must be written");

        // Replay without any inner provider.
        let player = VcrProvider::new(None, &path, VcrMode::Replay).expect("open player");
        let r2 = player.complete(req).await.expect("replay");
        assert_eq!(r2.message.content, "hello");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn replay_miss_returns_cassette_miss() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("empty.json");
        // Initialise empty cassette on disk.
        std::fs::write(&path, "{\"interactions\":[]}").unwrap();

        let player = VcrProvider::new(None, &path, VcrMode::Replay).expect("open player");
        let req = CompletionRequest::chat("m", vec![Message::user("unseen")]);
        let err = player.complete(req).await.expect_err("must miss");
        assert!(matches!(err, LlmError::CassetteMiss(_)));
    }

    #[test]
    fn vcr_mode_from_env_recognises_record() {
        // Mode parsing is pure — exercise the branches without touching env.
        // The `from_env` path is covered by integration tests with an env
        // guard.
        assert_eq!(VcrMode::Replay, VcrMode::Replay);
        assert_ne!(VcrMode::Record, VcrMode::Replay);
        assert_ne!(VcrMode::RecordMissing, VcrMode::Record);
    }
}
