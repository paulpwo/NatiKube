use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use futures::FutureExt as _;
use futures::executor::block_on;
use futures::future::BoxFuture;
use natikube_core::{ClusterId, ResourceKind, ResourceRef, kinds};
use serde_json::{Value, json};

use super::*;
use crate::adapters::requests::Operation;
use crate::adapters::transport::{ApiError, Transport};
use crate::domain::secret::MASK;

#[derive(Default)]
struct Mock {
    log: Mutex<Vec<Operation>>,
    replies: Mutex<VecDeque<Result<Value, ApiError>>>,
}

impl Mock {
    fn with(replies: Vec<Result<Value, ApiError>>) -> Arc<Self> {
        Arc::new(Self {
            log: Mutex::default(),
            replies: Mutex::new(replies.into()),
        })
    }
    fn ops(&self) -> Vec<Operation> {
        self.log.lock().unwrap().clone()
    }
}

impl Transport for Mock {
    fn execute(&self, op: Operation) -> BoxFuture<'static, Result<Value, ApiError>> {
        self.log.lock().unwrap().push(op);
        let reply = self
            .replies
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(Ok(Value::Null));
        async move { reply }.boxed()
    }
}

fn target(kind: ResourceKind) -> ResourceRef {
    ResourceRef::new(ClusterId::new("c"), kind, Some("default".into()), "web")
}

fn body() -> Value {
    json!({"apiVersion": "apps/v1", "kind": "Deployment", "metadata": {"name": "web"}})
}

#[test]
fn apply_validates_with_dry_run_then_applies() {
    let mock = Mock::with(vec![Ok(json!({"dry": true})), Ok(json!({"real": true}))]);
    let t = target(kinds::DEPLOYMENT);
    let out = block_on(apply_object(mock.as_ref(), &t, body())).unwrap();
    assert_eq!(out, json!({"real": true}));
    assert_eq!(
        mock.ops(),
        vec![
            Operation::Apply {
                target: t.clone(),
                body: body(),
                dry_run: true
            },
            Operation::Apply {
                target: t,
                body: body(),
                dry_run: false
            },
        ]
    );
}

#[test]
fn validation_failure_never_reaches_the_real_apply() {
    let mock = Mock::with(vec![Err(ApiError::new(
        422,
        "spec.replicas: Invalid value: -1",
    ))]);
    let err = block_on(apply_object(
        mock.as_ref(),
        &target(kinds::DEPLOYMENT),
        body(),
    ))
    .unwrap_err();
    assert_eq!(
        err,
        ApplyError::Validation("spec.replicas: Invalid value: -1".into())
    );
    assert_eq!(mock.ops().len(), 1);
}

#[test]
fn real_apply_failure_is_reported_separately() {
    let mock = Mock::with(vec![Ok(Value::Null), Err(ApiError::new(409, "conflict"))]);
    let err = block_on(apply_object(
        mock.as_ref(),
        &target(kinds::DEPLOYMENT),
        body(),
    ))
    .unwrap_err();
    assert_eq!(err, ApplyError::Apply("conflict".into()));
    assert_eq!(mock.ops().len(), 2);
}

#[test]
fn masked_secret_is_refused_before_any_request() {
    let mock = Mock::with(vec![]);
    let secret = json!({"kind": "Secret", "data": {"k": MASK}});
    let err = block_on(apply_object(mock.as_ref(), &target(kinds::SECRET), secret)).unwrap_err();
    assert_eq!(err, ApplyError::MaskedSecret);
    assert!(mock.ops().is_empty());
}

#[test]
fn scale_sends_a_replicas_merge_patch() {
    let mock = Mock::with(vec![Ok(Value::Null)]);
    let t = target(kinds::DEPLOYMENT);
    block_on(scale(mock.as_ref(), &t, 5)).unwrap();
    assert_eq!(
        mock.ops(),
        vec![Operation::MergePatch {
            target: t,
            body: json!({"spec": {"replicas": 5}})
        }]
    );
}

#[test]
fn scale_is_refused_for_unsupported_kinds() {
    let mock = Mock::with(vec![]);
    assert!(block_on(scale(mock.as_ref(), &target(kinds::POD), 1)).is_err());
    assert!(mock.ops().is_empty());
}

#[test]
fn restart_patches_the_template_annotation() {
    let mock = Mock::with(vec![Ok(Value::Null)]);
    let t = target(kinds::DAEMON_SET);
    block_on(rollout_restart(mock.as_ref(), &t, "2026-10-23T10:00:00Z")).unwrap();
    let ops = mock.ops();
    let Operation::MergePatch { body, .. } = &ops[0] else {
        panic!("expected a merge patch");
    };
    assert_eq!(
        body["spec"]["template"]["metadata"]["annotations"]["kubectl.kubernetes.io/restartedAt"],
        "2026-10-23T10:00:00Z"
    );
}

#[test]
fn restart_is_refused_for_unsupported_kinds() {
    let mock = Mock::with(vec![]);
    assert!(block_on(rollout_restart(mock.as_ref(), &target(kinds::POD), "t")).is_err());
    assert!(mock.ops().is_empty());
}

#[test]
fn delete_sends_one_delete() {
    let mock = Mock::with(vec![Ok(Value::Null)]);
    let t = target(kinds::POD);
    block_on(delete(mock.as_ref(), &t)).unwrap();
    assert_eq!(mock.ops(), vec![Operation::Delete { target: t }]);
}
