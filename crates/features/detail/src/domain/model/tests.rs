use super::*;
use natikube_core::kinds;
use serde_json::json;

const NOW: i64 = 1_800_000_000;

fn pod() -> Value {
    json!({
        "apiVersion": "v1", "kind": "Pod",
        "metadata": {
            "name": "api-1", "namespace": "default", "uid": "u-1",
            "creationTimestamp": "2027-01-15T07:59:00Z",
            "resourceVersion": "42",
            "labels": {"app": "api", "tier": "web"},
            "annotations": {"note": "hello"},
            "ownerReferences": [{"apiVersion": "apps/v1", "kind": "ReplicaSet", "name": "api-5d"}]
        },
        "spec": {
            "nodeName": "node-1", "serviceAccountName": "default", "restartPolicy": "Always",
            "containers": [{
                "name": "app", "image": "nginx:1.27",
                "ports": [{"containerPort": 8080, "protocol": "TCP", "name": "http"}],
                "env": [{"name": "MODE", "value": "prod"}, {"name": "TOKEN", "valueFrom": {"secretKeyRef": {"name": "s", "key": "k"}}}],
                "volumeMounts": [{"name": "data", "mountPath": "/data", "readOnly": true}],
                "livenessProbe": {"httpGet": {"path": "/healthz", "port": 8080}, "periodSeconds": 10, "timeoutSeconds": 1, "failureThreshold": 3},
                "resources": {"requests": {"cpu": "100m"}, "limits": {"memory": "128Mi"}}
            }],
            "initContainers": [{"name": "init", "image": "busybox"}]
        },
        "status": {
            "phase": "Running", "podIP": "10.0.0.5", "hostIP": "192.168.1.2", "qosClass": "Burstable",
            "conditions": [{"type": "Ready", "status": "True", "lastTransitionTime": "2027-01-15T07:59:30Z"}],
            "containerStatuses": [{
                "name": "app", "ready": true, "restartCount": 2,
                "state": {"running": {"startedAt": "2027-01-15T07:59:10Z"}}
            }],
            "initContainerStatuses": [{
                "name": "init", "ready": true, "restartCount": 0,
                "state": {"terminated": {"exitCode": 0, "reason": "Completed"}}
            }]
        }
    })
}

fn build_pod() -> DetailModel {
    DetailModel::build(&pod(), &kinds::POD, NOW)
}

#[test]
fn metadata_labels_annotations() {
    let m = build_pod();
    assert_eq!(m.name, "api-1");
    assert_eq!(m.namespace.as_deref(), Some("default"));
    assert_eq!(
        m.labels,
        vec![("app".into(), "api".into()), ("tier".into(), "web".into())]
    );
    assert_eq!(m.annotations, vec![("note".into(), "hello".into())]);
    assert_eq!(m.age(NOW), "60s");
    let uid = m.metadata.iter().find(|f| f.label == "UID").unwrap();
    assert_eq!(uid.value, "u-1");
}

#[test]
fn owner_links_resolve_builtin_kinds() {
    let m = build_pod();
    assert_eq!(m.owners.len(), 1);
    assert_eq!(m.owners[0].kind, kinds::REPLICA_SET);
    assert_eq!(m.owners[0].name, "api-5d");
}

#[test]
fn owner_links_for_unknown_kinds_are_custom() {
    let mut obj = pod();
    obj["metadata"]["ownerReferences"] =
        json!([{"apiVersion": "argoproj.io/v1alpha1", "kind": "Rollout", "name": "r"}]);
    let m = DetailModel::build(&obj, &kinds::POD, NOW);
    assert_eq!(m.owners[0].kind.group, "argoproj.io");
    assert_eq!(m.owners[0].kind.version, "v1alpha1");
    assert_eq!(m.owners[0].kind.kind, "Rollout");
    assert_eq!(m.owners[0].kind.plural, "rollouts");
}

#[test]
fn pod_status_fields_include_node_link() {
    let m = build_pod();
    let status = m.status.iter().find(|f| f.label == "Status").unwrap();
    assert_eq!(status.value, "Running");
    assert_eq!(status.health, Some(Health::Ok));
    let node = m.status.iter().find(|f| f.label == "Node").unwrap();
    assert_eq!(node.value, "node-1");
    assert_eq!(node.link.as_ref().unwrap().kind, kinds::NODE);
    assert!(
        m.status
            .iter()
            .any(|f| f.label == "Pod IP" && f.value == "10.0.0.5")
    );
}

#[test]
fn conditions_table() {
    let m = build_pod();
    assert_eq!(m.conditions.len(), 1);
    assert_eq!(m.conditions[0].kind, "Ready");
    assert_eq!(m.conditions[0].status, "True");
    assert_eq!(m.conditions[0].health, Health::Ok);
}

#[test]
fn false_ready_condition_is_an_error() {
    let mut obj = pod();
    obj["status"]["conditions"] = json!([{"type": "Ready", "status": "False", "reason": "ContainersNotReady", "message": "x"}]);
    let m = DetailModel::build(&obj, &kinds::POD, NOW);
    assert_eq!(m.conditions[0].health, Health::Error);
    assert_eq!(m.conditions[0].reason, "ContainersNotReady");
}

#[test]
fn containers_carry_image_ports_env_mounts_probes_and_state() {
    let m = build_pod();
    assert_eq!(m.containers.len(), 2);
    let init = &m.containers[0];
    assert!(init.init);
    assert_eq!(init.name, "init");
    assert_eq!(init.state.as_ref().unwrap().1, Health::Done);

    let app = &m.containers[1];
    assert_eq!(app.image, "nginx:1.27");
    assert_eq!(
        app.ports,
        vec![ContainerPort {
            label: "http 8080/TCP".into(),
            number: 8080
        }]
    );
    assert_eq!(app.env, vec!["MODE".to_string(), "TOKEN".to_string()]);
    assert_eq!(app.mounts, vec!["data -> /data (ro)".to_string()]);
    assert_eq!(app.probes.len(), 1);
    assert_eq!(app.probes[0].0, "Liveness");
    assert!(
        app.probes[0].1.contains("http-get :8080/healthz"),
        "{}",
        app.probes[0].1
    );
    assert!(app.probes[0].1.contains("period=10s"));
    assert_eq!(app.state, Some(("Running".to_string(), Health::Ok)));
    assert_eq!(app.restarts, Some(2));
    assert_eq!(
        app.resources,
        vec![
            ("Requests".into(), "cpu 100m".into()),
            ("Limits".into(), "memory 128Mi".into())
        ]
    );
}

#[test]
fn waiting_container_reports_its_reason() {
    let mut obj = pod();
    obj["status"]["containerStatuses"][0]["state"] =
        json!({"waiting": {"reason": "CrashLoopBackOff"}});
    let m = DetailModel::build(&obj, &kinds::POD, NOW);
    assert_eq!(
        m.containers[1].state,
        Some(("CrashLoopBackOff".to_string(), Health::Error))
    );
}

#[test]
fn deployment_containers_come_from_the_pod_template() {
    let obj = json!({
        "metadata": {"name": "web", "namespace": "default"},
        "spec": {
            "replicas": 3, "selector": {"matchLabels": {"app": "web"}},
            "strategy": {"type": "RollingUpdate"},
            "template": {"spec": {"containers": [{"name": "c", "image": "img"}]}}
        },
        "status": {"replicas": 3, "updatedReplicas": 3, "availableReplicas": 2, "readyReplicas": 2,
            "conditions": [{"type": "Available", "status": "True"}]}
    });
    let m = DetailModel::build(&obj, &kinds::DEPLOYMENT, NOW);
    assert_eq!(m.containers.len(), 1);
    assert_eq!(m.containers[0].image, "img");
    assert!(m.containers[0].state.is_none());
    let replicas = m.status.iter().find(|f| f.label == "Replicas").unwrap();
    assert_eq!(
        replicas.value,
        "3 desired, 3 updated, 3 total, 2 available, 1 unavailable"
    );
    let selector = m.status.iter().find(|f| f.label == "Selector").unwrap();
    assert_eq!(selector.value, "app=web");
}

#[test]
fn service_ports_are_forwardable() {
    let obj = json!({
        "metadata": {"name": "svc", "namespace": "default"},
        "spec": {"type": "ClusterIP", "clusterIP": "10.1.1.1",
            "ports": [{"name": "http", "port": 80, "targetPort": 8080, "protocol": "TCP"}]}
    });
    let m = DetailModel::build(&obj, &kinds::SERVICE, NOW);
    assert_eq!(
        m.ports,
        vec![ContainerPort {
            label: "http 80/TCP -> 8080".into(),
            number: 80
        }]
    );
    assert!(
        m.status
            .iter()
            .any(|f| f.label == "Cluster IP" && f.value == "10.1.1.1")
    );
}

#[test]
fn config_map_data_is_listed_sorted() {
    let obj = json!({"metadata": {"name": "cm"}, "data": {"b": "2", "a": "1"}});
    let m = DetailModel::build(&obj, &kinds::CONFIG_MAP, NOW);
    assert_eq!(
        m.data,
        vec![
            ("a".to_string(), "1".to_string()),
            ("b".to_string(), "2".to_string())
        ]
    );
}

#[test]
fn secret_values_stay_out_of_the_model_text_fields() {
    let obj = json!({"metadata": {"name": "s"}, "type": "Opaque", "data": {"pw": "aHVudGVyMg=="}});
    let m = DetailModel::build(&obj, &kinds::SECRET, NOW);
    assert_eq!(m.secret.len(), 1);
    assert_eq!(m.secret[0].key, "pw");
    assert!(m.data.is_empty());
    assert!(
        m.status
            .iter()
            .any(|f| f.label == "Type" && f.value == "Opaque")
    );
    assert!(!format!("{m:?}").contains("hunter2"));
}

#[test]
fn node_status_summarizes_system_info() {
    let obj = json!({
        "metadata": {"name": "n1"},
        "spec": {"taints": [{"key": "k", "value": "v", "effect": "NoSchedule"}]},
        "status": {
            "nodeInfo": {"kubeletVersion": "v1.33.1", "osImage": "Ubuntu", "architecture": "arm64"},
            "addresses": [{"type": "InternalIP", "address": "10.0.0.1"}],
            "capacity": {"cpu": "4", "memory": "8Gi"},
            "conditions": [{"type": "Ready", "status": "True"}, {"type": "MemoryPressure", "status": "False"}]
        }
    });
    let m = DetailModel::build(&obj, &kinds::NODE, NOW);
    assert!(
        m.status
            .iter()
            .any(|f| f.label == "Kubelet version" && f.value == "v1.33.1")
    );
    assert!(
        m.status
            .iter()
            .any(|f| f.label == "Taints" && f.value == "k=v:NoSchedule")
    );
    assert!(
        m.status
            .iter()
            .any(|f| f.label == "Internal IP" && f.value == "10.0.0.1")
    );
    // A pressure condition that is False is healthy.
    assert_eq!(m.conditions[1].health, Health::Ok);
}

#[test]
fn events_are_sorted_newest_first() {
    let list = json!({"items": [
        {"type": "Normal", "reason": "Pulled", "message": "ok", "lastTimestamp": "2027-01-15T07:59:10Z", "count": 1, "source": {"component": "kubelet"}},
        {"type": "Warning", "reason": "BackOff", "message": "restarting", "lastTimestamp": "2027-01-15T07:59:50Z", "count": 4, "source": {"component": "kubelet"}}
    ]});
    let events = parse_events(&list);
    assert_eq!(events[0].reason, "BackOff");
    assert_eq!(events[0].health, Health::Warning);
    assert_eq!(events[0].count, 4);
    assert_eq!(events[1].reason, "Pulled");
    assert_eq!(events[1].health, Health::Muted);
}

#[test]
fn secret_last_applied_annotation_is_masked_in_overview() {
    let obj = serde_json::json!({
        "metadata": {"name": "s", "namespace": "d", "annotations": {
            "kubectl.kubernetes.io/last-applied-configuration": "{\"data\":{\"p\":\"c2VjcmV0\"}}"
        }},
        "data": {"p": "c2VjcmV0"}
    });
    let m = DetailModel::build(&obj, &kinds::SECRET, NOW);
    assert_eq!(m.annotations.len(), 1);
    assert_eq!(m.annotations[0].1, crate::domain::secret::MASK);
}
