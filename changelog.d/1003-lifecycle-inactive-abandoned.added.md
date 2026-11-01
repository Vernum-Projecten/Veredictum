- Nine development-only cases cover the version lifecycle states `inactive`
  (800) and `abandoned` (801) of the development RM: a COMPOSITION deactivated
  or abandoned through the `openehr-version` header of an update and through a
  CONTRIBUTION member, reactivated, retrieved from either state, and logically
  deleted from either state. Each transition must commit a new version, so
  every case reads the latest VERSION back and checks its state and its
  preceding version.
