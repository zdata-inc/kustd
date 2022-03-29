Kustd
=====

![Test & Build Workflow Badge](https://github.com/zdata-inc/kustd/actions/workflows/docker-publish.yml/badge.svg)

Kustd is a simple Kubernetes operator which synchronizes secrets between
namespaces.

Installation
---

```bash
helm repo add kustd https://zdata-inc.github.io/kustd
helm repo update
helm upgrade --install --namespace kube-system kustd kustd/kustd
```

Run tests
---

```bash
kind create cluster
cargo test
kind delete cluster
```

### End to end tests
```bash
docker build -t kustd:latest .
kind create cluster
kind load docker-image kustd:latest
helm upgrade --install -n kube-system kustd ./charts/kustd \
  --set image.repository=kustd,image.tag=latest
kubectl kuttl test tests/e2e
kind delete cluster
```

Usage
---

```bash
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

