use k8s_openapi::{Metadata, api::core::v1::{ConfigMap, Secret}};
use kube::api::{Resource, ResourceExt, ObjectMeta};

/// Implemented for k8s_openapi resources which can be synced between namespaces.
pub trait Syncable where Self: Metadata<Ty=ObjectMeta> {
    /// Creates a new resource, only cloning critical data from the original.
    fn duplicate(&self) -> Self;
}

impl Syncable for Secret {
    fn duplicate(&self) -> Self {
        let mut new_resource = Self::default();
        new_resource.type_ = self.type_.clone();
        new_resource.meta_mut().name = Some(self.name());
        new_resource.meta_mut().namespace = self.namespace();
        new_resource.meta_mut().annotations = Some(self.annotations().clone());
        new_resource.meta_mut().labels = Some(self.labels().clone());
        new_resource.data = self.data.clone();

        new_resource
    }
}

impl Syncable for ConfigMap {
    fn duplicate(&self) -> Self {
        let mut new_resource = Self::default();
        new_resource.meta_mut().name = Some(self.name());
        new_resource.meta_mut().namespace = self.namespace();
        new_resource.meta_mut().annotations = Some(self.annotations().clone());
        new_resource.meta_mut().labels = Some(self.labels().clone());
        new_resource.data = self.data.clone();

        new_resource
    }
}
