use either::Either;
use futures::{stream::FuturesUnordered, StreamExt};
use k8s_openapi::api::core::v1::{ConfigMap, Namespace, Secret};
use kube::{
    api::{DeleteParams, PostParams},
    core::{ApiResource, DynamicObject},
    Api, Client, ResourceExt,
};
use rand::distr::{Alphanumeric, SampleString};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Value as JsonValue};
use std::collections::HashMap;
use std::fmt::Debug;
use std::marker::PhantomData;
use std::time::Duration;
use tokio::time;
use tracing::info;

pub use test_context::test_context;
use test_context::AsyncTestContext;

use kustd::{syncable::Syncable, Manager};

pub struct K8sContext {
    client: Client,
    _manager: Manager,
    random: String,
    // ApiReource, namespace, name
    to_cleanup: Vec<(ApiResource, Option<String>, String)>,
}

impl K8sContext {
    pub async fn new() -> Self {
        let (_manager, future) = Manager::new().await;
        tokio::task::spawn(future);

        Self {
            client: get_client().await,
            _manager,
            random: Self::gen_random(6),
            to_cleanup: Vec::new(),
        }
    }

    pub fn client(&self) -> Client {
        self.client.clone()
    }

    pub async fn create_namespace(&mut self, name: &str, labels: &str) -> Namespace {
        let namespaces: Api<Namespace> = Api::all(self.client());
        namespaces.create(&PostParams::default(), &self.namespace(name, labels)).await.unwrap()
    }

    pub fn namespace(&mut self, name: &str, labels: &str) -> Namespace {
        let mut split_labels = HashMap::new();
        for label in labels.split(",") {
            let mut split = label.splitn(2, "=");
            split_labels.insert(split.next().unwrap(), split.next().unwrap());
        }

        let ns: Namespace = serde_json::from_value(json!({
            "apiVersion": "v1",
            "kind": "Namespace",
            "metadata": {
                "name": self.mangle_name(name),
                "labels": split_labels,
            },
        }))
        .unwrap();

        let api_resource = ApiResource::erase::<Namespace>(&());
        self.to_cleanup.push((api_resource, None, ns.name_any().to_owned()));
        ns
    }

    pub fn secret(&self, name: &str) -> ResourceBuilder<Secret> {
        ResourceBuilder::new().name(name)
    }

    pub fn configmap(&self, name: &str) -> ResourceBuilder<ConfigMap> {
        ResourceBuilder::new().name(name)
    }

    pub fn mangle_name(&self, suffix: &str) -> String {
        format!("kustd-test-{}-{}", self.random, suffix)
    }

    fn gen_random(len: usize) -> String {
        Alphanumeric.sample_string(&mut rand::rng(), len).to_lowercase()
    }
}

impl AsyncTestContext for K8sContext {
    async fn setup() -> Self {
        Self::new().await
    }

    async fn teardown(mut self) {
        let client = self.client.clone();
        let mut ongoing_deletions = FuturesUnordered::new();
        for (api_resource, ns, name) in self.to_cleanup.drain(..) {
            let api: Api<DynamicObject> = if let Some(ns) = ns {
                Api::namespaced_with(client.clone(), &ns, &api_resource)
            } else {
                Api::all_with(client.clone(), &api_resource)
            };
            match api.delete(&name, &DeleteParams::default()).await {
                Ok(Either::Left(r)) => {
                    ongoing_deletions.push(async move {
                        let name = r.name_any().clone();
                        while api.get(&name).await.is_ok() {
                            time::sleep(Duration::from_millis(100)).await;
                        }
                        info!("{} deleted", r.name_any());
                    });
                }
                Ok(Either::Right(_)) => {}
                Err(err) => {
                    eprintln!("Failed to cleanup K8s resource: {:?}", err);
                }
            }
        }

        // Wait for deletions to finish
        while ongoing_deletions.next().await.is_some() {}
    }
}

#[derive(Default)]
pub struct ResourceBuilder<T> {
    json: JsonValue,
    _phantom: PhantomData<T>,
}

impl<T> ResourceBuilder<T>
where
    T: Syncable + Serialize + DeserializeOwned + Clone + Debug,
{
    pub fn new() -> Self {
        ResourceBuilder {
            json: json!({
                "apiVersion": "v1",
                "kind": T::kind(&())
            }),
            _phantom: PhantomData,
        }
    }

    pub fn name(mut self, name: &str) -> Self {
        merge_json(
            &mut self.json,
            &json!({
                "metadata": {
                    "name": name.to_owned(),
                }
            }),
        );
        self
    }

    pub fn type_(mut self, type_: &str) -> Self {
        merge_json(
            &mut self.json,
            &json!({
                "type": type_
            }),
        );
        self
    }

    pub fn sync_selector(mut self, sync_selector: &str) -> Self {
        merge_json(
            &mut self.json,
            &json!({
                "metadata": {
                    "annotations": {
                        "kustd.zdatainc.com/sync": sync_selector
                    }
                }
            }),
        );
        self
    }

    pub fn patch(mut self, patch: &JsonValue) -> Self {
        merge_json(&mut self.json, patch);
        self
    }

    pub fn make(self) -> T {
        serde_json::from_value(self.json).unwrap()
    }

    pub async fn create(self, api: &Api<T>) -> Result<T, kube::Error> {
        api.create(&PostParams::default(), &self.make()).await
    }
}

impl ResourceBuilder<Secret> {
    pub fn data(mut self, data: &JsonValue) -> Self {
        merge_json(&mut self.json, &json!({ "stringData": data }));
        self
    }
    pub fn data_b(mut self, data: &JsonValue) -> Self {
        merge_json(&mut self.json, &json!({ "data": data }));
        self
    }
}

impl ResourceBuilder<ConfigMap> {
    pub fn data(mut self, data: &JsonValue) -> Self {
        merge_json(&mut self.json, &json!({ "data": data }));
        self
    }
}

pub async fn get_client() -> Client {
    Client::try_default().await.expect("Failed to create client")
}

fn merge_json(a: &mut JsonValue, b: &JsonValue) {
    match (a, b) {
        (&mut JsonValue::Object(ref mut a), JsonValue::Object(b)) => {
            for (k, v) in b {
                merge_json(a.entry(k.clone()).or_insert(JsonValue::Null), v);
            }
        }
        (a, b) => {
            *a = b.clone();
        }
    }
}
