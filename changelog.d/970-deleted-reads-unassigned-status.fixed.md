- The reads of a deleted COMPOSITION, directory FOLDER or party, by
  versioned_object_uid without `version_at_time` or by the version_uid the
  delete created, no longer require `204`: the released
  `204_deleted_at_time.yaml` assigns it only to a read at a `version_at_time`.
  Each read must serve no data, so `204` or `404` passes, and its citation
  names upstream report #752.
