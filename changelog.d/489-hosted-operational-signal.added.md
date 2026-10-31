- The console serves its operator's counters at `/metrics`, in the Prometheus
  text format, to a request presenting the bearer token
  `VEREDICTUM_METRICS_TOKEN` sets: runs accepted, started and ended by
  outcome, a run-duration histogram, queue depth beside the concurrency cap,
  refusals by the guard and reason that refused (rate ledger, target guard,
  statement validator, one run per visitor), server-function answers by status
  class, and the container's memory, CPU, process count and OOM kills from its
  cgroup. Unset or empty, the route answers 404; a value shorter than 32
  characters or outside RFC 6750's `b64token` refuses to start. Every label is
  a closed vocabulary, so no target, SUT name, credential or result appears.
- Each run writes one JSON line to standard error when its engine starts and
  when it leaves the job map: its id, outcome, queue and drive time, and case
  counts. The hosted compose file sends the console's output to the host's
  journal, so the record survives a redeploy, and the hosted watch reads the
  counters to report a `/metrics` that refuses its token or an OOM kill in the
  console container.
