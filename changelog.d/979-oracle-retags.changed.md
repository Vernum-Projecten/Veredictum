- The released oracle under `specs/openehr/` now reads openEHR's corrected
  re-tags of the same releases: RM `Release-1.1.0v2`, BASE `Release-1.2.0v2`
  and LANG `Release-1.0.0v1`. The RM correction gives COMPOSITION.category's
  `episodic` code as `451` (it read `435`), types `other_reference_ranges` of
  DV_COUNT, DV_PROPORTION and DV_QUANTITY as a `List` of `REFERENCE_RANGE`,
  and removes the `tags` attribute that had been added to the EHR class by
  mistake. No catalogue citation or verdict changes.
