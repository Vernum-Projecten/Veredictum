- Seven development-only cases cover the ITEM_TAG semantics of the
  development RM on a COMPOSITION: a tag key with leading or trailing
  whitespace, an empty key and an empty value are each refused with 422 (the
  status rests on upstream report #750), against a control that stores the
  tag `k`/`v`; tagging a version leaves the latest `version_uid` unchanged;
  and deleting a tag on one COMPOSITION leaves the tag of the same key on
  another.
