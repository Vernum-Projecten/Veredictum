- `veredictum test` runs every workflow whose rows file says
  `server: exclusive` in a final phase, after every other workflow of the run
  (#794). The bulk delete of every EHR used to run in path order, so it could
  take an EHR away from a workflow that ran after it and turn that row red for
  a reason that was the instrument's. Within each phase the documents keep
  path order, `results.json` lists the rows in the order they ran, and a
  re-judgement replays the recording in the same order.
