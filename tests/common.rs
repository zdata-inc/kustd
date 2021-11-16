use std::collections::HashMap;
use std::time::Duration;
use either::Either;
use futures::{stream::FuturesUnordered, StreamExt};
use kube::{
    Api, Client, ResourceExt,
    core::{DynamicObject, ApiResource},
    api::{
        DeleteParams,
        PostParams,
    }
};
use k8s_openapi::api::core::v1::{ConfigMap, Namespace, Pod, Secret};
use serde_yaml::Value as YamlValue;
use serde_json::json;
use tokio::time;

use test_context::AsyncTestContext;
pub use test_context::test_context;

use kustd::manager::Manager;

pub struct K8sContext {
    client: Client,
    manager: Manager,
    random: String,
    // ApiReource, namespace, name
    to_cleanup: Vec<(ApiResource, Option<String>, String)>,
}

impl K8sContext {
    pub async fn new() -> Self {
        let (manager, future) = Manager::new().await;
        tokio::task::spawn(future);

        Self {
            client: get_client().await,
            manager,
            random: Self::gen_random(6),
            to_cleanup: Vec::new(),
        }
    }

    pub fn client(&self) -> Client {
        self.client.clone()
    }

    pub async fn create_namespace(&mut self, name: &str, labels: &str) -> Namespace {
        let namespaces: Api<Namespace> = Api::all(self.client());
        namespaces.create(&PostParams::default(), &self.namespace(&name, labels)).await.unwrap()
    }

    pub fn namespace(&mut self, name: &str, labels: &str) -> Namespace {
        let mut split_labels = HashMap::new();
        for label in labels.split(",") {
            let mut split = label.splitn(2, "=");
            split_labels.insert(split.next().unwrap(), split.next().unwrap());
        }

        let ns = Namespace::from(serde_json::from_value(json!({
            "apiVersion": "v1",
            "kind": "Namespace",
            "metadata": {
                "name": self.mangle_name(name),
                "labels": split_labels,
            },
        })).unwrap());

        let api_resource = ApiResource::erase::<Namespace>(&());
        self.to_cleanup.push((api_resource, None, ns.name().to_owned()));
        ns
    }

    pub fn empty_synced_secret(&mut self, name: &str, namespace: &str, sync_selector: &str) -> Secret {
        Secret::from(serde_json::from_value(json!({
            "apiVersion": "v1",
            "kind": "Secret",
            "metadata": {
                "name": self.mangle_name(name),
                "namespace": self.mangle_name(namespace),
                "annotations": {
                    "kustd.zdatainc.com/sync": sync_selector,
                }
            }
        })).unwrap())
    }

    pub fn mangle_name(&self, suffix: &str) -> String {
        format!("kustd-test-{}-{}", self.random, suffix)
    }

    fn gen_random(len: usize) -> String {
        use rand::{thread_rng, Rng};
        use rand::distributions::Alphanumeric;
        thread_rng()
            .sample_iter(&Alphanumeric)
            .take(len)
            .map(char::from)
            .collect::<String>()
            .to_lowercase()
    }
}

#[async_trait::async_trait]
impl AsyncTestContext for K8sContext {
    async fn setup() -> Self {
        Self::new().await
    }

    async fn teardown(mut self) {
        let mut ongoing_deletions = FuturesUnordered::new();
        let client = self.client.clone();
        for (api_resource, ns, name) in self.to_cleanup.drain(..) {
            let api: Api<DynamicObject> = ns.map_or_else(
                || Api::all_with(client.clone(), &api_resource),
                |ns| Api::namespaced_with(client.clone(), &ns, &api_resource));
            match api.delete(&name, &DeleteParams::default()).await {
                Ok(Either::Left(r)) => {
                    ongoing_deletions.push(async move {
                        let name = r.name().clone();
                        while let Ok(_) = api.get(&name).await {
                            time::sleep(Duration::from_millis(100)).await;
                        }
                    });
                },
                Ok(Either::Right(_)) => { }
                Err(err) => {
                    eprintln!("Failed to cleanup K8s resource: {:?}", err);
                }
            }
        }

        // Wait for deletions to finish
        while let Some(_) = ongoing_deletions.next().await { }
    }
}

pub async fn get_client() -> Client {
    let client = Client::try_default().await.expect("Failed to create client");

    // Ensure it's a minikube cluster, safeguard against running against the wrong context
    match is_minikube(&client).await {
        Ok(true) => client,
        Ok(false) => {
            eprintln!(concat!(
                "Testing Kubernetes cluster must be minikube. This is to ensure tests aren't ",
                "accidently run against the wrong context."));
            panic!();
        }
        Err(err) => {
            eprintln!(concat!(
                "Testing Kubernetes cluster must be minikube. This is to ensure tests aren't ",
                "accidently run against the wrong context."));
            eprintln!(
                "Error while attempting to validate test Kubernetes clusters is Minikube: {:?}", err);
            panic!();
        }
    }
}

async fn is_minikube(client: &Client) -> Result<bool, String> {
    let cluster_info = get_cluster_info(&client).await?;
    let clusters = cluster_info.get("clusters")
        .ok_or("Failed to get clusters from configmap".to_owned())?;

    let cluster_seq = clusters.as_sequence().ok_or("Unable to get clusters as sequence".to_owned())?;
    for raw_cluster in cluster_seq {
        if let Some(cluster) = raw_cluster.get("cluster") {
            if let Some(server) = cluster.get("server") {
                if server.as_str() == Some("https://control-plane.minikube.internal:8443") {
                    return Ok(true);
                }
            }
        }
    }

    Ok(false)
}

async fn get_cluster_info(client: &Client) -> Result<YamlValue, String> {
    let configmaps = Api::<ConfigMap>::namespaced(client.clone(), "kube-public");
    match configmaps.get("cluster-info").await {
        Ok(cluster_info) => {
            if let Some(data) = cluster_info.data {
                if let Some(kubeconfig) = data.get("kubeconfig") {
                    if let Ok(des) = serde_yaml::from_str(&kubeconfig) {
                        Ok(des)
                    } else {
                        Err("Failed to deserialize kube-public/cluster-info configmap".to_owned())
                    }
                } else {
                    Err("Failed to get kubeconfig key from kube-public/cluster-info configmap".to_owned())
                }
            } else {
                Err("Failed to get cluster-info configmap data".to_owned())
            }
        }
        Err(kube::Error::Api(kube::core::ErrorResponse { code: 404, .. })) => {
            Err("Unable to find kube_public/cluster_info configmap".to_owned())
        }
        Err(err) => {
            Err(format!("Failed to connect to a Kubernetes testing cluster: {:?}", err))
        }
    }
}
