Kustd
=====

![Test & Build Workflow Badge](https://github.com/zdata-inc/kustd/actions/workflows/docker-publish.yml/badge.svg)

Kustd is a simple Kubernetes operator which synchronizes secrets between
namespaces.

Installation
------------

```
helm repo add kustd https://zdata-inc.github.io/kustd
helm repo update
helm upgrade --install --namespace kube-system kustd kustd/kustd
```

Usage
-----

```
# Create some namespaces with labels
cat <<'EOF' | kubectl apply -f -
---
apiVersion: v1
kind: Namespace
metadata:
  name: prod-frontend
  labels:
    env: prod
    tier: frontend
---
apiVersion: v1
kind: Namespace
metadata:
  name: prod-backend
  labels:
    env: prod
    tier: backend
EOF

# Create a synchronized secret
cat <<'EOF' | kubectl apply -f -
---
apiVersion: v1
kind: Secret
metadata:
  name: test
  namespace: default
  annotations:
    kustd.zdatainc.com/sync: env=prod
stringData:
  username: admin
  password: supersecret!
EOF

# See it's automatically synchronized to prod-frontend and prod-backend!
# The secret also has some extra useful annotations
kubectl -n prod-frontend get secrets test -o yaml
> metadata:
>  annotations:
>    kustd.zdatainc.com/origin.name: test
>    kustd.zdatainc.com/origin.namespace: default

# Set based label selection works as well
cat <<'EOF' | kubectl apply -f -
---
apiVersion: v1
kind: Secret
metadata:
  name: test
  namespace: default
  annotations:
    kustd.zdatainc.com/sync: "env in (prod), tier notin (frontend)"
stringData:
  username: admin
  password: supersecret!
EOF

# Now the secret is in prod-backend, but not prod-frontend
kubectl -n prod-backend get secrets test -o yaml
kubectl -n prod-frontend get secrets test -o yaml

# Cleanup
kubectl delete secret test
kubectl delete ns prod-backend prod-frontend
```

Available annotations
---------------------

`kustd.zdatainc.com/sync` - Specify namespace labels to sync resource to  
`kustd.zdatainc.com/remove-annotations` - Comma separated list of annotations to remove from synced resource  
`kustd.zdatainc.com/remove-labels` - Comma separated list of labels to remove from synced resource  


Making a release
----------------

### Releasing an app update

1. Bump version in `Cargo.toml`. Bump `app_version` and `version` in `charts/kustd/Chart.yaml`.
    The chart's `version` should be bumped in the same was as the
    `app_version`. For example, minor changes of the app should cause a minor
    change of the chart.
2. `cargo build`
3. Stage `Cargo.lock` in Git
4. Commit with `Bump version v0.0.0` message on main
5. `git push`
6. Wait for CI to complete (tags break chart-releaser, see [helm/chart-releaser/action#60][1])
7. Tag commit with version
8. `git push --tags`

[1]: https://github.com/helm/chart-releaser-action/issues/60

### Releasing just a chart update

1. Bump `version in `charts/kustd/Chart.yaml`
2. Commit with `Bump chart version v0.0.0` message on main
3. `git push`
