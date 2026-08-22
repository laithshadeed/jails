# Load tests

The route list in `api.js` was derived from the application's Java source by
`jails add loadtest`. Start the application, install [k6](https://k6.io/), and
run `make run`. Override `BASE_URL`, `VUS`, `DURATION`, or `AUTH_TOKEN` through
the environment. Re-run `jails remove loadtest && jails add loadtest` after
changing routes, after reviewing any local edits reported by `remove`.
