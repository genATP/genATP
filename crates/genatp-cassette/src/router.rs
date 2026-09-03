//! Cassette router — hit → zero-token replay; miss → caller plans with LLM.

use std::collections::HashMap;
use std::sync::Arc;

use genatp_core::action::Action;
use genatp_core::dispatcher::BrowserAdapter;
use genatp_core::perception_bridge::raw_tree_from_accessibility;
use genatp_llm::LlmProvider;
use genatp_vault::Capability;
use tracing::{info, warn};
use url::Url;

use crate::error::{CassetteError, Result};
use crate::heal::{HealConfig, maybe_heal_step};
use crate::replay::{CassetteReplayer, ReplayOutcome, ReplayStepResult, ReplayStepStatus};
use crate::schema::Cassette;
use crate::signing;
use crate::storage::{cassettes_dir, latest_version_path, list_slugs, load, save};

/// How the router resolved a run.
#[derive(Debug)]
pub enum RouteHit {
    /// Deterministic replay (0 LLM tokens unless heal patched a step).
    Replay {
        slug: String,
        version: String,
        outcome: ReplayOutcome,
        /// Set when self-heal wrote a bumped cassette.
        healed_version: Option<String>,
    },
}

/// Options for [`try_route`].
pub struct RouteOptions<'a> {
    pub slug: Option<&'a str>,
    pub goal: &'a str,
    pub start_url: &'a str,
    pub skip_cassette: bool,
    pub vault_capability: Option<&'a Capability>,
    pub heal: Option<HealConfig>,
}

/// Look up a cassette and replay when possible. `Ok(None)` = miss → plan.
pub async fn try_route(
    adapter: &dyn BrowserAdapter,
    opts: RouteOptions<'_>,
) -> Result<Option<RouteHit>> {
    if opts.skip_cassette {
        return Ok(None);
    }

    let Some(mut cassette) = find_cassette(opts.slug, opts.goal, opts.start_url)? else {
        info!(goal = %opts.goal, url = %opts.start_url, "cassette router: miss");
        return Ok(None);
    };

    enforce_vault_capability(&cassette, opts.vault_capability)?;

    let slug = cassette.header.slug.clone();
    let version = cassette.header.version.clone();
    info!(%slug, %version, "cassette router: hit — replaying (0 LLM tokens)");

    let (outcome, healed_version) =
        replay_with_optional_heal(adapter, &mut cassette, opts.heal).await?;

    Ok(Some(RouteHit::Replay {
        slug,
        version,
        outcome,
        healed_version,
    }))
}

/// Enforce cassette `header.domains` against a vault capability.
pub fn enforce_vault_capability(cassette: &Cassette, cap: Option<&Capability>) -> Result<()> {
    let Some(cap) = cap else {
        return Ok(());
    };
    for domain in &cassette.header.domains {
        cap.require(domain).map_err(|e| match e {
            genatp_vault::VaultError::CapabilityDenied(d) => CassetteError::CapabilityDenied(d),
            other => CassetteError::Storage(other.to_string()),
        })?;
    }
    Ok(())
}

fn find_cassette(slug: Option<&str>, goal: &str, start_url: &str) -> Result<Option<Cassette>> {
    let root = cassettes_dir()?;
    let verifying =
        signing::load_verifying_key().map_err(|e| CassetteError::SigningKey(e.to_string()))?;

    if let Some(slug) = slug {
        let path = latest_version_path(&root, slug)?;
        return Ok(Some(load(&path, &verifying)?));
    }

    let want_host = host_of(start_url);
    let goal_n = normalize_goal(goal);
    let mut best: Option<(u32, Cassette)> = None;

    for slug in list_slugs(&root).unwrap_or_default() {
        let path = match latest_version_path(&root, &slug) {
            Ok(p) => p,
            Err(_) => continue,
        };
        let cassette = match load(&path, &verifying) {
            Ok(c) => c,
            Err(e) => {
                warn!(%slug, error = %e, "skip unreadable cassette");
                continue;
            }
        };
        let score = match_score(&cassette, want_host.as_deref(), &goal_n);
        if score == 0 {
            continue;
        }
        if best.as_ref().is_none_or(|(s, _)| score > *s) {
            best = Some((score, cassette));
        }
    }

    Ok(best.map(|(_, c)| c))
}

fn match_score(cassette: &Cassette, want_host: Option<&str>, goal_n: &str) -> u32 {
    let cas_host = host_of(&cassette.header.start_url);
    let host_ok = matches!((want_host, cas_host.as_deref()), (Some(a), Some(b)) if a == b);
    if !host_ok {
        return 0;
    }
    let cas_goal = normalize_goal(&cassette.header.goal);
    if cas_goal == *goal_n {
        100
    } else if !goal_n.is_empty() && (cas_goal.contains(goal_n) || goal_n.contains(&cas_goal)) {
        50
    } else {
        10
    }
}

fn host_of(url: &str) -> Option<String> {
    Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_ascii_lowercase()))
}

fn normalize_goal(goal: &str) -> String {
    goal.split_whitespace()
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>()
        .join(" ")
}

async fn replay_with_optional_heal(
    adapter: &dyn BrowserAdapter,
    cassette: &mut Cassette,
    heal: Option<HealConfig>,
) -> Result<(ReplayOutcome, Option<String>)> {
    let start = std::time::Instant::now();
    let mut step_results = Vec::new();
    let mut steps_succeeded = 0u32;
    let mut steps_failed = 0u32;
    let mut healed = false;

    if let Err(e) = adapter.navigate(&cassette.header.start_url).await {
        return Err(CassetteError::ReplayFailed {
            step: 0,
            action: Action::Navigate {
                url: cassette.header.start_url.clone(),
            },
            reason: format!("initial navigation failed: {e}"),
        });
    }

    let replayer = CassetteReplayer::new(cassette.clone());
    let step_count = cassette.steps.len();
    for i in 0..step_count {
        let step = cassette.steps[i].clone();
        let map = build_ref_map(adapter).await;
        let mut result = replayer
            .execute_step(adapter, &|r| map.get(r).cloned(), &step)
            .await
            .unwrap_or_else(|e| ReplayStepResult {
                seq: step.seq,
                action: step.action.clone(),
                status: ReplayStepStatus::Failed {
                    reason: e.to_string(),
                },
                url: step.url.clone(),
            });

        if matches!(&result.status, ReplayStepStatus::Failed { .. })
            && let Some(ref heal_cfg) = heal
            && let Some(new_action) = maybe_heal_step(adapter, &result.action, heal_cfg).await?
        {
            cassette.steps[i].action = new_action.clone();
            result.action = new_action.clone();
            let map2 = build_ref_map(adapter).await;
            let healed_step = crate::schema::CassetteStep {
                action: new_action,
                ..cassette.steps[i].clone()
            };
            result = replayer
                .execute_step(adapter, &|r| map2.get(r).cloned(), &healed_step)
                .await
                .unwrap_or(result);
            if matches!(&result.status, ReplayStepStatus::Success { .. }) {
                healed = true;
            }
        }

        match &result.status {
            ReplayStepStatus::Success { .. } => steps_succeeded += 1,
            ReplayStepStatus::Failed { .. } => steps_failed += 1,
        }

        let is_finish = matches!(&result.action, Action::Finish { .. });
        step_results.push(result);
        if is_finish {
            break;
        }
    }

    let mut healed_version = None;
    if healed {
        healed_version = Some(persist_healed(cassette)?);
    }

    Ok((
        ReplayOutcome {
            steps_succeeded,
            steps_failed,
            step_results,
            duration_ms: start.elapsed().as_millis() as u64,
        },
        healed_version,
    ))
}

fn persist_healed(cassette: &mut Cassette) -> Result<String> {
    cassette.header.version = bump_minor(&cassette.header.version);
    // Recompute hash chain tip after action patches.
    let mut prev = String::new();
    for step in &mut cassette.steps {
        let payload = crate::schema::step_payload_for_hash(step);
        step.parent_hash = prev.clone();
        step.hash = crate::schema::compute_step_hash(&prev, &payload);
        prev = step.hash.clone();
    }
    cassette.header.tip_hash = prev;
    cassette.header.step_count = cassette.steps.len() as u32;

    let root = cassettes_dir()?;
    let signing_key =
        signing::load_signing_key().map_err(|e| CassetteError::SigningKey(e.to_string()))?;
    let _ = signing::ensure_keypair()?;
    save(cassette, &root, &signing_key)?;
    info!(
        slug = %cassette.header.slug,
        version = %cassette.header.version,
        "self-heal: saved bumped cassette"
    );
    Ok(cassette.header.version.clone())
}

fn bump_minor(version: &str) -> String {
    let mut parts: Vec<&str> = version.split('.').collect();
    while parts.len() < 3 {
        parts.push("0");
    }
    let minor: u32 = parts[1].parse().unwrap_or(0);
    format!("{}.{}.0", parts[0], minor + 1)
}

pub(crate) async fn build_ref_map(adapter: &dyn BrowserAdapter) -> HashMap<String, String> {
    use genatp_perception::{RefMap, StripPolicy, strip};

    let mut out = HashMap::new();
    let Ok(tree) = adapter.accessibility().await else {
        return out;
    };
    let url = adapter
        .current_url()
        .await
        .ok()
        .flatten()
        .unwrap_or_default();
    let raw = raw_tree_from_accessibility(url, &tree);
    let a11y_to_backend: HashMap<String, String> = tree
        .nodes()
        .iter()
        .filter_map(|n| {
            n.backend_dom_node_id
                .map(|id| (n.node_id.clone(), id.to_string()))
        })
        .collect();
    let mut ref_map = RefMap::new();
    let stripped = strip(&raw, &StripPolicy::default());
    let node_to_ref = ref_map.assign_for_tree(&stripped);
    for (a11y_id, r) in node_to_ref {
        if let Some(backend) = a11y_to_backend.get(&a11y_id) {
            out.insert(r, backend.clone());
        }
    }
    out
}

/// Provider handle used by heal options.
pub type HealProvider = Arc<dyn LlmProvider>;

#[cfg(test)]
mod tests {
    use super::*;
    use genatp_core::budget::BudgetSpend;
    use genatp_vault::Capability;

    fn sample_cassette(domains: Vec<String>) -> Cassette {
        Cassette {
            header: crate::schema::CassetteHeader {
                schema_version: crate::schema::SCHEMA_VERSION,
                slug: "t".into(),
                version: "1.0.0".into(),
                description: String::new(),
                goal: "g".into(),
                start_url: "https://example.com/".into(),
                model: "mock".into(),
                recorded_at: chrono::Utc::now(),
                spend: BudgetSpend::default(),
                step_count: 0,
                tip_hash: String::new(),
                signer_public_key: String::new(),
                domains,
            },
            steps: vec![],
        }
    }

    #[test]
    fn capability_allows_declared_domains() {
        let cap = Capability::for_domains(["example.com"]).unwrap();
        let c = sample_cassette(vec!["example.com".into()]);
        assert!(enforce_vault_capability(&c, Some(&cap)).is_ok());
    }

    #[test]
    fn capability_denies_undeclared_domain() {
        let cap = Capability::for_domains(["example.com"]).unwrap();
        let c = sample_cassette(vec!["evil.com".into()]);
        assert!(matches!(
            enforce_vault_capability(&c, Some(&cap)),
            Err(CassetteError::CapabilityDenied(_))
        ));
    }

    #[test]
    fn bump_minor_increments() {
        assert_eq!(bump_minor("1.0.0"), "1.1.0");
        assert_eq!(bump_minor("2.3.9"), "2.4.0");
    }
}
