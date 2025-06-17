use k8s_openapi::{
    api::core::v1::{ConfigMap, Secret},
    NamespaceResourceScope,
};
use kube::api::{Resource, ResourceExt};

/// Implemented for k8s_openapi resources which can be synced between namespaces.
pub trait Syncable
where
    Self: ResourceExt<DynamicType = (), Scope = NamespaceResourceScope>,
{
    /// Creates a new resource, only cloning critical data from the original.
    fn duplicate(&self) -> Self;
}

impl Syncable for Secret {
    fn duplicate(&self) -> Self {
        let mut new_resource = Self { type_: self.type_.clone(), ..Default::default() };

        new_resource.meta_mut().name = Some(self.name_any());
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
        new_resource.meta_mut().name = Some(self.name_any());
        new_resource.meta_mut().namespace = self.namespace();
        new_resource.meta_mut().annotations = Some(self.annotations().clone());
        new_resource.meta_mut().labels = Some(self.labels().clone());
        new_resource.data = self.data.clone();

        new_resource
    }
}
