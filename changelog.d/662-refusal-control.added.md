- A refusal workflow, one that expects `400` or `422`, names its control in
  `x-veredictum-control`: the workflow whose request is the same apart from
  the one defect. A refusal row whose control was not accepted in the same run
  is errored with a reason naming the control, so a server that refuses every
  request for an unrelated reason no longer passes the refusals. `validate`
  refuses a refusal workflow without a control and a control that names no
  workflow, the workflow itself or another refusal workflow.
- The catalogue adds `content/dv_coded_text-validate_open-accepted`, the valid
  twin of the open DV_CODED_TEXT refusals, and moves `get-fetch_with_top` and
  `post-top_with_limit` into `query/aql_refused_top`, which runs only on the
  `aql-top-supported` arm, so a service that declines TOP is not judged on
  them.
- A refusal with no valid twin in the catalogue yet names the issue that
  tracks one, `x-veredictum-control: { pending: <issue> }`, and is never
  judged: every row of it is errored, naming the issue. The ADL2 upload
  refusals name #835 and the DV_CODED_TEXT external-terminology refusals #836.
