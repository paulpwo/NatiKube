//! The seam between detail logic and the cluster: [`Transport`] executes
//! [`Operation`]s. The real implementation wraps a kube client; tests use a
//! recording mock so writes are never sent anywhere.

use futures::FutureExt as _;
use futures::future::BoxFuture;
use natikube_kube_client::ClusterConnection;
use serde_json::Value;
use thiserror::Error;

use super::requests::{Operation, send};

/// A failed API call, reduced to what the UI shows.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{message}")]
pub struct ApiError {
    pub code: Option<u16>,
    pub message: String,
}

impl ApiError {
    pub fn new(code: u16, message: impl Into<String>) -> Self {
        Self {
            code: Some(code),
            message: message.into(),
        }
    }

    pub fn other(message: impl Into<String>) -> Self {
        Self {
            code: None,
            message: message.into(),
        }
    }
}

impl From<kube::Error> for ApiError {
    fn from(err: kube::Error) -> Self {
        match err {
            kube::Error::Api(status) => Self::new(status.code, status.message),
            other => Self::other(other.to_string()),
        }
    }
}

pub trait Transport: Send + Sync + 'static {
    fn execute(&self, op: Operation) -> BoxFuture<'static, Result<Value, ApiError>>;
}

/// Runs operations on a real cluster connection.
pub struct KubeTransport {
    client: kube::Client,
}

impl KubeTransport {
    pub fn new(connection: &ClusterConnection) -> Self {
        Self {
            client: connection.client().clone(),
        }
    }
}

impl Transport for KubeTransport {
    fn execute(&self, op: Operation) -> BoxFuture<'static, Result<Value, ApiError>> {
        let client = self.client.clone();
        async move { send(&client, &op).await.map_err(ApiError::from) }.boxed()
    }
}
