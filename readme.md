Kustd
=====

Kustd is a simple Kubernetes operator which syncronizes secrets between
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

# Create a syncronized secret
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

# See it's automatically syncronized to prod-frontend and prod-backend!
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
