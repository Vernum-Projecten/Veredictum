// SPDX-FileCopyrightText: Veredictum contributors
// SPDX-License-Identifier: Apache-2.0

//! Binding a workload document to the generator: the weighted [`Mix`] the
//! generator draws from, the stage each drawn call realizes, and the limits
//! every plateau is judged against.
//!
//! The generator knows operations by [`OperationId`] only. The binding keeps
//! the workload's own stage beside each interaction's position, so the
//! performer reads the payload and the query class of a call from the
//! interaction and stage the generator names, and two interactions that
//! send one operation with different payloads stay distinct on the wire
//! while sharing one measurement key.

use std::collections::BTreeMap;
use std::time::Duration;

use thiserror::Error;

use crate::perf_gen::{FollowUp, Interaction, Mix, MixError, OperationId};
use crate::perf_result::{DroppedInteractionRecord, RawQualification};
use crate::workload::ops::WorkloadOp;
use crate::workload::{Applied, Claimed, InteractionShape, QueryScope, Stage, Workload};

/// A workload bound to the generator.
#[derive(Debug, Clone)]
pub struct Binding {
    /// The interactions the generator draws, weighted by their share in
    /// millionths.
    pub mix: Mix,
    /// The workload stages of each mix interaction, by its position.
    pub stages: Vec<Vec<Stage>>,
    /// Operation arrivals per interaction arrival, over the kept mix.
    pub operations_per_interaction: f64,
    /// The limits every plateau is judged against.
    pub qualification: RawQualification,
    /// The interactions a statement dropped.
    pub dropped: Vec<DroppedInteractionRecord>,
}

impl Binding {
    /// The stage a call of interaction `interaction` at `stage` realizes.
    #[must_use]
    pub fn stage(&self, interaction: usize, stage: u32) -> Option<&Stage> {
        let stage = usize::try_from(stage).ok()?;
        self.stages.get(interaction)?.get(stage)
    }

    /// The operations every kept stage sends, without repetition.
    #[must_use]
    pub fn operations(&self) -> Vec<WorkloadOp> {
        let mut ops: Vec<WorkloadOp> = self.stages.iter().flatten().map(|s| s.op).collect();
        ops.sort();
        ops.dedup();
        ops
    }
}

/// A workload that cannot be bound.
#[derive(Debug, Error)]
pub enum BindError {
    /// The statement's claims drop every interaction.
    #[error("the statement's claims drop every interaction of the workload")]
    NothingKept,
    /// The generator refused the mix.
    #[error("the generator refuses the mix: {0}")]
    Mix(#[from] MixError),
}

/// Returns the operation id a stage is measured under.
///
/// # Errors
/// [`MixError::OperationId`] for a key the generator refuses, which no
/// loaded workload produces.
pub fn operation_id(stage: &Stage) -> Result<OperationId, MixError> {
    OperationId::new(stage.key().as_str())
}

/// The limits a workload judges its plateaus by: its operation budget for
/// every operation, its population budget for a population query class.
#[must_use]
pub fn qualification(workload: &Workload) -> RawQualification {
    let q = &workload.qualification;
    let mut budgets_ms = BTreeMap::new();
    for (name, class) in &workload.queries {
        if class.scope == QueryScope::Population
            && let Ok(id) = OperationId::new(&format!("aql:{name}"))
        {
            let _replaced = budgets_ms.insert(id, q.population_aql_p99_ms);
        }
    }
    RawQualification {
        default_budget_ms: q.operation_p99_ms,
        budgets_ms,
        max_error_ppm: q.max_error_share.ppm(),
        min_achieved_ppm: q.min_achieved_share.ppm(),
    }
}

/// Binds `workload`, keeping only what `claimed` claims when a statement is
/// given.
///
/// # Errors
/// [`BindError::NothingKept`] when the claims drop every interaction, and
/// [`BindError::Mix`] when the generator refuses the mix.
pub fn bind(workload: &Workload, claimed: Option<&Claimed>) -> Result<Binding, BindError> {
    let Applied { dropped, .. } = claimed.map_or_else(
        || Applied {
            kept: BTreeMap::new(),
            dropped: Vec::new(),
        },
        |c| workload.apply_claims(c),
    );
    let mut interactions = Vec::new();
    let mut stages = Vec::new();
    let mut weighted_ops: u64 = 0;
    let mut weights: u64 = 0;
    for (name, interaction) in &workload.interactions {
        if dropped.iter().any(|d| &d.interaction == name) {
            continue;
        }
        let weight = interaction.share.ppm();
        let kept_stages = interaction.stages().to_vec();
        let Some((first, rest)) = kept_stages.split_first() else {
            continue;
        };
        let think = match &interaction.shape {
            InteractionShape::Session(session) => {
                Duration::from_millis(u64::from(session.think_time_mean_ms))
            }
            InteractionShape::Single(_) => Duration::ZERO,
        };
        let follow_ups = rest
            .iter()
            .map(|stage| {
                Ok(FollowUp {
                    think_mean: think,
                    operation: operation_id(stage)?,
                })
            })
            .collect::<Result<Vec<_>, MixError>>()?;
        interactions.push(Interaction::session(
            name.as_str(),
            weight,
            operation_id(first)?,
            follow_ups,
        )?);
        let count = u64::try_from(kept_stages.len()).unwrap_or(u64::MAX);
        weighted_ops = weighted_ops.saturating_add(u64::from(weight).saturating_mul(count));
        weights = weights.saturating_add(u64::from(weight));
        stages.push(kept_stages);
    }
    if interactions.is_empty() {
        return Err(BindError::NothingKept);
    }
    let operations_per_interaction = crate::perf_result::quantile::u64_to_f64(weighted_ops)
        / crate::perf_result::quantile::u64_to_f64(weights.max(1));
    Ok(Binding {
        mix: Mix::new(interactions)?,
        stages,
        operations_per_interaction,
        qualification: qualification(workload),
        dropped: dropped
            .into_iter()
            .map(|d| DroppedInteractionRecord {
                interaction: d.interaction.to_string(),
                share: d.share.to_string(),
                missing: d.missing,
            })
            .collect(),
    })
}
