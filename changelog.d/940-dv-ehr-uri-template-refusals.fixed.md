- The DV_EHR_URI list and pattern refusals send ehr URIs of the forms the RM
  lists (`ehr:/ehr_id`, `ehr://system_id/ehr_id`), so a 422 shows the
  template constraint was enforced rather than the RM `Scheme_valid`
  invariant. The `https://` value they sent before is kept as its own row of
  the `Scheme_valid` refusal.
