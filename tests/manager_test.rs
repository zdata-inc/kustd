use base64::prelude::*;

use k8s_openapi::{
    api::core::v1::{ConfigMap, Namespace, Secret},
    ByteString, Metadata,
};
use kube::{
    api::{Api, DeleteParams, Patch, PatchParams},
    ResourceExt,
};
use serde_json::json;
use serial_test::serial;
/// To ensure exhaustive testing; each of these cases must be covered:
/// | On Event                | Synced Resource Action |
/// | ----------------------- | ---------------------- |
/// | NS created              | Created                |
/// | NS updated              | Created/Deleted        |
/// | NS deleted              | No action*             |
/// | Source resource created | Created                |
/// | Source resource updated | Created/Deleted        |
/// | Source resource deleted | Deleted                |
/// Currently every namespace update forces a refresh of every synced resource. This could be very
/// costly depending on the size of the cluster. (TODO)
use std::matches;
use std::time::Duration;
use tokio::time;

mod common;
use common::{test_context, K8sContext};

#[ctor::ctor]
fn init_log() {
    env_logger::builder().is_test(true).init();
}

#[test_context(K8sContext)]
#[tokio::test]
#[serial]
async fn test_sync_secret(ctx: &mut K8sContext) {
    let ns1 = ctx.create_namespace("test1", "loc=a").await;
    let ns2 = ctx.create_namespace("test2", "loc=b").await;

    let ns1_secrets: Api<Secret> = Api::namespaced(ctx.client(), &ns1.name_any());
    let ns2_secrets: Api<Secret> = Api::namespaced(ctx.client(), &ns2.name_any());

    // Test creating secret
    let secret = &ctx
        .secret("test")
        .sync_selector("loc=b")
        .data(&json!({ "data": "data" }))
        .create(&ns1_secrets)
        .await
        .unwrap();
    time::sleep(Duration::from_millis(500)).await;
    let synced_secret = ns2_secrets.get(&secret.name_any()).await.unwrap();
    assert_eq!(
        synced_secret.data.and_then(|x| { x.get("data").cloned() }),
        Some(ByteString("data".as_bytes().to_vec()))
    );

    // Test updating secret to delete from NS
    ns1_secrets
        .patch(
            &secret.name_any(),
            &PatchParams::apply("kustd").force(),
            &Patch::Apply(json!({
                "apiVersion": "v1",
                "kind": "Secret",
                "metadata": {
                    "annotations": {
                        "kustd.zdatainc.com/sync": "loc=c"
                    }
                }
            })),
        )
        .await
        .unwrap();
    time::sleep(Duration::from_millis(500)).await;
    assert!(matches!(
        ns2_secrets.get(&secret.name_any()).await,
        Err(kube::Error::Api(kube::core::ErrorResponse { code: 404, .. }))
    ));

    // Test updating secret to create in NS
    ns1_secrets
        .patch(
            &secret.name_any(),
            &PatchParams::apply("kustd").force(),
            &Patch::Apply(json!({
                "apiVersion": "v1",
                "kind": "Secret",
                "metadata": {
                    "annotations": {
                        "kustd.zdatainc.com/sync": "loc=b"
                    }
                }
            })),
        )
        .await
        .unwrap();
    time::sleep(Duration::from_millis(500)).await;
    assert!(ns2_secrets.get(&secret.name_any()).await.is_ok());

    // Test deleting secret
    ns1_secrets.delete(&secret.name_any(), &DeleteParams::default()).await.unwrap();
    time::sleep(Duration::from_millis(500)).await;
    assert!(matches!(
        ns2_secrets.get(&secret.name_any()).await,
        Err(kube::Error::Api(kube::core::ErrorResponse { code: 404, .. }))
    ));
}

#[test_context(K8sContext)]
#[tokio::test]
#[serial]
async fn test_sync_dockerconfig_secret(ctx: &mut K8sContext) {
    let ns1 = ctx.create_namespace("test1", "loc=a").await;
    let ns2 = ctx.create_namespace("test2", "loc=b").await;

    let dockerconfigjson = "eyJhdXRocyI6eyJ0ZXN0Ijp7InVzZXJuYW1lIjoidGVzdCIsInBhc3N3b3JkIjoidGVzdCIsImF1dGgiOiJkR1Z6ZERwMFpYTjAifX19";

    let ns1_secrets: Api<Secret> = Api::namespaced(ctx.client(), &ns1.name_any());
    let ns2_secrets: Api<Secret> = Api::namespaced(ctx.client(), &ns2.name_any());

    // Test creating secret
    let secret = &ctx
        .secret("test")
        .type_("kubernetes.io/dockerconfigjson")
        .sync_selector("loc=b")
        .data_b(&json!({ ".dockerconfigjson": dockerconfigjson }))
        .create(&ns1_secrets)
        .await
        .unwrap();
    time::sleep(Duration::from_millis(500)).await;
    let synced_secret = ns2_secrets.get(&secret.name_any()).await.unwrap();
    assert_eq!(
        synced_secret.data.and_then(|x| { x.get(".dockerconfigjson").cloned() }),
        Some(ByteString(BASE64_STANDARD.decode(dockerconfigjson.as_bytes()).unwrap()))
    );
    assert_eq!(synced_secret.type_, Some("kubernetes.io/dockerconfigjson".to_string()));

    // Test updating secret to delete from NS
    ns1_secrets
        .patch(
            &secret.name_any(),
            &PatchParams::apply("kustd").force(),
            &Patch::Apply(json!({
                "apiVersion": "v1",
                "kind": "Secret",
                "type": "kubernetes.io/dockerconfigjson",
                "metadata": {
                    "annotations": {
                        "kustd.zdatainc.com/sync": "loc=c"
                    }
                }
            })),
        )
        .await
        .unwrap();
    time::sleep(Duration::from_millis(500)).await;
    assert!(matches!(
        ns2_secrets.get(&secret.name_any()).await,
        Err(kube::Error::Api(kube::core::ErrorResponse { code: 404, .. }))
    ));

    // Test updating secret to create in NS
    ns1_secrets
        .patch(
            &secret.name_any(),
            &PatchParams::apply("kustd").force(),
            &Patch::Apply(json!({
                "apiVersion": "v1",
                "kind": "Secret",
                "type": "kubernetes.io/dockerconfigjson",
                "metadata": {
                    "annotations": {
                        "kustd.zdatainc.com/sync": "loc=b"
                    }
                },
            })),
        )
        .await
        .unwrap();
    time::sleep(Duration::from_millis(500)).await;
    assert!(ns2_secrets.get(&secret.name_any()).await.is_ok());

    // Test deleting secret
    ns1_secrets.delete(&secret.name_any(), &DeleteParams::default()).await.unwrap();
    time::sleep(Duration::from_millis(500)).await;
    assert!(matches!(
        ns2_secrets.get(&secret.name_any()).await,
        Err(kube::Error::Api(kube::core::ErrorResponse { code: 404, .. }))
    ));
}

#[test_context(K8sContext)]
#[tokio::test]
#[serial]
async fn test_sync_secret_all_ns(ctx: &mut K8sContext) {
    let ns1 = ctx.create_namespace("test1", "loc=a").await;
    let ns2 = ctx.create_namespace("test2", "loc=b").await;

    let ns1_secrets: Api<Secret> = Api::namespaced(ctx.client(), &ns1.name_any());
    let ns2_secrets: Api<Secret> = Api::namespaced(ctx.client(), &ns2.name_any());

    // Test creating secret
    let secret = &ctx
        .secret("test-sync-all")
        .sync_selector("")
        .data(&json!({ "data": "data" }))
        .create(&ns1_secrets)
        .await
        .unwrap();
    time::sleep(Duration::from_millis(500)).await;

    let orig_secret = ns1_secrets.get(&secret.name_any()).await.unwrap();
    // Ensure original secret doesn't get overwritten during sync
    assert_eq!(orig_secret.metadata().annotations, secret.metadata().annotations);

    let synced_secret = ns2_secrets.get(&secret.name_any()).await.unwrap();
    assert_eq!(
        synced_secret.data.clone().and_then(|x| { x.get("data").cloned() }),
        Some(ByteString("data".as_bytes().to_vec()))
    );

    // Test updating secret change data
    ns1_secrets
        .patch(
            &secret.name_any(),
            &PatchParams::apply("kustd").force(),
            &Patch::Apply(json!({
                "apiVersion": "v1",
                "kind": "Secret",
                "stringData": {
                    "data": "data2"
                }
            })),
        )
        .await
        .unwrap();
    time::sleep(Duration::from_millis(500)).await;
    let synced_secret = ns2_secrets.get(&secret.name_any()).await.unwrap();
    assert_eq!(
        synced_secret.data.and_then(|x| { x.get("data").cloned() }),
        Some(ByteString("data2".as_bytes().to_vec()))
    );

    // Test updating secret to delete from NS
    ns1_secrets
        .patch(
            &secret.name_any(),
            &PatchParams::apply("kustd").force(),
            &Patch::Apply(json!({
                "apiVersion": "v1",
                "kind": "Secret",
                "metadata": {
                    "annotations": {
                        "kustd.zdatainc.com/sync": "loc=c"
                    }
                }
            })),
        )
        .await
        .unwrap();
    time::sleep(Duration::from_millis(500)).await;
    assert!(matches!(
        ns2_secrets.get(&secret.name_any()).await,
        Err(kube::Error::Api(kube::core::ErrorResponse { code: 404, .. }))
    ));

    // Test updating secret to create in NS
    ns1_secrets
        .patch(
            &secret.name_any(),
            &PatchParams::apply("kustd").force(),
            &Patch::Apply(json!({
                "apiVersion": "v1",
                "kind": "Secret",
                "metadata": {
                    "annotations": {
                        "kustd.zdatainc.com/sync": ""
                    }
                }
            })),
        )
        .await
        .unwrap();
    time::sleep(Duration::from_millis(500)).await;
    assert!(ns2_secrets.get(&secret.name_any()).await.is_ok());

    // Test deleting secret
    ns1_secrets.delete(&secret.name_any(), &DeleteParams::default()).await.unwrap();
    time::sleep(Duration::from_millis(500)).await;
    assert!(matches!(
        ns2_secrets.get(&secret.name_any()).await,
        Err(kube::Error::Api(kube::core::ErrorResponse { code: 404, .. }))
    ));
}

#[test_context(K8sContext)]
#[tokio::test]
#[serial]
async fn test_sync_configmap(ctx: &mut K8sContext) {
    let ns1 = ctx.create_namespace("test1", "loc=a").await;
    let ns2 = ctx.create_namespace("test2", "loc=b").await;

    let ns1_configmaps: Api<ConfigMap> = Api::namespaced(ctx.client(), &ns1.name_any());
    let ns2_configmaps: Api<ConfigMap> = Api::namespaced(ctx.client(), &ns2.name_any());

    // Test creating secret
    let secret = &ctx
        .configmap("test")
        .sync_selector("loc=b")
        .data(&json!({ "data": "data" }))
        .create(&ns1_configmaps)
        .await
        .unwrap();
    time::sleep(Duration::from_millis(500)).await;
    let synced_secret = ns2_configmaps.get(&secret.name_any()).await.unwrap();
    assert_eq!(
        synced_secret.data.and_then(|x| { x.get("data").cloned() }),
        Some("data".to_owned())
    );

    // Test updating secret to delete from NS
    ns1_configmaps
        .patch(
            &secret.name_any(),
            &PatchParams::apply("kustd").force(),
            &Patch::Apply(json!({
                "apiVersion": "v1",
                "kind": "ConfigMap",
                "metadata": {
                    "annotations": {
                        "kustd.zdatainc.com/sync": "loc=c"
                    }
                }
            })),
        )
        .await
        .unwrap();
    time::sleep(Duration::from_millis(500)).await;
    assert!(matches!(
        ns2_configmaps.get(&secret.name_any()).await,
        Err(kube::Error::Api(kube::core::ErrorResponse { code: 404, .. }))
    ));

    // Test updating secret to create in NS
    ns1_configmaps
        .patch(
            &secret.name_any(),
            &PatchParams::apply("kustd").force(),
            &Patch::Apply(json!({
                "apiVersion": "v1",
                "kind": "ConfigMap",
                "metadata": {
                    "annotations": {
                        "kustd.zdatainc.com/sync": "loc=b"
                    }
                }
            })),
        )
        .await
        .unwrap();
    time::sleep(Duration::from_millis(500)).await;
    assert!(ns2_configmaps.get(&secret.name_any()).await.is_ok());

    // Test deleting secret
    ns1_configmaps.delete(&secret.name_any(), &DeleteParams::default()).await.unwrap();
    time::sleep(Duration::from_millis(500)).await;
    assert!(matches!(
        ns2_configmaps.get(&secret.name_any()).await,
        Err(kube::Error::Api(kube::core::ErrorResponse { code: 404, .. }))
    ));
}

#[test_context(K8sContext)]
#[tokio::test]
#[serial]
async fn test_sync_ns(ctx: &mut K8sContext) {
    let ns1 = ctx.create_namespace("test1", "loc=a").await;
    let ns2 = ctx.create_namespace("test2", "loc=b").await;

    let namespaces: Api<Namespace> = Api::all(ctx.client());

    let ns1_secrets: Api<Secret> = Api::namespaced(ctx.client(), &ns1.name_any());
    let ns2_secrets: Api<Secret> = Api::namespaced(ctx.client(), &ns2.name_any());
    let ns3_secrets: Api<Secret> = Api::namespaced(ctx.client(), &ctx.mangle_name("test3"));

    let secret = ctx
        .secret("test")
        .sync_selector("loc=b")
        .data(&json!({ "data": "data" }))
        .create(&ns1_secrets)
        .await
        .unwrap();

    time::sleep(Duration::from_millis(500)).await;
    let synced_secret = ns2_secrets.get(&secret.name_any()).await.unwrap();
    assert_eq!(
        synced_secret.data.and_then(|x| { x.get("data").cloned() }),
        Some(ByteString("data".as_bytes().to_vec()))
    );

    // Test creating NS
    let ns3 = ctx.create_namespace("test3", "loc=b").await;
    time::sleep(Duration::from_millis(250)).await;
    assert!(ns3_secrets.get(&secret.name_any()).await.is_ok());

    // Test updating NS to delete secret
    namespaces
        .patch(
            &ns3.name_any(),
            &PatchParams::apply("kustd").force(),
            &Patch::Apply(json!({
                "apiVersion": "v1",
                "kind": "Namespace",
                "metadata": {
                    "labels": {
                        "loc": "c"
                    }
                }
            })),
        )
        .await
        .unwrap();
    time::sleep(Duration::from_millis(250)).await;
    assert!(matches!(
        ns3_secrets.get(&secret.name_any()).await,
        Err(kube::Error::Api(kube::core::ErrorResponse { code: 404, .. }))
    ));

    // Test updating NS to create secret
    namespaces
        .patch(
            &ns3.name_any(),
            &PatchParams::apply("kustd").force(),
            &Patch::Apply(json!({
                "apiVersion": "v1",
                "kind": "Namespace",
                "metadata": {
                    "labels": {
                        "loc": "b"
                    }
                }
            })),
        )
        .await
        .unwrap();
    time::sleep(Duration::from_millis(500)).await;
    assert!(ns3_secrets.get(&secret.name_any()).await.is_ok());
}

#[test_context(K8sContext)]
#[tokio::test]
#[serial]
async fn test_sync_remove_ann_labels(ctx: &mut K8sContext) {
    let ns1 = ctx.create_namespace("test1", "loc=a").await;
    let ns2 = ctx.create_namespace("test2", "loc=b").await;

    let ns1_secrets: Api<Secret> = Api::namespaced(ctx.client(), &ns1.name_any());
    let ns2_secrets: Api<Secret> = Api::namespaced(ctx.client(), &ns2.name_any());

    let secret = ctx
        .secret("test")
        .sync_selector("loc=b")
        .data(&json!({ "data": "data" }))
        .patch(&json!({
            "metadata": {
                "annotations": {
                    "kustd.zdatainc.com/remove-annotations": "test1,test2",
                    "kustd.zdatainc.com/remove-labels": "test1,test2",
                    "test1": "test1",
                    "test2": "test2",
                    "test3": "test3",
                },
                "labels": {
                    "test1": "test1",
                    "test2": "test2",
                    "test3": "test3",
                }
            }
        }))
        .create(&ns1_secrets)
        .await
        .unwrap();
    time::sleep(Duration::from_millis(500)).await;
    let synced_secret = ns2_secrets.get(&secret.name_any()).await.unwrap();
    let annotations = synced_secret.annotations();
    assert_eq!(annotations.get("test1"), None);
    assert_eq!(annotations.get("test2"), None);
    assert_eq!(annotations.get("test3"), Some(&"test3".to_owned()));

    let labels = synced_secret.labels();
    assert_eq!(labels.get("test1"), None);
    assert_eq!(labels.get("test2"), None);
    assert_eq!(labels.get("test3"), Some(&"test3".to_owned()));

    assert_eq!(
        synced_secret.data.and_then(|x| { x.get("data").cloned() }),
        Some(ByteString("data".as_bytes().to_vec()))
    );
}
