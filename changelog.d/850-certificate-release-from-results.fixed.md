- A certificate's `sut.restapi_specs_version` is the release the run's
  `results.json` records under `server`, and absent when the server stated
  none. It used to come from the console's own identity read, so a certificate
  could state a release the run never recorded.
