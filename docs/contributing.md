Contributing
===

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
