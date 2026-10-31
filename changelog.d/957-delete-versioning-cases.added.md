- Four catalogue cases on delete and versioning: a COMPOSITION delete
  addressed by its container's versioned_object_uid is refused with 400 and
  deletes nothing, the version a delete commits is identified within the
  deleted COMPOSITION's container, and an EHR_STATUS update adds a version
  that follows the one it names and records the change type
  251|modification|.
