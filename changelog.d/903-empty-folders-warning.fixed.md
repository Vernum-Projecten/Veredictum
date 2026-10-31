- `directory_create_refused/create_directory-folders_empty` records a warning,
  not a failure, when a server accepts a FOLDER whose `folders` list is empty.
  The refusal rests on the ITS-REST sentence that a request payload SHOULD be
  valid against the published JSON schemas, and the RM `Folders_valid`
  invariant it could otherwise rest on lacks the guard its 0..1 cardinality
  needs (upstream report #167).
