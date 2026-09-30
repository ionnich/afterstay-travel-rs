# afterstay backend

AWS backend for the afterstay travel app: Rust Lambdas (`api`, `integrations`,
`chat`) shipped as container images, provisioned with Pulumi in `infra/`.
Replaces Supabase.

Start here: `./oracle.sh help` (or `make help`). The `oracle.sh` script is the
single entry point for building, pushing, deploying, testing, and smoke checks.
