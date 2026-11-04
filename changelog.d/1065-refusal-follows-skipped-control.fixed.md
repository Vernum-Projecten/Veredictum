- A refusal row whose every control was skipped is now skipped with the
  control's reason and citation, naming the control in its detail, instead of
  errored. On the development profile, the two `get_template_adl2_example`
  refusals followed their control's `cited-text-absent` skip into the errored
  count (#1065).
