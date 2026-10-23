//! Write workflows built on a [`Transport`]: validated apply, scale, restart, delete.

use serde_json::Value;
use thiserror::Error;

use natikube_core::ResourceRef;

use super::requests::Operation;
use super::transport::{ApiError, Transport};
use crate::domain::actions::{restart_body, scale_body, supports_restart, supports_scale};
use crate::domain::secret::contains_mask;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ApplyError {
    /// The server-side dry run rejected the object; nothing was changed.
    #[error("{0}")]
    Validation(String),
    /// The dry run passed but the real apply failed.
    #[error("{0}")]
    Apply(String),
    #[error(
        "Secret values are hidden; reveal them before saving so the masked text is not written"
    )]
    MaskedSecret,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ActionError {
    #[error("{0}")]
    Unsupported(String),
    #[error("{0}")]
    Api(String),
}

/// Server-side apply with `dryRun=All` validation first, then the real apply.
pub async fn apply_object(
    transport: &dyn Transport,
    target: &ResourceRef,
    body: Value,
) -> Result<Value, ApplyError> {
    if contains_mask(&body) {
        return Err(ApplyError::MaskedSecret);
    }
    transport
        .execute(Operation::Apply {
            target: target.clone(),
            body: body.clone(),
            dry_run: true,
        })
        .await
        .map_err(|e| ApplyError::Validation(e.message))?;
    transport
        .execute(Operation::Apply {
            target: target.clone(),
            body,
            dry_run: false,
        })
        .await
        .map_err(|e: ApiError| ApplyError::Apply(e.message))
}

pub async fn scale(
    transport: &dyn Transport,
    target: &ResourceRef,
    replicas: u32,
) -> Result<(), ActionError> {
    if !supports_scale(&target.kind) {
        return Err(ActionError::Unsupported(format!(
            "{} cannot be scaled",
            target.kind.kind
        )));
    }
    patch(transport, target, scale_body(replicas)).await
}

pub async fn rollout_restart(
    transport: &dyn Transport,
    target: &ResourceRef,
    timestamp: &str,
) -> Result<(), ActionError> {
    if !supports_restart(&target.kind) {
        return Err(ActionError::Unsupported(format!(
            "{} does not support rollout restart",
            target.kind.kind
        )));
    }
    patch(transport, target, restart_body(timestamp)).await
}

pub async fn delete(transport: &dyn Transport, target: &ResourceRef) -> Result<(), ActionError> {
    transport
        .execute(Operation::Delete {
            target: target.clone(),
        })
        .await
        .map(|_| ())
        .map_err(|e| ActionError::Api(e.message))
}

async fn patch(
    transport: &dyn Transport,
    target: &ResourceRef,
    body: Value,
) -> Result<(), ActionError> {
    transport
        .execute(Operation::MergePatch {
            target: target.clone(),
            body,
        })
        .await
        .map(|_| ())
        .map_err(|e| ActionError::Api(e.message))
}

#[cfg(test)]
mod tests;
