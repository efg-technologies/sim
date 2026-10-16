# CI and retries

Keep the full Linux and macOS gates on pull requests and pushes to `main`.
Both platforms run si's and sim's checks and terminal E2E suites; platform
failures need investigation even when the other platform passes. The existing
per-ref concurrency cancels superseded runs when a newer commit arrives.

Agents and operators should inspect the failed logs first:

```sh
gh run view RUN_ID --log-failed --repo efg-technologies/sim
```

For a transient failure on the same commit, default to failed jobs only:

```sh
scripts/rerun-failed.sh RUN_ID
```

To retry one failed job, use `gh run rerun --job JOB_ID --repo efg-technologies/sim`.
The wrapper accepts exactly one numeric run ID and has no
fallback that reruns successful jobs. Rerun successful jobs only when their
repeat execution is needed to investigate a flake or infrastructure problem.

After changing code, push a new commit and wait for all of its required checks.
Do not reuse a previous SHA's success to validate new code or bypass a failing
platform.
