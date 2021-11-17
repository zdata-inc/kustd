#![allow(unused_imports)]
use std::iter::FromIterator;
use std::fmt::Debug;
use std::sync::Arc;
use std::time::Duration;
use std::future::Future;
use either::Either;
use futures::{future::join, FutureExt, StreamExt};
use tracing::{debug, error, field, info, instrument, trace, warn, Span};
use serde::{de::DeserializeOwned, Serialize};
use tokio::{sync::mpsc, sync::RwLock};
use tokio_stream::wrappers::UnboundedReceiverStream;
use chrono::prelude::*;
use k8s_openapi::{Metadata, api::core::v1::{Namespace, ConfigMap, Secret}};
use kube::{
    api::{Api, ApiResource, DynamicObject, ListParams, DeleteParams, PostParams, ObjectList, ObjectMeta, Resource, ResourceExt},
    client::Client,
    runtime::{
        controller::{Context, Controller, ReconcilerAction},
        events::Reporter,
        finalizer::{finalizer, Event},
        utils::try_flatten_applied,
        watcher,
    },
};

use super::{Result, Error};

const KUSTD_ORIGIN_NAME: &str = "kustd.zdatainc.com/origin.name";
const KUSTD_ORIGIN_NAMESPACE: &str = "kustd.zdatainc.com/origin.namespace";
const KUSTD_SYNC_ANN: &str = "kustd.zdatainc.com/sync";
const KUSTD_REMOVE_ANN_ANN: &str = "kustd.zdatainc.com/remove-annotations";
const KUSTD_REMOVE_LABELS_ANN: &str = "kustd.zdatainc.com/remove-labels";

struct Data {
    client: Client,
    state: Arc<RwLock<State>>
}

#[derive(Clone, Debug, Serialize)]
pub struct State {
    #[serde(deserialize_with = "from_ts")]
    pub last_event: DateTime<Utc>,
    #[serde(skip)]
    pub reporter: Reporter,
}

impl State {
    fn new() -> Self {
        State {
            last_event: Utc::now(),
            reporter: "kustd-controller".into(),
        }
    }
}

#[instrument(skip(resource, ctx), fields(trace_id))]
async fn reconcile(resource: Secret, ctx: Context<Data>) -> Result<ReconcilerAction> {
    let client = ctx.get_ref().client.clone();
    ctx.get_ref().state.write().await.last_event = Utc::now();

    // let reporter = ctx.get_ref().state.read().await.reporter.clone();
    // let recorder = Recorder::new(client.clone(), reporter, secret.object_ref(&()));
    // let name = ResourceExt::name(&secret);
    // let namespace = ResourceExt::namespace(&secret).expect("secret must be namespaced");
    // let secrets: Api<Secret> = Api::namespaced(client.clone(), &namespace);

    let name = resource.name();
    let namespace = resource.namespace().expect("secret must be namespaced");

    if !resource.annotations().contains_key(KUSTD_SYNC_ANN) {
        debug!("Skipping resource, no sync annotation {}/{}", namespace, name);
        return Ok(ReconcilerAction { requeue_after: None });
    }

    debug!("Reconciling resource {}/{}", namespace, name);
    let api: Api<Secret> = Api::namespaced(client.clone(), &namespace);
    Ok(finalizer(&api, "kustd.zdatainc.com/cleanup", resource, |event| async {
        match event {
            Event::Apply(resource) => {
                sync_resource(&ctx, &resource).await?;
                Result::<_, Error>::Ok(ReconcilerAction { requeue_after: Some(Duration::from_secs(60 * 60)) })
            }
            Event::Cleanup(resource) => {
                sync_deleted_resource(&client, &resource).await?;
                Result::<_, Error>::Ok(ReconcilerAction { requeue_after: Some(Duration::from_secs(60 * 60)) })
            }
        }
    }).await.unwrap())
}

async fn dest_namespaces_from_ann<T>(client: Client, resource: &T) -> Result<Vec<Namespace>>
    where T: Metadata<Ty=ObjectMeta>
{
    let name = resource.name();
    let namespace = resource.namespace().expect("secret must be namespaced");
    let selector = resource.annotations().get(KUSTD_SYNC_ANN).expect("Can't accept resource without sync annotation.");

    let api: Api<Namespace> = Api::all(client);
    let mut params = ListParams::default();
    if !selector.is_empty() {
        params = params.labels(selector);
    }

    let result = api.list(&params).await.map_err(Error::KubeError)?;
    if result.items.len() == 0 {
        warn!("Given label selector {} for resource {}/{} matched no namespaces", selector, namespace, name);
    }
    Ok(Vec::<Namespace>::from_iter(result))
}

async fn sync_resource<T>(ctx: &Context<Data>, source_resource: &T) -> Result<()>
    where T: Metadata<Ty=ObjectMeta> + Serialize + DeserializeOwned + Clone + Debug
{
    let client = ctx.get_ref().client.clone();

    let name = source_resource.name();
    let namespace = source_resource.namespace().expect("secret must be namespaced");
    debug!("Synchronizing resource {}/{}", namespace, name);

    let api_resource = ApiResource::erase::<T>(&());
    let new_resource = managed_to_synced_resource(source_resource).await?;

    let dest_namespaces: Vec<_> = dest_namespaces_from_ann(client.clone(), source_resource).await?.iter().map(|ns| ns.name()).collect();

    // Find all synced copies, delete any not in dest_namespaces
    // TODO - Replace with searching controller store?
    for resource in Api::<T>::all(client.clone())
        .list(&ListParams::default()).await
        .map_err(Error::KubeError)?.iter().filter(|r| {
            r.annotations().get(KUSTD_ORIGIN_NAME) == Some(&name) &&
            r.annotations().get(KUSTD_ORIGIN_NAMESPACE) == Some(&namespace) &&
            r.namespace().map_or(false, |ns| !dest_namespaces.contains(&ns))
        }) {
        let api = Api::<T>::namespaced(client.clone(), &resource.namespace().expect("Resource must be namespaced"));
        match api.delete(&resource.name(), &DeleteParams::default()).await {
            Ok(Either::Left(_)) => {
                info!("Deleted resource {}/{}", namespace, name);
            },
            Ok(Either::Right(_)) => {
                info!("Deleting resource {}/{}", namespace, name);
            },
            Err(kube::Error::Api(kube::core::ErrorResponse { code: 404, .. })) => {
                warn!("Unable to cleanup synchronized resource {}/{}, it does not exist.", namespace, name);
            }
            Err(err) => {
                error!("Unable to cleanup synchronized resource {}, {}, {:?}", namespace, name, err);
            }
        }
    }

    // Filter out current mappings for this resource
    for dest_ns in dest_namespaces {
        let api = Api::namespaced_with(client.clone(), &dest_ns, &api_resource);
        match api.get(&name).await {
            // Update
            Ok(_) => {
                info!("Updating resource {}/{} in {}", namespace, name, dest_ns);
                api.replace(&name, &PostParams::default(), &new_resource).await.map_err(Error::KubeError)?;
            }

            // Create
            Err(kube::Error::Api(kube::core::ErrorResponse { code: 404, .. })) => {
                info!("Creating resource {}/{} in {}", namespace, name, dest_ns);
                api.create(&PostParams::default(), &new_resource).await.map_err(Error::KubeError)?;
            }

            // Unexpected kube error
            Err(err) => {
                return Err(Error::KubeError(err));
            }
        }
    }

    Ok(())
}

/// Takes a managed resource and returns a new synced resource
async fn managed_to_synced_resource<T>(source_resource: &T) -> Result<DynamicObject>
    where T: Metadata<Ty=ObjectMeta> + Serialize + DeserializeOwned + Clone + Debug
{
    let name = source_resource.name();
    let namespace = source_resource.namespace().expect("secret must be namespaced");
    let api_resource = ApiResource::erase::<T>(&());
    let mut new_resource = DynamicObject::new(&name, &api_resource);
    new_resource.meta_mut().annotations = Some(source_resource.annotations().clone());
    new_resource.meta_mut().labels = Some(source_resource.labels().clone());

    // Remove annotations
    {
        let annotations = new_resource.annotations_mut();
        annotations.remove(KUSTD_SYNC_ANN);
        annotations.insert(KUSTD_ORIGIN_NAME.to_owned(), name.clone());
        annotations.insert(KUSTD_ORIGIN_NAMESPACE.to_owned(), namespace.clone());

        // Remove annotations
        if let Some(keys) = annotations.get(KUSTD_REMOVE_ANN_ANN).cloned() {
            for key in keys.split(",") {
                annotations.remove(key.trim());
            }
        }
    }

    // Remove labels
    let remove_labels_ann = new_resource.annotations().get(KUSTD_REMOVE_LABELS_ANN).cloned();
    let labels = new_resource.labels_mut();
    if let Some(keys) = remove_labels_ann {
        for key in keys.split(",") {
            labels.remove(key.trim());
        }
    }

    Ok(new_resource)
}

async fn sync_deleted_resource<T>(client: &Client, source_resource: &T) -> Result<()>
    where T: Metadata<Ty=ObjectMeta> + Serialize + DeserializeOwned + Clone + Debug
{
    let name = source_resource.name();
    let namespace = source_resource.namespace().expect("secret must be namespaced");

    let dest_namespaces = dest_namespaces_from_ann(client.clone(), source_resource).await?;

    for ns in dest_namespaces.iter() {
        let api: Api<T> = Api::namespaced_with(client.clone(), &ns.name(), &());
        match api.delete(&name, &DeleteParams::default()).await {
            Ok(Either::Left(_)) => {
                info!("Deleted resource {}/{}", ns.name(), name);
            },
            Ok(Either::Right(_)) => {
                info!("Deleting resource {}/{}", ns.name(), name);
            },
            Err(kube::Error::Api(kube::core::ErrorResponse { code: 404, .. })) => {
                warn!("Unable to cleanup synchronized resource {}/{}, it does not exist.", ns.name(), name);
            }
            Err(err) => {
                error!("Unable to cleanup synchronized resource {}, {}, {:?}", ns.name(), name, err);
            }
        }
    }

    info!("Cleaning up removed resource {}/{}", namespace, name);
    Ok(())
}

fn error_policy(error: &Error, _ctx: Context<Data>) -> ReconcilerAction {
    warn!("reconcile failed: {:?}", error);
    ReconcilerAction {
        requeue_after: Some(Duration::from_secs(60 * 5)),
    }
}

pub struct Manager {
    state: Arc<RwLock<State>>,
}

impl Manager {
    pub async fn new() -> (Self, impl Future<Output = ((),())>) {
        let client = Client::try_default().await.expect("Failed to create client");
        let state = Arc::new(RwLock::new(State::new()));
        let context = Context::new(Data {
            client: client.clone(),
            state: state.clone(),
        });

        let secrets = Api::<Secret>::all(client.clone());
        // let configmaps = Api::<ConfigMap>::all(client.clone);

        let (ns_watcher_tx, ns_watcher_rx) = mpsc::unbounded_channel::<()>();
        let ns_watcher = async move {
            let tx = ns_watcher_tx.clone();
            watcher(Api::<Namespace>::all(client), ListParams::default()).for_each(|_| async {
                tx.send(()); // Ignore errors
            }).await;
        };

        let drainer = Controller::new(secrets, ListParams::default())
            .reconcile_all_on(UnboundedReceiverStream::new(ns_watcher_rx))
            .shutdown_on_signal()
            .run(reconcile, error_policy, context)
            .filter_map(|x| async move { std::result::Result::ok(x) })
            .for_each(|_| futures::future::ready(()))
            .boxed();

        (Self { state }, join(drainer, ns_watcher))
    }

    pub async fn state(&self) -> State {
        self.state.read().await.clone()
    }
}
